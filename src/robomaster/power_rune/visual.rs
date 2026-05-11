use crate::all_arg_constructor;
use crate::robomaster::power_rune::common::{RUNE_TARGET_COUNT, RuneMode};
use crate::robomaster::power_rune::state::MechanismState;
use crate::robomaster::visibility::{Activation, Control, Controller, StatefulAppearance};
use bevy::prelude::Component;

all_arg_constructor!(
    pub struct RuneVisual {
        target: Controller,
        legging_segments: [Controller; 3],
        padding_segments: Controller,
        progress_segments: Controller,
    }
);

impl RuneVisual {
    pub fn apply(
        &mut self,
        mode: RuneMode,
        activation: Activation,
        appearance: &mut StatefulAppearance,
    ) {
        match mode {
            RuneMode::Small => {
                self.target.set(activation, appearance);
                for swap in &mut self.legging_segments {
                    swap.set(activation, appearance);
                }
            }
            RuneMode::Large => {
                self.target.set(
                    match activation {
                        Activation::Activated => Activation::Deactivated,
                        _ => activation,
                    },
                    appearance,
                );
                match activation {
                    Activation::Activated => self.legging_segments[0].set(activation, appearance),
                    _ => {
                        for legging in &mut self.legging_segments {
                            legging.set(activation, appearance);
                        }
                    }
                }
            }
        }

        self.padding_segments.set(activation, appearance);
        self.progress_segments.set(activation, appearance);
    }
}

#[derive(Component)]
pub struct PowerRuneVisuals {
    root: Controller,
    targets: [RuneVisual; RUNE_TARGET_COUNT],
}

impl PowerRuneVisuals {
    pub fn new(root: Controller, targets: [RuneVisual; RUNE_TARGET_COUNT]) -> Self {
        Self { root, targets }
    }

    pub fn apply(
        &mut self,
        mode: RuneMode,
        state: &MechanismState,
        appearance: &mut StatefulAppearance,
    ) {
        self.root.set(state.root_activation(), appearance);
        for (target, activation) in self.targets.iter_mut().zip(state.target_states()) {
            target.apply(mode, activation, appearance);
        }
    }
}
