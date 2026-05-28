//! Pomodoro timer state machine.
//!
//! Mirrors the Python original's `pomodoro.py`:
//!   - Phases: Idle (off), Focus (work), Break (rest)
//!   - Countdown in seconds; emits tick events for the UI badge
//!   - start() begins focus; on focus completion auto-transitions to break;
//!     on break completion auto-transitions back to focus (a "cycle").

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
    /// Remaining seconds in the current phase. Zero when idle.
    pub remaining_secs:  u32,
    /// True when actively counting down.
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
    pub fn start(&mut self) {
        if self.phase == Phase::Idle {
            self.phase = Phase::Focus;
            self.remaining_secs = self.focus_mins * 60;
        }
        self.running = true;
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    pub fn stop(&mut self) {
        self.phase = Phase::Idle;
        self.running = false;
        self.remaining_secs = 0;
    }

    /// Called every second by the ticker thread.
    /// Returns Some(new_phase) if a phase transition happened, so the caller
    /// can emit an event.
    pub fn tick(&mut self) -> Option<Phase> {
        if !self.running || self.phase == Phase::Idle {
            return None;
        }
        if self.remaining_secs > 0 {
            self.remaining_secs -= 1;
            return None;
        }
        // Phase complete — flip Focus <-> Break.
        let new_phase = match self.phase {
            Phase::Focus => Phase::Break,
            Phase::Break => Phase::Focus,
            Phase::Idle  => return None,
        };
        self.phase = new_phase;
        self.remaining_secs = match new_phase {
            Phase::Focus => self.focus_mins * 60,
            Phase::Break => self.break_mins * 60,
            Phase::Idle  => 0,
        };
        Some(new_phase)
    }
}
