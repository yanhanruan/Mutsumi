//! Pet state: energy, affection, computed mood + idle-animation variant.
//!
//! Mirrors the Python original's `state.py`:
//!   - Energy decays over time, refreshed by clicks.
//!   - Affection decays over time, increased by gentle drag.
//!   - Mood is computed from the two scalars.
//!   - IdleVariant selects which idle animation to play based on the three
//!     distinct low-status tiers (low energy only / low affection only / both).

use serde::{Deserialize, Serialize};

// ── Context actions ────────────────────────────────────────────────

/// Interactions available from the right-click context menu.
///
/// | Action        | Energy Δ | Affection Δ | Notes                          |
/// |---------------|----------|-------------|--------------------------------|
/// | PatHead       |    0     |    +5       | Pure affection                 |
/// | Feed          |  +15     |   +10       | Treat — restores both          |
/// | Sleep         |  +30     |    0        | Full rest — energy only        |
/// | FastLearning  |  -10     |   +15       | Hard work, but feels rewarding |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextAction {
    PatHead,
    Feed,
    Sleep,
    FastLearning,
}

// ── Thresholds ─────────────────────────────────────────────────────
/// Energy at or below this value is considered "low".
pub const LOW_ENERGY_THRESHOLD:    f32 = 30.0;
/// Affection at or below this value is considered "low".
pub const LOW_AFFECTION_THRESHOLD: f32 = 25.0;

// ── Mood ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    Happy,
    Content,
    Bored,
    Sad,
    Tired,
}

// ── IdleVariant ────────────────────────────────────────────────────

/// Determines which idle animation the frontend should play.
///
/// Three distinct low-status tiers (mirrored by three animation placeholders
/// in the frontend registry):
///
/// | Condition              | Variant      | Frontend animation     |
/// |------------------------|--------------|------------------------|
/// | energy ≤ 30, aff > 25  | LowEnergy    | `idle_low_energy`      |
/// | energy > 30, aff ≤ 25  | LowAffection | `idle_low_affection`   |
/// | energy ≤ 30, aff ≤ 25  | Exhausted    | `idle_exhausted`       |
/// | both > threshold       | Normal       | `idle`                 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleVariant {
    Normal,
    LowEnergy,
    LowAffection,
    Exhausted,
}

// ── PetState ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PetState {
    pub energy:       f32,
    pub affection:    f32,
    pub mood:         Mood,
    /// Which idle animation tier to show. Recomputed alongside `mood`.
    pub idle_variant: IdleVariant,
}

impl Default for PetState {
    fn default() -> Self {
        Self {
            energy:       100.0,
            affection:    100.0,
            mood:         Mood::Content,
            idle_variant: IdleVariant::Normal,
        }
    }
}

impl PetState {
    /// Recompute both `mood` and `idle_variant` from current energy/affection.
    pub fn recompute_mood(&mut self) {
        self.mood = match (self.energy, self.affection) {
            (e, _) if e < 20.0 => Mood::Tired,
            (_, a) if a > 75.0 => Mood::Happy,
            (e, _) if e < 40.0 => Mood::Bored,
            (_, a) if a < 25.0 => Mood::Sad,
            _                  => Mood::Content,
        };

        let low_e = self.energy    <= LOW_ENERGY_THRESHOLD;
        let low_a = self.affection <= LOW_AFFECTION_THRESHOLD;
        self.idle_variant = match (low_e, low_a) {
            (true,  true)  => IdleVariant::Exhausted,
            (true,  false) => IdleVariant::LowEnergy,
            (false, true)  => IdleVariant::LowAffection,
            (false, false) => IdleVariant::Normal,
        };
    }

    /// Called every 5 seconds by the ticker thread.
    /// Stats do not decay passively — only FastLearning (learning mode) costs energy.
    pub fn tick(&mut self) {
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

    pub fn on_context_action(&mut self, action: ContextAction) {
        match action {
            ContextAction::PatHead => {
                self.affection = (self.affection + 5.0).min(100.0);
            }
            ContextAction::Feed => {
                self.energy    = (self.energy    + 15.0).min(100.0);
                self.affection = (self.affection + 10.0).min(100.0);
            }
            ContextAction::Sleep => {
                self.energy = (self.energy + 30.0).min(100.0);
            }
            ContextAction::FastLearning => {
                self.energy    = (self.energy    - 10.0).max(0.0);
                self.affection = (self.affection + 15.0).min(100.0);
            }
        }
        self.recompute_mood();
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pet_with(energy: f32, affection: f32) -> PetState {
        let mut p = PetState { energy, affection, ..Default::default() };
        p.recompute_mood();
        p
    }

    // ── IdleVariant ───────────────────────────────────────────────

    #[test]
    fn idle_variant_normal_when_both_above_threshold() {
        let p = pet_with(80.0, 60.0);
        assert_eq!(p.idle_variant, IdleVariant::Normal);
    }

    #[test]
    fn idle_variant_low_energy_only() {
        let p = pet_with(LOW_ENERGY_THRESHOLD, 60.0);
        assert_eq!(p.idle_variant, IdleVariant::LowEnergy);
    }

    #[test]
    fn idle_variant_low_affection_only() {
        let p = pet_with(80.0, LOW_AFFECTION_THRESHOLD);
        assert_eq!(p.idle_variant, IdleVariant::LowAffection);
    }

    #[test]
    fn idle_variant_exhausted_when_both_low() {
        let p = pet_with(LOW_ENERGY_THRESHOLD, LOW_AFFECTION_THRESHOLD);
        assert_eq!(p.idle_variant, IdleVariant::Exhausted);
    }

    #[test]
    fn idle_variant_just_above_threshold_is_normal() {
        let p = pet_with(LOW_ENERGY_THRESHOLD + 0.1, LOW_AFFECTION_THRESHOLD + 0.1);
        assert_eq!(p.idle_variant, IdleVariant::Normal);
    }

    #[test]
    fn idle_variant_zero_energy_and_affection_is_exhausted() {
        let p = pet_with(0.0, 0.0);
        assert_eq!(p.idle_variant, IdleVariant::Exhausted);
    }

    // ── Mood ──────────────────────────────────────────────────────

    #[test]
    fn mood_tired_when_very_low_energy() {
        let p = pet_with(15.0, 50.0);
        assert_eq!(p.mood, Mood::Tired);
    }

    #[test]
    fn mood_happy_when_high_affection() {
        let p = pet_with(80.0, 80.0);
        assert_eq!(p.mood, Mood::Happy);
    }

    #[test]
    fn mood_sad_when_low_affection() {
        let p = pet_with(80.0, 20.0);
        assert_eq!(p.mood, Mood::Sad);
    }

    #[test]
    fn tick_does_not_change_energy_or_affection() {
        let mut p = PetState::default();
        p.tick();
        assert_eq!(p.energy,    100.0);
        assert_eq!(p.affection, 100.0);
    }

    #[test]
    fn tick_on_low_stats_leaves_them_unchanged() {
        let mut p = pet_with(5.0, 3.0);
        p.tick();
        assert!((p.energy    - 5.0).abs() < 0.001);
        assert!((p.affection - 3.0).abs() < 0.001);
    }

    #[test]
    fn on_click_raises_energy_and_affection() {
        let mut p = pet_with(50.0, 40.0);
        p.on_click();
        assert!((p.energy    - 52.0).abs() < 0.001);
        assert!((p.affection - 41.0).abs() < 0.001);
    }

    // ── ContextAction ─────────────────────────────────────────────

    #[test]
    fn pat_head_raises_affection_only() {
        let mut p = pet_with(50.0, 40.0);
        p.on_context_action(ContextAction::PatHead);
        assert!((p.energy    - 50.0).abs() < 0.001);
        assert!((p.affection - 45.0).abs() < 0.001);
    }

    #[test]
    fn feed_raises_both_energy_and_affection() {
        let mut p = pet_with(50.0, 40.0);
        p.on_context_action(ContextAction::Feed);
        assert!((p.energy    - 65.0).abs() < 0.001);
        assert!((p.affection - 50.0).abs() < 0.001);
    }

    #[test]
    fn sleep_raises_energy_only() {
        let mut p = pet_with(50.0, 40.0);
        p.on_context_action(ContextAction::Sleep);
        assert!((p.energy    - 80.0).abs() < 0.001);
        assert!((p.affection - 40.0).abs() < 0.001);
    }

    #[test]
    fn fast_learning_lowers_energy_raises_affection() {
        let mut p = pet_with(50.0, 40.0);
        p.on_context_action(ContextAction::FastLearning);
        assert!((p.energy    - 40.0).abs() < 0.001);
        assert!((p.affection - 55.0).abs() < 0.001);
    }

    #[test]
    fn context_actions_clamp_at_100() {
        let mut p = pet_with(95.0, 95.0);
        p.on_context_action(ContextAction::Feed);
        assert_eq!(p.energy,    100.0);
        assert_eq!(p.affection, 100.0);
    }

    #[test]
    fn fast_learning_clamps_energy_at_zero() {
        let mut p = pet_with(5.0, 40.0);
        p.on_context_action(ContextAction::FastLearning);
        assert_eq!(p.energy, 0.0);
    }

    #[test]
    fn context_action_recomputes_mood_and_variant() {
        // Start exhausted; feed should push out of exhausted.
        let mut p = pet_with(20.0, 15.0);
        assert_eq!(p.idle_variant, IdleVariant::Exhausted);
        p.on_context_action(ContextAction::Feed);
        // energy 20+15=35 > 30; affection 15+10=25 == threshold (≤ 25 is low)
        assert_eq!(p.idle_variant, IdleVariant::LowAffection);
    }
}
