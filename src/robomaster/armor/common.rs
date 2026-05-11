#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArmorType {
    Small = 0,
    Large = 1,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArmorLabel {
    /// G号标签 - ⚙️工程机器人专用
    EngineerG = 0,
    /// 1号标签 - 🦸英雄机器人专用
    HeroOne = 1,
    /// 2号标签 - 🎯步兵机器人专用（3号机）
    InfantryTwo = 2,
    /// 3号标签 - 🎯步兵机器人专用（4号机）
    InfantryOrHeroThree = 3,
    /// 4号标签 - 🎯步兵机器人备用编号
    InfantryOrHeroFour = 4,
    /// O号标签 - 🏰前哨站装甲模块
    OutpostZeo = 5,
    /// Bs号标签 - 🏠基地小装甲模块
    BaseSmall = 6,
    /// Bb号标签 - 🏠基地大装甲模块
    BaseLarge = 7,

    HeroLegacyFive = 255,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SmallArmorLabel {
    BaseSmall,
    EngineerG,
    Outpost,
    InfantryTwo,
    InfantryOrHeroThree,
    InfantryOrHeroFour,
    HeroLegacyFive,
}

impl SmallArmorLabel {
    pub const fn label(self) -> ArmorLabel {
        match self {
            Self::BaseSmall => ArmorLabel::BaseSmall,
            Self::EngineerG => ArmorLabel::EngineerG,
            Self::Outpost => ArmorLabel::OutpostZeo,
            Self::InfantryTwo => ArmorLabel::InfantryTwo,
            Self::InfantryOrHeroThree => ArmorLabel::InfantryOrHeroThree,
            Self::InfantryOrHeroFour => ArmorLabel::InfantryOrHeroFour,
            Self::HeroLegacyFive => ArmorLabel::HeroLegacyFive,
        }
    }
}

impl From<SmallArmorLabel> for ArmorLabel {
    fn from(label: SmallArmorLabel) -> Self {
        label.label()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum LargeArmorLabel {
    HeroOne,
    InfantryOrHeroThree,
    InfantryOrHeroFour,
    HeroLegacyFive,
    BaseLarge,
}

impl LargeArmorLabel {
    pub const fn label(self) -> ArmorLabel {
        match self {
            Self::HeroOne => ArmorLabel::HeroOne,
            Self::InfantryOrHeroThree => ArmorLabel::InfantryOrHeroThree,
            Self::InfantryOrHeroFour => ArmorLabel::InfantryOrHeroFour,
            Self::HeroLegacyFive => ArmorLabel::HeroLegacyFive,
            Self::BaseLarge => ArmorLabel::BaseLarge,
        }
    }
}

impl From<LargeArmorLabel> for ArmorLabel {
    fn from(label: LargeArmorLabel) -> Self {
        label.label()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArmorSpec {
    Small(SmallArmorLabel),
    Large(LargeArmorLabel),
}

impl ArmorSpec {
    pub const fn armor_type(self) -> ArmorType {
        match self {
            Self::Small(_) => ArmorType::Small,
            Self::Large(_) => ArmorType::Large,
        }
    }

    pub const fn label(self) -> ArmorLabel {
        match self {
            Self::Small(label) => label.label(),
            Self::Large(label) => label.label(),
        }
    }

    pub const fn sticker_slots(self) -> &'static [ArmorStickerSlot] {
        match self {
            Self::Small(_) => &SMALL_ARMOR_STICKER_SLOTS,
            Self::Large(_) => &LARGE_ARMOR_STICKER_SLOTS,
        }
    }
}

impl From<SmallArmorLabel> for ArmorSpec {
    fn from(label: SmallArmorLabel) -> Self {
        Self::Small(label)
    }
}

impl From<LargeArmorLabel> for ArmorSpec {
    fn from(label: LargeArmorLabel) -> Self {
        Self::Large(label)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ArmorStickerSlot {
    pub label: ArmorLabel,
    pub name_suffix: &'static str,
}

pub const SMALL_ARMOR_STICKER_SLOTS: [ArmorStickerSlot; 7] = [
    ArmorStickerSlot {
        label: ArmorLabel::BaseSmall,
        name_suffix: "B",
    },
    ArmorStickerSlot {
        label: ArmorLabel::EngineerG,
        name_suffix: "G",
    },
    ArmorStickerSlot {
        label: ArmorLabel::OutpostZeo,
        name_suffix: "O",
    },
    ArmorStickerSlot {
        label: ArmorLabel::InfantryTwo,
        name_suffix: "2",
    },
    ArmorStickerSlot {
        label: ArmorLabel::InfantryOrHeroThree,
        name_suffix: "3",
    },
    ArmorStickerSlot {
        label: ArmorLabel::InfantryOrHeroFour,
        name_suffix: "4",
    },
    ArmorStickerSlot {
        label: ArmorLabel::HeroLegacyFive,
        name_suffix: "5",
    },
];

pub const LARGE_ARMOR_STICKER_SLOTS: [ArmorStickerSlot; 5] = [
    ArmorStickerSlot {
        label: ArmorLabel::HeroOne,
        name_suffix: "1",
    },
    ArmorStickerSlot {
        label: ArmorLabel::InfantryOrHeroThree,
        name_suffix: "3",
    },
    ArmorStickerSlot {
        label: ArmorLabel::InfantryOrHeroFour,
        name_suffix: "4",
    },
    ArmorStickerSlot {
        label: ArmorLabel::HeroLegacyFive,
        name_suffix: "5",
    },
    ArmorStickerSlot {
        label: ArmorLabel::BaseLarge,
        name_suffix: "B",
    },
];

impl ArmorLabel {
    pub fn sequence_small() -> &'static [ArmorLabel; 9] {
        &[
            ArmorLabel::EngineerG,
            ArmorLabel::HeroOne,
            ArmorLabel::InfantryTwo,
            ArmorLabel::InfantryOrHeroThree,
            ArmorLabel::InfantryOrHeroFour,
            ArmorLabel::OutpostZeo,
            ArmorLabel::BaseSmall,
            ArmorLabel::BaseLarge,
            ArmorLabel::HeroLegacyFive,
        ]
    }

    pub fn index_from_small(label: ArmorLabel) -> usize {
        match label {
            ArmorLabel::EngineerG => 0,
            ArmorLabel::HeroOne => 1,
            ArmorLabel::InfantryTwo => 2,
            ArmorLabel::InfantryOrHeroThree => 3,
            ArmorLabel::InfantryOrHeroFour => 4,
            ArmorLabel::OutpostZeo => 5,
            ArmorLabel::BaseSmall => 6,
            ArmorLabel::BaseLarge => 8,
            ArmorLabel::HeroLegacyFive => 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_spec_preserves_legacy_type_and_label() {
        let cases = [
            (
                ArmorSpec::Small(SmallArmorLabel::EngineerG),
                ArmorType::Small,
                ArmorLabel::EngineerG,
            ),
            (
                ArmorSpec::Small(SmallArmorLabel::Outpost),
                ArmorType::Small,
                ArmorLabel::OutpostZeo,
            ),
            (
                ArmorSpec::Large(LargeArmorLabel::HeroOne),
                ArmorType::Large,
                ArmorLabel::HeroOne,
            ),
            (
                ArmorSpec::Large(LargeArmorLabel::BaseLarge),
                ArmorType::Large,
                ArmorLabel::BaseLarge,
            ),
        ];

        for (spec, armor_type, label) in cases {
            assert_eq!(spec.armor_type(), armor_type);
            assert_eq!(spec.label(), label);
        }
    }

    #[test]
    fn debug_sequence_and_indexes_keep_legacy_order() {
        assert_eq!(
            ArmorLabel::sequence_small(),
            &[
                ArmorLabel::EngineerG,
                ArmorLabel::HeroOne,
                ArmorLabel::InfantryTwo,
                ArmorLabel::InfantryOrHeroThree,
                ArmorLabel::InfantryOrHeroFour,
                ArmorLabel::OutpostZeo,
                ArmorLabel::BaseSmall,
                ArmorLabel::BaseLarge,
                ArmorLabel::HeroLegacyFive,
            ]
        );

        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::EngineerG), 0);
        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::HeroOne), 1);
        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::InfantryTwo), 2);
        assert_eq!(
            ArmorLabel::index_from_small(ArmorLabel::InfantryOrHeroThree),
            3
        );
        assert_eq!(
            ArmorLabel::index_from_small(ArmorLabel::InfantryOrHeroFour),
            4
        );
        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::OutpostZeo), 5);
        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::BaseSmall), 6);
        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::HeroLegacyFive), 7);
        assert_eq!(ArmorLabel::index_from_small(ArmorLabel::BaseLarge), 8);
    }

    #[test]
    fn sticker_slot_tables_keep_asset_suffixes() {
        assert_eq!(
            ArmorSpec::Small(SmallArmorLabel::Outpost).sticker_slots(),
            &[
                ArmorStickerSlot {
                    label: ArmorLabel::BaseSmall,
                    name_suffix: "B",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::EngineerG,
                    name_suffix: "G",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::OutpostZeo,
                    name_suffix: "O",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::InfantryTwo,
                    name_suffix: "2",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::InfantryOrHeroThree,
                    name_suffix: "3",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::InfantryOrHeroFour,
                    name_suffix: "4",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::HeroLegacyFive,
                    name_suffix: "5",
                },
            ]
        );

        assert_eq!(
            ArmorSpec::Large(LargeArmorLabel::HeroOne).sticker_slots(),
            &[
                ArmorStickerSlot {
                    label: ArmorLabel::HeroOne,
                    name_suffix: "1",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::InfantryOrHeroThree,
                    name_suffix: "3",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::InfantryOrHeroFour,
                    name_suffix: "4",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::HeroLegacyFive,
                    name_suffix: "5",
                },
                ArmorStickerSlot {
                    label: ArmorLabel::BaseLarge,
                    name_suffix: "B",
                },
            ]
        );
    }
}
