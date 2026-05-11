use crate::robomaster::power_rune::common::{
    RUNE_TARGET_COUNT, RuneHitOutcome, RuneMode, RuneTransition,
};
use crate::robomaster::power_rune::consts::{
    ACTIVATED_HOLD, ACTIVATION_GLOBAL_TIMEOUT, ACTIVATION_PRIMARY_TIMEOUT, FAILURE_RECOVER,
    INACTIVE_WAIT, LARGE_SECONDARY_TIMEOUT,
};
use crate::robomaster::visibility::Activation;
use rand::Rng;
use rand::prelude::SliceRandom;

pub type RuneTargetStates = [Activation; RUNE_TARGET_COUNT];

#[derive(Debug, Clone, PartialEq)]
pub enum MechanismState {
    Inactive { remaining: f32 },
    Activating(ActivationRun),
    Activated { remaining: f32 },
    Failed { remaining: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivationRun {
    global_remaining: f32,
    targets: RuneTargetStates,
    round: ActivationRound,
}

#[derive(Debug, Clone, PartialEq)]
enum ActivationRound {
    Small(SmallRound),
    Large(LargeRound),
}

#[derive(Debug, Clone, PartialEq)]
struct SmallRound {
    primary_remaining: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum LargeRound {
    Primary {
        primary_remaining: f32,
    },
    Secondary {
        secondary_remaining: f32,
        target: Option<usize>,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum RunTransition {
    None,
    Advanced,
    Failed,
    Activated,
}

impl MechanismState {
    pub fn inactive() -> Self {
        Self::Inactive {
            remaining: INACTIVE_WAIT,
        }
    }

    pub fn start(mode: RuneMode, rng: &mut impl Rng) -> Self {
        Self::Activating(ActivationRun::new(mode, rng))
    }

    pub fn tick(&mut self, mode: RuneMode, delta_secs: f32, rng: &mut impl Rng) -> RuneTransition {
        let delta_secs = delta_secs.max(0.0);
        let mut next = None;

        let transition = match self {
            Self::Inactive { remaining } => {
                if expire_after(remaining, delta_secs) {
                    next = Some(Self::start(mode, rng));
                    RuneTransition::Started
                } else {
                    RuneTransition::None
                }
            }
            Self::Activating(run) => match run.tick(delta_secs, rng) {
                RunTransition::None => RuneTransition::None,
                RunTransition::Advanced => RuneTransition::Advanced,
                RunTransition::Failed => {
                    next = Some(Self::failed());
                    RuneTransition::Failed
                }
                RunTransition::Activated => {
                    next = Some(Self::activated());
                    RuneTransition::Activated
                }
            },
            Self::Activated { remaining } => {
                if expire_after(remaining, delta_secs) {
                    next = Some(Self::inactive());
                    RuneTransition::ResetToInactive
                } else {
                    RuneTransition::None
                }
            }
            Self::Failed { remaining } => {
                if expire_after(remaining, delta_secs) {
                    next = Some(Self::inactive());
                    RuneTransition::ResetToInactive
                } else {
                    RuneTransition::None
                }
            }
        };

        if let Some(state) = next {
            *self = state;
        }

        transition
    }

    pub fn hit(
        &mut self,
        target_index: usize,
        _mode: RuneMode,
        rng: &mut impl Rng,
    ) -> RuneHitOutcome {
        let Self::Activating(run) = self else {
            return RuneHitOutcome::Ignored;
        };

        match run.hit(target_index, rng) {
            RuneHitOutcome::WrongTarget => {
                const FUNNY: bool = true;
                if !FUNNY {
                    *self = Self::failed();
                }
                RuneHitOutcome::WrongTarget
            }
            RuneHitOutcome::Activated => {
                *self = Self::activated();
                RuneHitOutcome::Activated
            }
            outcome => outcome,
        }
    }

    pub fn target_states(&self) -> RuneTargetStates {
        match self {
            Self::Inactive { .. } | Self::Failed { .. } => {
                [Activation::Deactivated; RUNE_TARGET_COUNT]
            }
            Self::Activating(run) => run.targets,
            Self::Activated { .. } => [Activation::Completed; RUNE_TARGET_COUNT],
        }
    }

    pub fn root_activation(&self) -> Activation {
        match self {
            Self::Inactive { .. } | Self::Failed { .. } => Activation::Deactivated,
            Self::Activating(_) | Self::Activated { .. } => Activation::Activated,
        }
    }

    fn activated() -> Self {
        Self::Activated {
            remaining: ACTIVATED_HOLD,
        }
    }

    fn failed() -> Self {
        Self::Failed {
            remaining: FAILURE_RECOVER,
        }
    }
}

impl ActivationRun {
    fn new(mode: RuneMode, rng: &mut impl Rng) -> Self {
        let mut run = Self {
            global_remaining: ACTIVATION_GLOBAL_TIMEOUT,
            targets: [Activation::Deactivated; RUNE_TARGET_COUNT],
            round: match mode {
                RuneMode::Small => ActivationRound::Small(SmallRound {
                    primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
                }),
                RuneMode::Large => ActivationRound::Large(LargeRound::Primary {
                    primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
                }),
            },
        };
        run.start_round(mode, rng);
        run
    }

    pub fn target_states(&self) -> RuneTargetStates {
        self.targets
    }

    fn tick(&mut self, delta_secs: f32, rng: &mut impl Rng) -> RunTransition {
        if expire_after(&mut self.global_remaining, delta_secs) {
            return RunTransition::Failed;
        }

        match &mut self.round {
            ActivationRound::Small(round) => {
                if expire_after(&mut round.primary_remaining, delta_secs) {
                    RunTransition::Failed
                } else {
                    RunTransition::None
                }
            }
            ActivationRound::Large(LargeRound::Primary { primary_remaining }) => {
                if expire_after(primary_remaining, delta_secs) {
                    RunTransition::Failed
                } else {
                    RunTransition::None
                }
            }
            ActivationRound::Large(LargeRound::Secondary {
                secondary_remaining,
                ..
            }) => {
                if expire_after(secondary_remaining, delta_secs) {
                    self.advance_large_round(rng)
                } else {
                    RunTransition::None
                }
            }
        }
    }

    fn hit(&mut self, target_index: usize, rng: &mut impl Rng) -> RuneHitOutcome {
        if target_index >= RUNE_TARGET_COUNT {
            return RuneHitOutcome::WrongTarget;
        }

        match &self.round {
            ActivationRound::Small(_) => self.hit_small(target_index, rng),
            ActivationRound::Large(LargeRound::Primary { .. }) => {
                self.hit_large_primary(target_index, rng)
            }
            ActivationRound::Large(LargeRound::Secondary { target, .. }) => {
                self.hit_large_secondary(target_index, *target, rng)
            }
        }
    }

    fn hit_small(&mut self, target_index: usize, rng: &mut impl Rng) -> RuneHitOutcome {
        if self.targets[target_index] != Activation::Activating {
            return RuneHitOutcome::WrongTarget;
        }

        self.targets[target_index] = Activation::Activated;
        if self.all_targets_activated() {
            RuneHitOutcome::Activated
        } else {
            self.start_small_round(rng);
            RuneHitOutcome::PrimaryHit
        }
    }

    fn hit_large_primary(&mut self, target_index: usize, _rng: &mut impl Rng) -> RuneHitOutcome {
        if self.targets[target_index] != Activation::Activating {
            return RuneHitOutcome::WrongTarget;
        }

        let secondary_target = self.targets.iter().enumerate().find_map(|(idx, state)| {
            (idx != target_index && *state == Activation::Activating).then_some(idx)
        });

        self.targets[target_index] = Activation::Activated;
        if self.all_targets_activated() {
            return RuneHitOutcome::Activated;
        }

        self.round = ActivationRound::Large(LargeRound::Secondary {
            secondary_remaining: LARGE_SECONDARY_TIMEOUT,
            target: secondary_target,
        });
        RuneHitOutcome::PrimaryHit
    }

    fn hit_large_secondary(
        &mut self,
        target_index: usize,
        secondary_target: Option<usize>,
        rng: &mut impl Rng,
    ) -> RuneHitOutcome {
        if secondary_target != Some(target_index)
            || self.targets[target_index] != Activation::Activating
        {
            return RuneHitOutcome::WrongTarget;
        }

        self.advance_large_round(rng);
        RuneHitOutcome::SecondaryHit
    }

    fn start_round(&mut self, mode: RuneMode, rng: &mut impl Rng) -> RunTransition {
        match mode {
            RuneMode::Small => self.start_small_round(rng),
            RuneMode::Large => self.start_large_primary_round(rng),
        }
    }

    fn start_small_round(&mut self, rng: &mut impl Rng) -> RunTransition {
        self.clear_transient_targets();
        let Some(target) = self.choose_targets(1, rng).into_iter().next() else {
            return RunTransition::Activated;
        };
        self.targets[target] = Activation::Activating;
        self.round = ActivationRound::Small(SmallRound {
            primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
        });
        RunTransition::Advanced
    }

    fn start_large_primary_round(&mut self, rng: &mut impl Rng) -> RunTransition {
        self.clear_transient_targets();
        let targets = self.choose_targets(2, rng);
        if targets.is_empty() {
            return RunTransition::Activated;
        }
        for target in targets {
            self.targets[target] = Activation::Activating;
        }
        self.round = ActivationRound::Large(LargeRound::Primary {
            primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
        });
        RunTransition::Advanced
    }

    fn advance_large_round(&mut self, rng: &mut impl Rng) -> RunTransition {
        self.clear_transient_targets();
        if self.all_targets_activated() {
            RunTransition::Activated
        } else {
            self.start_large_primary_round(rng)
        }
    }

    fn choose_targets(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        let mut available = self
            .targets
            .iter()
            .enumerate()
            .filter_map(|(idx, state)| (*state != Activation::Activated).then_some(idx))
            .collect::<Vec<_>>();
        available.shuffle(rng);
        available.truncate(count.min(available.len()));
        available
    }

    fn clear_transient_targets(&mut self) {
        for state in &mut self.targets {
            if *state != Activation::Activated {
                *state = Activation::Deactivated;
            }
        }
    }

    fn all_targets_activated(&self) -> bool {
        self.targets
            .iter()
            .all(|state| *state == Activation::Activated)
    }
}

fn expire_after(remaining: &mut f32, delta_secs: f32) -> bool {
    if delta_secs >= *remaining {
        *remaining = 0.0;
        true
    } else {
        *remaining -= delta_secs;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_indices(state: &MechanismState) -> Vec<usize> {
        state
            .target_states()
            .iter()
            .enumerate()
            .filter_map(|(idx, activation)| (*activation == Activation::Activating).then_some(idx))
            .collect()
    }

    fn activated_count(state: &MechanismState) -> usize {
        state
            .target_states()
            .iter()
            .filter(|activation| **activation == Activation::Activated)
            .count()
    }

    #[test]
    fn small_rune_lights_one_target_and_advances_on_hit() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Small, &mut rng);
        let active = active_indices(&state);

        assert_eq!(active.len(), 1);
        assert_eq!(
            state.hit(active[0], RuneMode::Small, &mut rng),
            RuneHitOutcome::PrimaryHit
        );
        assert_eq!(activated_count(&state), 1);
        assert_eq!(active_indices(&state).len(), 1);
    }

    #[test]
    fn small_rune_wrong_target_fails() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Small, &mut rng);
        let active = active_indices(&state)[0];
        let wrong = (0..RUNE_TARGET_COUNT).find(|idx| *idx != active).unwrap();

        assert_eq!(
            state.hit(wrong, RuneMode::Small, &mut rng),
            RuneHitOutcome::WrongTarget
        );
        assert!(matches!(state, MechanismState::Failed { .. }));
    }

    #[test]
    fn small_rune_primary_timeout_fails() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Small, &mut rng);

        assert_eq!(
            state.tick(RuneMode::Small, ACTIVATION_PRIMARY_TIMEOUT, &mut rng),
            RuneTransition::Failed
        );
        assert!(matches!(state, MechanismState::Failed { .. }));
    }

    #[test]
    fn large_rune_lights_two_targets_and_enters_secondary_window() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);
        let active = active_indices(&state);

        assert_eq!(active.len(), 2);
        assert_eq!(
            state.hit(active[0], RuneMode::Large, &mut rng),
            RuneHitOutcome::PrimaryHit
        );
        assert_eq!(activated_count(&state), 1);
        assert_eq!(active_indices(&state).len(), 1);
    }

    #[test]
    fn large_rune_secondary_timeout_advances_without_failure() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);
        let first = active_indices(&state)[0];
        state.hit(first, RuneMode::Large, &mut rng);

        assert_eq!(
            state.tick(RuneMode::Large, LARGE_SECONDARY_TIMEOUT, &mut rng),
            RuneTransition::Advanced
        );
        assert!(matches!(state, MechanismState::Activating(_)));
        assert_eq!(activated_count(&state), 1);
    }

    #[test]
    fn large_rune_primary_timeout_fails() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);

        assert_eq!(
            state.tick(RuneMode::Large, ACTIVATION_PRIMARY_TIMEOUT, &mut rng),
            RuneTransition::Failed
        );
        assert!(matches!(state, MechanismState::Failed { .. }));
    }

    #[test]
    fn large_rune_global_timeout_fails() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);

        assert_eq!(
            state.tick(RuneMode::Large, ACTIVATION_GLOBAL_TIMEOUT, &mut rng),
            RuneTransition::Failed
        );
        assert!(matches!(state, MechanismState::Failed { .. }));
    }
}
