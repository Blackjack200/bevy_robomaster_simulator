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
            // 20 秒总激活窗口
            timeout: Timer::from_seconds(ACTIVATION_GLOBAL_TIMEOUT, TimerMode::Once),
            // 当前轮次的 2.5 秒窗口
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

    fn all_activated(&self) -> bool {
        self.targets
            .iter()
            .all(|v| matches!(v, Activation::Activated))
    }

    pub fn on_hit(&mut self, target_index: usize) -> Option<Vec<RuneAction>> {
        let action = self.try_activate(target_index)?;

        // 大机关：本轮第一次命中成功后，给 1 秒二次打击窗口，不立即切下一轮
        if self.mode == RuneMode::Large && matches!(action, RuneAction::PartialActivate(_)) {
            // 注意：这里只改 next_round，不碰 20 秒全局 timeout
            self.next_round = Timer::from_seconds(LARGE_SECONDARY_TIMEOUT, TimerMode::Once);
            return Some(vec![action]);
        }

        // 全部激活完成，不再开启下一轮
        if matches!(action, RuneAction::FullActivate(_)) {
            return Some(vec![action]);
        }

        // 小机关：命中后立即进入下一轮
        let mut actions = vec![action];
        actions.extend(self.new_round());
        Some(actions)
    }

    pub fn tick(&mut self, delta: Duration) -> Option<Vec<RuneAction>> {
        // 20 秒全局超时：整次激活机会结束
        if self.timeout.tick(delta).just_finished() {
            self.timeout = Timer::from_seconds(ACTIVATION_GLOBAL_TIMEOUT, TimerMode::Once);
            return Some(vec![RuneAction::Failure]);
        }

        // 当前轮次/窗口超时
        if self.next_round.tick(delta).just_finished() {
            return match self.mode {
                // 小机关：2.5 秒内没打中亮靶 => 失败
                RuneMode::Small => Some(vec![RuneAction::Failure]),

                // 大机关：
                // 1) 若此时仍存在 Activating，说明第一击后 1 秒二击窗口结束
                //    不论第二击中不中，都进入下一组
                // 2) 若不存在 Activating，说明是 2.5 秒主窗口超时，失败
                RuneMode::Large => {
                    let has_activating = self
                        .targets
                        .iter()
                        .any(|v| matches!(v, Activation::Activating));

                    if has_activating {
                        if self.all_activated() {
                            None
                        } else {
                            Some(self.new_round())
                        }
                    } else {
                        Some(vec![RuneAction::Failure])
                    }
                }
            };
        }

        None
    }

    fn new_round(&mut self) -> Vec<RuneAction> {
        let mut available = self.available_targets();
        if available.is_empty() {
            return vec![];
        }

        available.shuffle(&mut rand::rng());

        let required = match self.mode {
            RuneMode::Small => 1,
            RuneMode::Large => 2,
        };
        let count = required.min(available.len());
        let selection: Vec<usize> = available.into_iter().take(count).collect();

        let mut vec = vec![];

        // 先把所有未永久激活的臂清成 Deactivated
        for (idx, activation) in self.targets.iter_mut().enumerate() {
            if !matches!(activation, Activation::Activated) {
                *activation = Activation::Deactivated;
                vec.push(RuneAction::SetAppearance(idx, Activation::Deactivated));
            }
        }

        // 点亮本轮目标
        for &idx in &selection {
            self.targets[idx] = Activation::Activating;
            vec.push(RuneAction::SetAppearance(idx, Activation::Activating));
        }

        // 开启本轮 2.5 秒主窗口
        self.next_round = Timer::from_seconds(ACTIVATION_PRIMARY_TIMEOUT, TimerMode::Once);
        vec
    }

    fn try_activate(&mut self, target: usize) -> Option<RuneAction> {
        match self.targets.get(target)? {
            Activation::Completed | Activation::Activating => {}
            Activation::Activated | Activation::Deactivated => {
                return match FUNNY {
                    true => None,
                    false => Some(RuneAction::Failure),
                };
            }
        }

        self.targets[target] = Activation::Activated;

        Some(if self.all_activated() {
            RuneAction::FullActivate(target)
        } else {
            RuneAction::PartialActivate(target)
        })
    }
}
