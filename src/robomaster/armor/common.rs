#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum ArmorType {
    Small = 0,
    Large = 1,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
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
}
