#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum RuneMode {
    Small,
    Large,
}

pub const RUNE_TARGET_COUNT: usize = 5;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuneHitOutcome {
    Ignored,
    WrongTarget,
    PrimaryHit,
    SecondaryHit,
    Activated,
}

impl RuneHitOutcome {
    pub const fn is_accurate(self) -> bool {
        matches!(
            self,
            Self::PrimaryHit | Self::SecondaryHit | Self::Activated
        )
    }

    pub const fn activates_rune(self) -> bool {
        matches!(self, Self::Activated)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuneTransition {
    None,
    Started,
    Advanced,
    Failed,
    Activated,
    ResetToInactive,
}
