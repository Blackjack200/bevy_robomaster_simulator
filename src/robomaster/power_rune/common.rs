use crate::robomaster::visibility::Activation;

#[derive(Debug, PartialEq, Eq)]
pub enum RuneAction {
    StartActivating,
    Failure,
    SetAppearance(usize, Activation),
    ResetToInactive,
    PartialActivate(usize),
    FullActivate(usize),
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum RuneMode {
    Small,
    Large,
}
