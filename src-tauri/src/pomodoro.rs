//! Pomodoro timer state machine.
//!
//! Mirrors the Python original's `pomodoro.py`:
//!   - Phases: Idle (off), Focus (work), Break (rest)
//!   - Countdown in seconds; emits tick events for the UI badge
//!   - start() begins focus; on focus completion auto-transitions to break;
//!     on break completion auto-transitions back to focus (a "cycle").
//!
//! Design notes
//! ─────────────
//! • `remaining_secs` counts *remaining full seconds* to display.
//!   The sequence is: start at N, decrement each tick; when it reaches 0
//!   the timer shows "0:00" for one tick, then transitions on the NEXT tick.
//!   Total session length is therefore (focus_mins × 60 + 1) ticks, which
//!   is imperceptible in practice and lets the frontend show "0:00" cleanly.
//!
//! • `tick()` is called by the app_state ticker, which uses a fixed-step
//!   Instant schedule to avoid OS sleep drift accumulating over a session.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Focus,
    Break,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PomodoroState {
    pub phase:           Phase,
    pub focus_mins:      u32,
    pub break_mins:      u32,
    /// Remaining seconds to display. Zero while idle.
    pub remaining_secs:  u32,
    /// True when actively counting down (false when paused or idle).
    pub running:         bool,
}

impl Default for PomodoroState {
    fn default() -> Self {
        Self {
            phase:          Phase::Idle,
            focus_mins:     25,
            break_mins:     5,
            remaining_secs: 0,
            running:        false,
        }
    }
}

impl PomodoroState {
    /// Start or resume the timer.
    ///
    /// * If **Idle**: initialises a new Focus session.
    /// * If **paused** (Focus or Break, running=false): resumes countdown.
    /// * If already **running**: no-op (idempotent).
    pub fn start(&mut self) {
        if self.running {
            return; // already running — no double-start
        }
        if self.phase == Phase::Idle {
            self.phase = Phase::Focus;
            self.remaining_secs = self.focus_mins * 60;
        }
        self.running = true;
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    /// Stop and reset to Idle, clearing remaining time.
    pub fn stop(&mut self) {
        self.phase          = Phase::Idle;
        self.running        = false;
        self.remaining_secs = 0;
    }

    /// Called every second by the ticker thread.
    ///
    /// Returns `Some(new_phase)` if a phase transition just happened so the
    /// caller can emit a `pomodoro-phase-change` event and play a sound.
    pub fn tick(&mut self) -> Option<Phase> {
        if !self.running || self.phase == Phase::Idle {
            return None;
        }

        // If remaining > 0, count down and stay in the same phase.
        if self.remaining_secs > 0 {
            self.remaining_secs -= 1;
            return None;
        }

        // remaining_secs == 0 — the display has shown "0:00" for one tick;
        // now perform the phase transition.
        let new_phase = match self.phase {
            Phase::Focus => Phase::Break,
            Phase::Break => Phase::Focus,
            Phase::Idle  => return None,
        };
        self.phase          = new_phase;
        self.remaining_secs = match new_phase {
            Phase::Focus => self.focus_mins  * 60,
            Phase::Break => self.break_mins  * 60,
            Phase::Idle  => 0,
        };
        Some(new_phase)
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_n(p: &mut PomodoroState, n: u32) -> Vec<Phase> {
        (0..n).filter_map(|_| p.tick()).collect()
    }

    // ── start / pause / stop ──────────────────────────────────────

    #[test]
    fn default_state_is_idle_not_running() {
        let p = PomodoroState::default();
        assert_eq!(p.phase, Phase::Idle);
        assert!(!p.running);
        assert_eq!(p.remaining_secs, 0);
    }

    #[test]
    fn start_from_idle_enters_focus() {
        let mut p = PomodoroState::default();
        p.start();
        assert_eq!(p.phase, Phase::Focus);
        assert!(p.running);
        assert_eq!(p.remaining_secs, 25 * 60);
    }

    #[test]
    fn start_is_idempotent_when_already_running() {
        let mut p = PomodoroState::default();
        p.start();
        let before = p.remaining_secs;
        tick_n(&mut p, 3);
        let mid = p.remaining_secs;
        p.start(); // should NOT reset remaining
        assert_eq!(p.remaining_secs, mid);
        assert!(before > mid, "remaining should have decreased");
    }

    #[test]
    fn pause_stops_countdown() {
        let mut p = PomodoroState::default();
        p.start();
        tick_n(&mut p, 5);
        p.pause();
        let frozen = p.remaining_secs;
        tick_n(&mut p, 10);
        assert_eq!(p.remaining_secs, frozen, "paused timer must not change");
    }

    #[test]
    fn resume_continues_from_where_it_paused() {
        let mut p = PomodoroState::default();
        p.start();
        tick_n(&mut p, 10);
        p.pause();
        let r = p.remaining_secs;
        p.start(); // resume
        tick_n(&mut p, 3);
        assert_eq!(p.remaining_secs, r - 3);
    }

    #[test]
    fn stop_resets_to_idle() {
        let mut p = PomodoroState::default();
        p.start();
        tick_n(&mut p, 5);
        p.stop();
        assert_eq!(p.phase, Phase::Idle);
        assert!(!p.running);
        assert_eq!(p.remaining_secs, 0);
    }

    #[test]
    fn stop_then_start_begins_fresh_session() {
        let mut p = PomodoroState::default();
        p.start();
        tick_n(&mut p, 30);
        p.stop();
        p.start();
        assert_eq!(p.remaining_secs, 25 * 60);
        assert_eq!(p.phase, Phase::Focus);
    }

    // ── Countdown & phase transition ──────────────────────────────

    #[test]
    fn countdown_decrements_each_tick() {
        let mut p = PomodoroState::default();
        p.start();
        assert_eq!(p.remaining_secs, 1500);
        tick_n(&mut p, 1);
        assert_eq!(p.remaining_secs, 1499);
        tick_n(&mut p, 9);
        assert_eq!(p.remaining_secs, 1490);
    }

    #[test]
    fn focus_transitions_to_break_after_full_countdown() {
        let mut p = PomodoroState { focus_mins: 1, break_mins: 5, ..Default::default() };
        p.start();
        // 60 ticks → remaining hits 0, shows "0:00"
        let events = tick_n(&mut p, 60);
        assert_eq!(events, vec![], "no transition yet at remaining=0");
        assert_eq!(p.remaining_secs, 0);
        // 1 more tick → transition fires
        let tr = p.tick();
        assert_eq!(tr, Some(Phase::Break));
        assert_eq!(p.phase, Phase::Break);
        assert_eq!(p.remaining_secs, 5 * 60);
    }

    #[test]
    fn break_transitions_back_to_focus() {
        let mut p = PomodoroState { focus_mins: 1, break_mins: 1, ..Default::default() };
        p.start();
        tick_n(&mut p, 60); // remaining = 0 (still Focus)
        p.tick();           // → Break, remaining = 60
        assert_eq!(p.phase, Phase::Break);
        tick_n(&mut p, 60); // remaining = 0 (still Break)
        let tr = p.tick();  // → Focus
        assert_eq!(tr, Some(Phase::Focus));
        assert_eq!(p.phase, Phase::Focus);
        assert_eq!(p.remaining_secs, 60);
    }

    #[test]
    fn no_spurious_transitions_while_counting() {
        let mut p = PomodoroState { focus_mins: 1, break_mins: 1, ..Default::default() };
        p.start();
        let events = tick_n(&mut p, 59); // 1 second before reaching 0
        assert_eq!(events, vec![]);
        assert_eq!(p.remaining_secs, 1);
    }

    #[test]
    fn custom_durations_are_respected() {
        let mut p = PomodoroState { focus_mins: 2, break_mins: 3, ..Default::default() };
        p.start();
        assert_eq!(p.remaining_secs, 120);
        tick_n(&mut p, 120);
        p.tick(); // transition
        assert_eq!(p.phase, Phase::Break);
        assert_eq!(p.remaining_secs, 180);
    }

    #[test]
    fn set_durations_affects_next_session() {
        let mut p = PomodoroState::default();
        p.focus_mins = 10;
        p.start();
        assert_eq!(p.remaining_secs, 600);
    }
}
