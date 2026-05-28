//! Late-night reminder logic.
//!
//! Fires `late-night-reminder` once per night when the current local hour
//! is inside the LATE_WINDOW (default: 00:00–04:59). "Once per night" means:
//! the reminder fires the first time we observe a late-night hour after
//! having previously observed a non-late-night hour. Re-armed when the user
//! leaves the window (hour ≥ 5).
//!
//! The pure state machine `LateNightReminder` is exposed so it can be
//! unit-tested by feeding it synthetic hour values.

/// Default late-night window: hours strictly less than this count as late.
pub const LATE_WINDOW_END_HOUR: u32 = 5;

#[derive(Debug, Default)]
pub struct LateNightReminder {
    /// True if we've fired the reminder during the current late-night session.
    /// Resets to false when an out-of-window hour is observed.
    fired_this_session: bool,
}

impl LateNightReminder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the current local hour (0..24). Returns true iff the reminder
    /// should fire now.
    pub fn observe(&mut self, hour: u32) -> bool {
        let in_late = hour < LATE_WINDOW_END_HOUR;
        if !in_late {
            // Out of the window — re-arm for the next night.
            self.fired_this_session = false;
            return false;
        }
        // In window — fire iff we haven't fired yet this session.
        if self.fired_this_session {
            return false;
        }
        self.fired_this_session = true;
        true
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_entering_window_at_midnight() {
        let mut r = LateNightReminder::new();
        assert!(r.observe(0));
    }

    #[test]
    fn does_not_fire_outside_window() {
        let mut r = LateNightReminder::new();
        for h in 5..24 {
            assert!(!r.observe(h), "fired at hour {}", h);
        }
    }

    #[test]
    fn fires_only_once_during_continuous_late_session() {
        let mut r = LateNightReminder::new();
        assert!(r.observe(0));   // first late observation — fire
        assert!(!r.observe(0));  // still late, same session — no
        assert!(!r.observe(1));  // still late — no
        assert!(!r.observe(2));  // still late — no
        assert!(!r.observe(3));  // still late — no
        assert!(!r.observe(4));  // still late — no
    }

    #[test]
    fn rearms_after_leaving_window() {
        let mut r = LateNightReminder::new();
        assert!(r.observe(2));    // fire
        assert!(!r.observe(3));   // already fired
        assert!(!r.observe(8));   // out of window — rearm
        assert!(!r.observe(20));  // still out
        assert!(r.observe(0));    // back in window the next night → fire again
    }

    #[test]
    fn observing_pre_midnight_does_not_arm_or_fire() {
        let mut r = LateNightReminder::new();
        assert!(!r.observe(23));  // late evening but not in window
        assert!(r.observe(0));    // crosses midnight
    }

    #[test]
    fn fires_at_any_in_window_hour_for_a_fresh_tracker() {
        for entry_hour in 0..LATE_WINDOW_END_HOUR {
            let mut r = LateNightReminder::new();
            assert!(r.observe(entry_hour), "should fire at entry hour {}", entry_hour);
        }
    }

    #[test]
    fn no_fire_at_boundary_hour_5() {
        let mut r = LateNightReminder::new();
        assert!(!r.observe(5));
    }
}
