use crate::robomaster::power_rune::common::{
    RUNE_TARGET_COUNT, RuneHitOutcome, RuneMode, RuneTransition,
};
use crate::robomaster::power_rune::consts::{
    ACTIVATED_HOLD, ACTIVATION_GLOBAL_TIMEOUT, ACTIVATION_PRIMARY_TIMEOUT, FAILURE_RECOVER,
    FUNNY_IGNORE_WRONG_TARGET_FAILURE, INACTIVE_WAIT, LARGE_SECONDARY_TIMEOUT,
};
use crate::robomaster::visibility::Activation;
use rand::Rng;
use rand::prelude::SliceRandom;

pub type RuneTargetStates = [Activation; RUNE_TARGET_COUNT];

#[derive(Debug, Clone, PartialEq)]
pub enum MechanismState {
    Inactive { mode: RuneMode, remaining: f32 },
    Activating(ActivationRun),
    Activated { mode: RuneMode, remaining: f32 },
    Failed { mode: RuneMode, remaining: f32 },
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
    Large(LargeRun),
}

#[derive(Debug, Clone, PartialEq)]
struct SmallRound {
    primary_remaining: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct LargeRun {
    completed_groups: usize,
    phase: LargePhase,
}

#[derive(Debug, Clone, PartialEq)]
enum LargePhase {
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
    ResetToInactive,
}

impl MechanismState {
    pub fn inactive(mode: RuneMode) -> Self {
        Self::Inactive {
            mode,
            remaining: INACTIVE_WAIT,
        }
    }

    pub fn start(mode: RuneMode, rng: &mut impl Rng) -> Self {
        Self::Activating(ActivationRun::new(mode, rng))
    }

    pub fn mode(&self) -> RuneMode {
        match self {
            Self::Inactive { mode, .. }
            | Self::Activated { mode, .. }
            | Self::Failed { mode, .. } => *mode,
            Self::Activating(run) => run.mode(),
        }
    }

    pub fn tick(&mut self, delta_secs: f32, rng: &mut impl Rng) -> RuneTransition {
        let delta_secs = delta_secs.max(0.0);
        let mut next = None;

        let transition = match self {
            Self::Inactive { mode, remaining } => {
                if expire_after(remaining, delta_secs) {
                    next = Some(Self::start(*mode, rng));
                    RuneTransition::Started
                } else {
                    RuneTransition::None
                }
            }
            Self::Activating(run) => match run.tick(delta_secs, rng) {
                RunTransition::None => RuneTransition::None,
                RunTransition::Advanced => RuneTransition::Advanced,
                RunTransition::Failed => {
                    next = Some(Self::failed(run.mode()));
                    RuneTransition::Failed
                }
                RunTransition::Activated => {
                    next = Some(Self::activated(run.mode()));
                    RuneTransition::Activated
                }
                RunTransition::ResetToInactive => {
                    next = Some(Self::inactive(run.mode()));
                    RuneTransition::ResetToInactive
                }
            },
            Self::Activated { mode, remaining } => {
                if expire_after(remaining, delta_secs) {
                    next = Some(Self::inactive(*mode));
                    RuneTransition::ResetToInactive
                } else {
                    RuneTransition::None
                }
            }
            Self::Failed { mode, remaining } => {
                if expire_after(remaining, delta_secs) {
                    next = Some(Self::inactive(*mode));
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

    pub fn hit(&mut self, target_index: usize, rng: &mut impl Rng) -> RuneHitOutcome {
        let Self::Activating(run) = self else {
            return RuneHitOutcome::Ignored;
        };
        let mode = run.mode();

        match run.hit(target_index, rng) {
            RuneHitOutcome::WrongTarget => {
                if !FUNNY_IGNORE_WRONG_TARGET_FAILURE {
                    *self = Self::failed(mode);
                }
                RuneHitOutcome::WrongTarget
            }
            RuneHitOutcome::Activated => {
                *self = Self::activated(mode);
                RuneHitOutcome::Activated
            }
            outcome => outcome,
        }
    }

    pub fn is_activating(&self) -> bool {
        matches!(self, Self::Activating(_))
    }

    pub fn is_activating_large(&self) -> bool {
        matches!(
            self,
            Self::Activating(ActivationRun {
                round: ActivationRound::Large(_),
                ..
            })
        )
    }

    pub fn large_progress(&self) -> Option<usize> {
        match self {
            Self::Activating(run) => run.large_progress(),
            Self::Inactive { .. } | Self::Activated { .. } | Self::Failed { .. } => None,
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

    fn activated(mode: RuneMode) -> Self {
        Self::Activated {
            mode,
            remaining: ACTIVATED_HOLD,
        }
    }

    fn failed(mode: RuneMode) -> Self {
        Self::Failed {
            mode,
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
                RuneMode::Large => ActivationRound::Large(LargeRun {
                    completed_groups: 0,
                    phase: LargePhase::Primary {
                        primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
                    },
                }),
            },
        };
        run.start_round(mode, rng);
        run
    }

    pub fn mode(&self) -> RuneMode {
        match &self.round {
            ActivationRound::Small(_) => RuneMode::Small,
            ActivationRound::Large(_) => RuneMode::Large,
        }
    }

    pub fn target_states(&self) -> RuneTargetStates {
        self.targets
    }

    pub fn large_progress(&self) -> Option<usize> {
        match &self.round {
            ActivationRound::Large(run) => Some(run.completed_groups),
            ActivationRound::Small(_) => None,
        }
    }

    fn tick(&mut self, delta_secs: f32, rng: &mut impl Rng) -> RunTransition {
        match &mut self.round {
            ActivationRound::Small(round) => tick_two_timers(
                &mut self.global_remaining,
                &mut round.primary_remaining,
                delta_secs,
                RunTransition::ResetToInactive,
                RunTransition::Failed,
            ),
            ActivationRound::Large(run) => match &mut run.phase {
                LargePhase::Primary { primary_remaining } => tick_two_timers(
                    &mut self.global_remaining,
                    primary_remaining,
                    delta_secs,
                    RunTransition::ResetToInactive,
                    RunTransition::Failed,
                ),
                LargePhase::Secondary {
                    secondary_remaining,
                    ..
                } => match tick_two_timers(
                    &mut self.global_remaining,
                    secondary_remaining,
                    delta_secs,
                    RunTransition::ResetToInactive,
                    RunTransition::Advanced,
                ) {
                    RunTransition::Advanced => self.start_large_primary_round(rng),
                    transition => transition,
                },
            },
        }
    }

    fn hit(&mut self, target_index: usize, rng: &mut impl Rng) -> RuneHitOutcome {
        if target_index >= RUNE_TARGET_COUNT {
            return RuneHitOutcome::WrongTarget;
        }

        match &self.round {
            ActivationRound::Small(_) => self.hit_small(target_index, rng),
            ActivationRound::Large(run) => match &run.phase {
                LargePhase::Primary { .. } => self.hit_large_primary(target_index, rng),
                LargePhase::Secondary { target, .. } => {
                    self.hit_large_secondary(target_index, *target)
                }
            },
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
        let ActivationRound::Large(run) = &mut self.round else {
            unreachable!("large primary hit requires a large run");
        };
        run.completed_groups += 1;
        if run.completed_groups == RUNE_TARGET_COUNT {
            return RuneHitOutcome::Activated;
        }

        run.phase = LargePhase::Secondary {
            secondary_remaining: LARGE_SECONDARY_TIMEOUT,
            target: secondary_target,
        };
        RuneHitOutcome::PrimaryHit
    }

    fn hit_large_secondary(
        &mut self,
        target_index: usize,
        secondary_target: Option<usize>,
    ) -> RuneHitOutcome {
        if secondary_target != Some(target_index)
            || self.targets[target_index] != Activation::Activating
        {
            return RuneHitOutcome::WrongTarget;
        }

        self.targets[target_index] = Activation::Activated;
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
        let completed_groups = match &self.round {
            ActivationRound::Large(run) => run.completed_groups,
            ActivationRound::Small(_) => 0,
        };
        self.clear_all_targets();
        let targets = self.choose_targets_from_all(2, rng);
        if targets.is_empty() {
            return RunTransition::Activated;
        }
        for target in targets {
            self.targets[target] = Activation::Activating;
        }
        self.round = ActivationRound::Large(LargeRun {
            completed_groups,
            phase: LargePhase::Primary {
                primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
            },
        });
        RunTransition::Advanced
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

    fn choose_targets_from_all(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        let mut targets = (0..RUNE_TARGET_COUNT).collect::<Vec<_>>();
        targets.shuffle(rng);
        targets.truncate(count.min(RUNE_TARGET_COUNT));
        targets
    }

    fn clear_transient_targets(&mut self) {
        for state in &mut self.targets {
            if *state != Activation::Activated {
                *state = Activation::Deactivated;
            }
        }
    }

    fn clear_all_targets(&mut self) {
        self.targets = [Activation::Deactivated; RUNE_TARGET_COUNT];
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

fn tick_two_timers(
    global_remaining: &mut f32,
    local_remaining: &mut f32,
    delta_secs: f32,
    global_transition: RunTransition,
    local_transition: RunTransition,
) -> RunTransition {
    let next_event = (*global_remaining).min(*local_remaining);
    if delta_secs < next_event {
        *global_remaining -= delta_secs;
        *local_remaining -= delta_secs;
        return RunTransition::None;
    }

    if *global_remaining <= *local_remaining {
        *global_remaining = 0.0;
        global_transition
    } else {
        *global_remaining -= *local_remaining;
        *local_remaining = 0.0;
        local_transition
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
        assert_eq!(state.hit(active[0], &mut rng), RuneHitOutcome::PrimaryHit);
        assert_eq!(activated_count(&state), 1);
        assert_eq!(active_indices(&state).len(), 1);
    }

    #[test]
    fn funny_mode_keeps_small_rune_activating_after_wrong_target() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Small, &mut rng);
        let active = active_indices(&state)[0];
        let wrong = (0..RUNE_TARGET_COUNT).find(|idx| *idx != active).unwrap();

        assert_eq!(state.hit(wrong, &mut rng), RuneHitOutcome::WrongTarget);
        assert!(matches!(state, MechanismState::Activating(_)));
        assert_eq!(active_indices(&state), vec![active]);
    }

    #[test]
    fn small_rune_primary_timeout_fails() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Small, &mut rng);

        assert_eq!(
            state.tick(ACTIVATION_PRIMARY_TIMEOUT, &mut rng),
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
        assert_eq!(state.hit(active[0], &mut rng), RuneHitOutcome::PrimaryHit);
        assert!(state.is_activating_large());
        assert_eq!(state.large_progress(), Some(1));
        assert_eq!(activated_count(&state), 1);
        assert_eq!(active_indices(&state).len(), 1);
    }

    #[test]
    fn large_rune_secondary_hit_waits_for_window_timeout() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);
        let active = active_indices(&state);

        assert_eq!(state.hit(active[0], &mut rng), RuneHitOutcome::PrimaryHit);
        assert_eq!(state.hit(active[1], &mut rng), RuneHitOutcome::SecondaryHit);
        assert_eq!(state.large_progress(), Some(1));
        assert_eq!(active_indices(&state).len(), 0);

        assert_eq!(
            state.tick(LARGE_SECONDARY_TIMEOUT * 0.5, &mut rng),
            RuneTransition::None
        );
        assert_eq!(state.large_progress(), Some(1));
        assert_eq!(active_indices(&state).len(), 0);

        assert_eq!(
            state.tick(LARGE_SECONDARY_TIMEOUT * 0.5, &mut rng),
            RuneTransition::Advanced
        );
        assert_eq!(state.large_progress(), Some(1));
        assert_eq!(active_indices(&state).len(), 2);
    }

    #[test]
    fn large_rune_secondary_timeout_advances_without_failure() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);
        let first = active_indices(&state)[0];
        state.hit(first, &mut rng);

        assert_eq!(
            state.tick(LARGE_SECONDARY_TIMEOUT, &mut rng),
            RuneTransition::Advanced
        );
        assert!(matches!(state, MechanismState::Activating(_)));
        assert!(state.is_activating_large());
        assert_eq!(state.large_progress(), Some(1));
        assert_eq!(active_indices(&state).len(), 2);
    }

    #[test]
    fn large_rune_activates_after_five_primary_hits() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);

        for expected_progress in 1..RUNE_TARGET_COUNT {
            let active = active_indices(&state);
            assert_eq!(active.len(), 2);
            assert_eq!(state.hit(active[0], &mut rng), RuneHitOutcome::PrimaryHit);
            assert_eq!(state.large_progress(), Some(expected_progress));
            assert_eq!(
                state.tick(LARGE_SECONDARY_TIMEOUT, &mut rng),
                RuneTransition::Advanced
            );
        }

        let active = active_indices(&state);
        assert_eq!(active.len(), 2);
        assert_eq!(state.hit(active[0], &mut rng), RuneHitOutcome::Activated);
        assert!(matches!(state, MechanismState::Activated { .. }));
    }

    #[test]
    fn large_rune_primary_timeout_fails() {
        let mut rng = rand::rng();
        let mut state = MechanismState::start(RuneMode::Large, &mut rng);

        assert_eq!(
            state.tick(ACTIVATION_PRIMARY_TIMEOUT, &mut rng),
            RuneTransition::Failed
        );
        assert!(matches!(state, MechanismState::Failed { .. }));
    }

    #[test]
    fn large_rune_global_timeout_resets_to_inactive() {
        let mut state = MechanismState::Activating(ActivationRun {
            global_remaining: 1.0,
            targets: [
                Activation::Activating,
                Activation::Activating,
                Activation::Deactivated,
                Activation::Deactivated,
                Activation::Deactivated,
            ],
            round: ActivationRound::Large(LargeRun {
                completed_groups: 1,
                phase: LargePhase::Primary {
                    primary_remaining: ACTIVATION_PRIMARY_TIMEOUT,
                },
            }),
        });
        let mut rng = rand::rng();

        assert_eq!(state.tick(1.0, &mut rng), RuneTransition::ResetToInactive);
        assert!(matches!(
            state,
            MechanismState::Inactive {
                mode: RuneMode::Large,
                ..
            }
        ));
    }
}
