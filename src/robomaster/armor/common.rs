#[repr(u8)]
#[derive(Debug, Copy, Clone)]
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
    InfantryThree = 3,
    /// 4号标签 - 🎯步兵机器人备用编号
    InfantryFour = 4,
    /// O号标签 - 🏰前哨站装甲模块
    OutpostZeo = 5,
    /// Bs号标签 - 🏠基地小装甲模块
    BaseSmall = 6,
    /// Bb号标签 - 🏠基地大装甲模块
    BaseLarge = 7,

    LegacyFive = 255,
}

impl ArmorLabel {
    pub fn sequence_small() -> &'static [ArmorLabel; 9] {
        &[
            ArmorLabel::EngineerG,
            ArmorLabel::HeroOne,
            ArmorLabel::InfantryTwo,
            ArmorLabel::InfantryThree,
            ArmorLabel::InfantryFour,
            ArmorLabel::OutpostZeo,
            ArmorLabel::BaseSmall,
            ArmorLabel::BaseLarge,
            ArmorLabel::LegacyFive,
        ]
    }

    pub fn index_from_small(label: ArmorLabel) -> usize {
        match label {
            ArmorLabel::EngineerG => 0,
            ArmorLabel::HeroOne => 1,
            ArmorLabel::InfantryTwo => 2,
            ArmorLabel::InfantryThree => 3,
            ArmorLabel::InfantryFour => 4,
            ArmorLabel::OutpostZeo => 5,
            ArmorLabel::BaseSmall => 6,
            ArmorLabel::BaseLarge => 8,
            ArmorLabel::LegacyFive => 7,
        }
    }
}
