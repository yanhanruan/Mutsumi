//! Pet state: energy, affection, computed mood.
//!
//! Mirrors the Python original's `state.py`:
//!   - Energy decays over time, refreshed by clicks.
//!   - Affection decays over time, increased by gentle drag (and decreased
//!     by rough drag, in the original; we keep "rough" notion out for now).
//!   - Mood is computed from the two scalars.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    Happy,
    Content,
    Bored,
    Sad,
    Tired,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PetState {
    pub energy:    f32,
    pub affection: f32,
    pub mood:      Mood,
}

impl Default for PetState {
    fn default() -> Self {
        Self {
            energy:    100.0,
            affection: 50.0,
            mood:      Mood::Content,
        }
    }
}

impl PetState {
    /// Recompute the mood from the current energy/affection values.
    pub fn recompute_mood(&mut self) {
        self.mood = match (self.energy, self.affection) {
            (e, _) if e < 20.0           => Mood::Tired,
            (_, a) if a > 75.0           => Mood::Happy,
            (e, _) if e < 40.0           => Mood::Bored,
            (_, a) if a < 25.0           => Mood::Sad,
            _                            => Mood::Content,
        };
    }

    /// Called every 5 seconds by the ticker thread.
    /// Energy and affection both decay slowly.
    pub fn tick(&mut self) {
        self.energy    = (self.energy    - 0.8).max(0.0);
        self.affection = (self.affection - 0.3).max(0.0);
        self.recompute_mood();
    }

    pub fn on_click(&mut self) {
        self.energy    = (self.energy    + 2.0).min(100.0);
        self.affection = (self.affection + 1.0).min(100.0);
        self.recompute_mood();
    }

    pub fn on_drag_release(&mut self, rough: bool) {
        if rough {
            self.affection = (self.affection - 5.0).max(0.0);
        } else {
            self.affection = (self.affection + 2.0).min(100.0);
        }
        self.recompute_mood();
    }
}
