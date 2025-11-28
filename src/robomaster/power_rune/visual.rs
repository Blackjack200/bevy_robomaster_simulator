use crate::all_arg_constructor;
use crate::robomaster::power_rune::common::RuneMode;
use crate::robomaster::visibility::{Activation, Control, Controller, StatefulAppearance};

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
