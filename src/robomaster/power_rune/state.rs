use crate::robomaster::power_rune::common::{RuneAction, RuneMode};
use crate::robomaster::power_rune::consts::{
    ACTIVATION_GLOBAL_TIMEOUT, ACTIVATION_PRIMARY_TIMEOUT, LARGE_SECONDARY_TIMEOUT,
};
use crate::robomaster::visibility::Activation;
use bevy::prelude::{Timer, TimerMode};
use rand::prelude::SliceRandom;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum MechanismState {
    Inactive { wait: Timer },
    Activating(ActivatingState),
    Activated { wait: Timer },
    Failed { wait: Timer },
}

#[derive(Debug, Clone)]
pub struct ActivatingState {
    mode: RuneMode,
    targets: Vec<Activation>,
    timeout: Timer,
    next_round: Timer,
}

pub const FUNNY: bool = true;

impl ActivatingState {
    pub fn new(mode: RuneMode, targets: Vec<Activation>) -> Self {
        Self {
            mode,
            targets,
            timeout: Timer::from_seconds(ACTIVATION_GLOBAL_TIMEOUT, TimerMode::Once),
            next_round: Timer::from_seconds(ACTIVATION_PRIMARY_TIMEOUT, TimerMode::Once),
        }
    }

    pub fn start(&mut self) -> Vec<RuneAction> {
        self.new_round()
    }

    fn available_targets(&self) -> Vec<usize> {
        self.targets
            .iter()
            .enumerate()
            .filter_map(|(idx, activation)| {
                if matches!(activation, Activation::Activated) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect()
    }

    pub fn on_hit(&mut self, target_index: usize) -> Option<RuneAction> {
        let action = self.try_activate(target_index)?;
        if self.mode != RuneMode::Large {
            return Some(action);
        }
        if !matches!(action, RuneAction::PartialActivate(_)) {
            return Some(action);
        }
        // 重设20秒超时
        self.timeout.reset();
        // 大机关逻辑：规则要求命中任意一个靶后启动1秒二次窗口
        // 命中第一个靶后启动1秒二次命中窗口
        self.next_round = Timer::from_seconds(LARGE_SECONDARY_TIMEOUT, TimerMode::Once);
        Some(action)
    }

    pub fn tick(&mut self, delta: Duration) -> Option<Vec<RuneAction>> {
        if self.timeout.tick(delta).just_finished() {
            self.timeout = Timer::from_seconds(ACTIVATION_GLOBAL_TIMEOUT, TimerMode::Once);
            return Some(vec![RuneAction::Failure]); // 20秒全局超时激活失败
        }
        if self.next_round.tick(delta).just_finished() {
            return Some(self.new_round()); // 激活窗口超时
        }
        None
    }

    fn new_round(&mut self) -> Vec<RuneAction> {
        let mut available = self.available_targets();
        if available.is_empty() {
            panic!("No active targets available");
        }
        available.shuffle(&mut rand::rng());
        let required = match self.mode {
            RuneMode::Small => 1,
            RuneMode::Large => 2,
        };
        let count = required.min(available.len());
        let selection: Vec<usize> = available.into_iter().take(count).collect();

        let mut vec = vec![];
        for (idx, activation) in &mut self.targets.iter_mut().enumerate() {
            if !matches!(activation, Activation::Activated) {
                *activation = Activation::Deactivated;
                vec.push(RuneAction::SetAppearance(idx, Activation::Deactivated));
            }
        }

        for &idx in &selection {
            self.targets[idx] = Activation::Activating;
            vec.push(RuneAction::SetAppearance(idx, Activation::Activating));
        }
        self.next_round = Timer::from_seconds(ACTIVATION_PRIMARY_TIMEOUT, TimerMode::Once);
        vec
    }

    fn try_activate(&mut self, target: usize) -> Option<RuneAction> {
        if self.targets[target] != Activation::Activating {
            return match FUNNY {
                true => None,
                // 击中非点亮模块，触发激活失败
                false => Some(RuneAction::Failure),
            };
        }
        if self.targets[target] == Activation::Activated {
            return None;
        }
        self.timeout = Timer::from_seconds(ACTIVATION_PRIMARY_TIMEOUT, TimerMode::Once);
        self.targets[target] = Activation::Activated;
        Some(
            match self
                .targets
                .iter()
                .all(|v| matches!(v, Activation::Activated))
            {
                true => RuneAction::FullActivate(target),
                false => RuneAction::PartialActivate(target),
            },
        )
    }
}
