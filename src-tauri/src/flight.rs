//! flight.rs — balloon-mode window flight controller.
//!
//! # Why the window moves, not the sprite
//! The pet window is only ~170×289 logical px (tauri.conf.json). The first
//! balloon implementation animated the sprite *inside* the webview, so a
//! 120 px sprite had ~50 px of horizontal travel: it hit a "wall" every few
//! frames, re-randomized its direction each time, and looked like erratic
//! jitter inside a tiny box. The sprite now stays put filling the webview
//! and this module glides the whole OS window across the monitor work area.
//!
//! # Design
//! `set_active(app, true)` spawns a movement thread; `set_active(app, false)`
//! stops it and restores the window to where the user had it. The thread:
//!   - samples the window's monitor work area once at start,
//!   - advances a slow, mostly-horizontal velocity every TICK_MS, with a
//!     gentle sine bob overlaid on the vertical axis for a balloon-like feel,
//!   - bounces off the work-area edges (pure `step_flight`, unit-tested),
//!   - emits `balloon-facing { facing: "left"|"right" }` at start and on every
//!     horizontal bounce so the frontend can mirror the left-only frames.
//!
//! Window movement uses Tauri's `WebviewWindow::set_position`, which is safe
//! to call from any thread (it dispatches to the event loop). Only position
//! changes — no size — so the atomic SetWindowPos path in window_ops.rs is
//! not needed here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

// ── Configuration ──────────────────────────────────────────────────

/// Cruise speed in physical px/sec. Deliberately slow — balloon drift, not a
/// screensaver bounce. (The old in-webview mover ran ~90 CSS px/sec.)
const SPEED_PX_PER_SEC: f64 = 40.0;

/// Maximum vertical share of the cruise velocity (keeps flight mostly
/// horizontal so the left/right frames stay believable).
const MAX_VERTICAL_RATIO: f64 = 0.35;

/// Gentle vertical bob overlaid on the linear velocity: amplitude of the
/// extra vy in px/sec, and the period of one full bob cycle.
const BOB_AMP_PX_PER_SEC: f64 = 12.0;
const BOB_PERIOD_SECS: f64 = 6.0;

/// Movement tick. ~60 Hz keeps the glide smooth; set_position is cheap.
const TICK_MS: u64 = 16;

// ── Event payload ──────────────────────────────────────────────────

#[derive(Serialize, Clone, Copy)]
struct FacingPayload {
    /// "left" or "right" — which way the sprite should face.
    facing: &'static str,
}

fn facing_of(vx: f64) -> &'static str {
    if vx < 0.0 { "left" } else { "right" }
}

// ── Pure step function (unit-testable) ─────────────────────────────

/// Result of advancing the flight by one tick.
pub struct FlightStep {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    /// True when the horizontal direction reversed this tick (edge bounce) —
    /// the frontend must mirror the sprite.
    pub flipped_x: bool,
}

/// Advances position by velocity (plus a vertical bob term), clamps to the
/// work-area rectangle and reflects velocity off any edge hit.
///
/// `bounds` is (left, top, right, bottom) of the work area; `win` is the
/// window's (width, height). All physical px. Pure — no OS calls.
pub fn step_flight(
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    dt_secs: f64,
    bob_vy: f64,
    bounds: (f64, f64, f64, f64),
    win: (f64, f64),
) -> FlightStep {
    let (left, top, right, bottom) = bounds;
    let (w, h) = win;

    let mut nx = x + vx * dt_secs;
    let mut ny = y + (vy + bob_vy) * dt_secs;
    let mut nvx = vx;
    let mut nvy = vy;
    let mut flipped_x = false;

    if nx <= left {
        nx = left;
        if nvx < 0.0 {
            nvx = -nvx;
            flipped_x = true;
        }
    } else if nx + w >= right {
        nx = right - w;
        if nvx > 0.0 {
            nvx = -nvx;
            flipped_x = true;
        }
    }

    if ny <= top {
        ny = top;
        nvy = nvy.abs();
    } else if ny + h >= bottom {
        ny = bottom - h;
        nvy = -nvy.abs();
    }

    FlightStep { x: nx, y: ny, vx: nvx, vy: nvy, flipped_x }
}

// ── Pixel-perfect collision insets ─────────────────────────────────

/// Transparent margins around the sprite's opaque pixels, as FRACTIONS of the
/// window size (0.0–1.0, DPI-independent). Reported by the frontend, which
/// alpha-scans the flying frames once (src/composables/useFlightCollision.ts).
/// Left/right describe the un-mirrored (left-facing) sprite; the flight loop
/// swaps them while she faces right (the sprite is mirrored with scaleX(-1)).
/// Zero insets — the default until the frontend reports — fall back to
/// window-rect collision.
#[derive(Clone, Copy, Default)]
struct Insets {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

fn insets_slot() -> &'static Mutex<Insets> {
    static SLOT: OnceLock<Mutex<Insets>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Insets::default()))
}

/// Frontend-reported sprite margins. Each value is the transparent gap
/// between a window edge and the nearest opaque pixel, as a fraction of the
/// window size. Clamped defensively — a bogus report must never make the
/// collision rect degenerate.
#[tauri::command]
pub fn flight_set_insets(left: f64, top: f64, right: f64, bottom: f64) {
    let c = |v: f64| if v.is_finite() { v.clamp(0.0, 0.45) } else { 0.0 };
    *insets_slot().lock().unwrap() = Insets {
        left: c(left),
        top: c(top),
        right: c(right),
        bottom: c(bottom),
    };
}

/// Expands the work-area bounds by the sprite's transparent margins so that
/// `step_flight`'s window-rect collision fires exactly when an opaque pixel
/// reaches the real work-area edge. `facing_right` swaps the horizontal
/// margins because the sprite is mirrored while flying right.
///
/// `insets` is (left, top, right, bottom) as fractions of the window size.
/// Pure — no OS calls.
pub fn effective_bounds(
    bounds: (f64, f64, f64, f64),
    win: (f64, f64),
    insets: (f64, f64, f64, f64),
    facing_right: bool,
) -> (f64, f64, f64, f64) {
    let (l, t, r, b) = bounds;
    let (w, h) = win;
    let (mut il, it, mut ir, ib) = (insets.0 * w, insets.1 * h, insets.2 * w, insets.3 * h);
    if facing_right {
        std::mem::swap(&mut il, &mut ir);
    }
    (l - il, t - it, r + ir, b + ib)
}

// ── Controller (start / stop) ──────────────────────────────────────

/// Stop flag of the currently running flight thread, if any.
fn slot() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Starts or stops the flight thread. Called by the idle monitor on every
/// balloon-mode transition; only reachable on Windows today, hence the
/// dead-code allowance for other targets.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn set_active(app: &AppHandle, active: bool) {
    let mut guard = slot().lock().unwrap();

    // Stop any running flight first (also the "deactivate" path — the thread
    // restores the window's original position on its way out).
    if let Some(stop) = guard.take() {
        stop.store(true, Ordering::Relaxed);
    }

    if active {
        let stop = Arc::new(AtomicBool::new(false));
        *guard = Some(stop.clone());
        let app = app.clone();
        thread::spawn(move || run_flight(app, stop));
    }
}

// ── Flight thread ──────────────────────────────────────────────────

fn run_flight(app: AppHandle, stop: Arc<AtomicBool>) {
    let Some(window) = app.get_webview_window("main") else { return };
    let Ok(origin) = window.outer_position() else { return };
    let Ok(size) = window.outer_size() else { return };

    // Work area of the monitor the window currently sits on (excludes the
    // taskbar). Fall back to a no-op if the monitor can't be resolved.
    let Ok(Some(monitor)) = window.current_monitor() else { return };
    let area = monitor.work_area();
    let bounds = (
        area.position.x as f64,
        area.position.y as f64,
        (area.position.x + area.size.width as i32) as f64,
        (area.position.y + area.size.height as i32) as f64,
    );
    let win = (size.width as f64, size.height as f64);

    // Initial velocity: fly left (the native frame orientation) with a small
    // pseudo-random vertical drift. No rand crate — clock nanos are plenty.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let drift = (seed % 1000) as f64 / 1000.0 * 2.0 - 1.0; // [-1, 1)
    let mut vy = SPEED_PX_PER_SEC * MAX_VERTICAL_RATIO * drift;
    let mut vx = -(SPEED_PX_PER_SEC * SPEED_PX_PER_SEC - vy * vy).sqrt();

    let mut x = origin.x as f64;
    let mut y = origin.y as f64;
    let (mut last_ix, mut last_iy) = (origin.x, origin.y);

    let _ = app.emit("balloon-facing", FacingPayload { facing: facing_of(vx) });

    let dt = TICK_MS as f64 / 1000.0;
    let mut t = 0.0_f64;

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(TICK_MS));
        t += dt;

        let bob = BOB_AMP_PX_PER_SEC
            * (t * std::f64::consts::TAU / BOB_PERIOD_SECS).sin();
        // Bounce on the sprite's opaque pixels, not the window rect: widen
        // the bounds by the transparent margins the frontend reported.
        let ins = *insets_slot().lock().unwrap();
        let eff = effective_bounds(
            bounds,
            win,
            (ins.left, ins.top, ins.right, ins.bottom),
            vx > 0.0,
        );
        let s = step_flight(x, y, vx, vy, dt, bob, eff, win);
        x = s.x;
        y = s.y;
        vx = s.vx;
        vy = s.vy;

        if s.flipped_x {
            let _ = app.emit("balloon-facing", FacingPayload { facing: facing_of(vx) });
        }

        // Only touch the OS window when the integer position actually moved.
        let (ix, iy) = (x.round() as i32, y.round() as i32);
        if (ix, iy) != (last_ix, last_iy) {
            let _ = window.set_position(PhysicalPosition::new(ix, iy));
            (last_ix, last_iy) = (ix, iy);
        }
    }

    // Flight over — put the window back where the user had it.
    let _ = window.set_position(origin);
}

// ── Unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1040.0);
    const WIN: (f64, f64) = (170.0, 289.0);

    #[test]
    fn free_flight_advances_by_velocity() {
        let s = step_flight(500.0, 400.0, -40.0, 10.0, 1.0, 0.0, BOUNDS, WIN);
        assert_eq!(s.x, 460.0);
        assert_eq!(s.y, 410.0);
        assert_eq!(s.vx, -40.0);
        assert_eq!(s.vy, 10.0);
        assert!(!s.flipped_x);
    }

    #[test]
    fn bob_term_adds_to_vertical_motion_only() {
        let s = step_flight(500.0, 400.0, -40.0, 0.0, 1.0, 12.0, BOUNDS, WIN);
        assert_eq!(s.x, 460.0);
        assert_eq!(s.y, 412.0);
        // Bob affects position, not the persistent velocity.
        assert_eq!(s.vy, 0.0);
    }

    #[test]
    fn left_edge_bounces_and_flips_facing() {
        let s = step_flight(5.0, 400.0, -40.0, 0.0, 1.0, 0.0, BOUNDS, WIN);
        assert_eq!(s.x, 0.0);
        assert_eq!(s.vx, 40.0);
        assert!(s.flipped_x);
    }

    #[test]
    fn right_edge_bounces_and_flips_facing() {
        let s = step_flight(1740.0, 400.0, 40.0, 0.0, 1.0, 0.0, BOUNDS, WIN);
        assert_eq!(s.x, 1920.0 - 170.0);
        assert_eq!(s.vx, -40.0);
        assert!(s.flipped_x);
    }

    #[test]
    fn top_edge_bounces_without_horizontal_flip() {
        let s = step_flight(500.0, 3.0, -40.0, -10.0, 1.0, 0.0, BOUNDS, WIN);
        assert_eq!(s.y, 0.0);
        assert_eq!(s.vy, 10.0);
        assert!(!s.flipped_x);
    }

    #[test]
    fn bottom_edge_bounces_without_horizontal_flip() {
        let s = step_flight(500.0, 745.0, -40.0, 10.0, 1.0, 0.0, BOUNDS, WIN);
        assert_eq!(s.y, 1040.0 - 289.0);
        assert_eq!(s.vy, -10.0);
        assert!(!s.flipped_x);
    }

    #[test]
    fn corner_hit_flips_both_axes() {
        let s = step_flight(3.0, 3.0, -40.0, -10.0, 1.0, 0.0, BOUNDS, WIN);
        assert_eq!((s.x, s.y), (0.0, 0.0));
        assert_eq!((s.vx, s.vy), (40.0, 10.0));
        assert!(s.flipped_x);
    }

    #[test]
    fn resting_on_left_edge_moving_right_does_not_reflip() {
        // Already clamped to the edge but heading away — no bounce, no event.
        let s = step_flight(0.0, 400.0, 40.0, 0.0, 0.0, 0.0, BOUNDS, WIN);
        assert_eq!(s.vx, 40.0);
        assert!(!s.flipped_x);
    }

    #[test]
    fn work_area_with_nonzero_origin_is_respected() {
        // Secondary monitor to the left: negative-x work area.
        let bounds = (-1920.0, 0.0, 0.0, 1040.0);
        let s = step_flight(-1918.0, 400.0, -40.0, 0.0, 1.0, 0.0, bounds, WIN);
        assert_eq!(s.x, -1920.0);
        assert!(s.flipped_x);
    }

    #[test]
    fn speed_is_slow_and_mostly_horizontal() {
        assert!(SPEED_PX_PER_SEC <= 60.0, "flight should stay slow and natural");
        assert!(MAX_VERTICAL_RATIO < 0.5, "flight should stay mostly horizontal");
    }

    // ── effective_bounds (pixel-perfect collision margins) ─────────

    #[test]
    fn zero_insets_leave_bounds_unchanged() {
        let eff = effective_bounds(BOUNDS, WIN, (0.0, 0.0, 0.0, 0.0), false);
        assert_eq!(eff, BOUNDS);
    }

    #[test]
    fn insets_widen_bounds_by_window_scaled_margins() {
        // 10% left margin of a 170-wide window = 17 px of transparent gap:
        // the window may overshoot the left edge by exactly that much before
        // an opaque pixel touches it.
        let eff = effective_bounds(BOUNDS, WIN, (0.1, 0.1, 0.2, 0.2), false);
        assert_eq!(eff.0, 0.0 - 0.1 * 170.0);
        assert_eq!(eff.1, 0.0 - 0.1 * 289.0);
        assert_eq!(eff.2, 1920.0 + 0.2 * 170.0);
        assert_eq!(eff.3, 1040.0 + 0.2 * 289.0);
    }

    #[test]
    fn facing_right_swaps_horizontal_insets_only() {
        let left  = effective_bounds(BOUNDS, WIN, (0.1, 0.05, 0.2, 0.15), false);
        let right = effective_bounds(BOUNDS, WIN, (0.1, 0.05, 0.2, 0.15), true);
        // Horizontal margins swap (the sprite is mirrored)…
        assert_eq!(right.0, 0.0 - 0.2 * 170.0);
        assert_eq!(right.2, 1920.0 + 0.1 * 170.0);
        // …vertical margins do not.
        assert_eq!(right.1, left.1);
        assert_eq!(right.3, left.3);
    }

    #[test]
    fn opaque_pixel_bounce_happens_past_the_window_edge() {
        // Sprite with a 10% transparent left margin (17 px of a 170-wide
        // window) flying left: the window may cross x=0 and only bounces
        // once the opaque pixels arrive at the work-area edge.
        let eff = effective_bounds(BOUNDS, WIN, (0.1, 0.0, 0.0, 0.0), false);
        let past_window_edge = step_flight(-3.0, 400.0, -40.0, 0.0, 0.1, 0.0, eff, WIN);
        assert!(!past_window_edge.flipped_x, "no bounce while only transparent px overhang");
        assert_eq!(past_window_edge.x, -7.0);
        let at_opaque_edge = step_flight(-14.0, 400.0, -40.0, 0.0, 0.1, 0.0, eff, WIN);
        assert!(at_opaque_edge.flipped_x, "bounce once the opaque box reaches the edge");
        assert_eq!(at_opaque_edge.x, -17.0); // clamped so opaque pixels sit on x=0
    }
}
