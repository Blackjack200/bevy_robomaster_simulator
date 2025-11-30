use crate::robomaster::prelude::*;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Team {
    Red,
    Blue,
}

pub type RobotConfig = (ArmorType, ArmorLabel, usize);

pub const HERO_ROBOT_CONFIG: RobotConfig = (ArmorType::Large, ArmorLabel::HeroOne, 4);
pub const ENGINEER_ROBOT_CONFIG: RobotConfig = (ArmorType::Small, ArmorLabel::EngineerG, 4);
pub const INFANTRY_THREE_CONFIG: RobotConfig = (ArmorType::Small, ArmorLabel::InfantryThree, 4);
pub const INFANTRY_FOUR_CONFIG: RobotConfig = (ArmorType::Small, ArmorLabel::InfantryFour, 4);

pub const SENTINEL_ROBOT_TWO_CONFIG: RobotConfig = (ArmorType::Small, ArmorLabel::InfantryTwo, 4);
pub const SENTINEL_ROBOT_THREE_CONFIG: RobotConfig =
    (ArmorType::Small, ArmorLabel::InfantryThree, 4);
pub const SENTINEL_ROBOT_FOUR_CONFIG: RobotConfig = (ArmorType::Small, ArmorLabel::InfantryFour, 4);

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Robot {
    /// 英雄机器人 - 唯一可以发射42mm弹丸的机器人
    /// - 编号: 1号机
    /// - 特点: 高血量、高伤害、可部署模式
    Hero,

    /// 工程机器人 - 负责抓取能量单元和团队增益
    /// - 编号: 2号机  
    /// - 特点: 无发射机构、高机动性、特殊任务执行能力
    Engineer,

    /// 步兵机器人 - 基础作战单位，发射17mm弹丸
    /// - 编号: 3/4号机（两台）
    /// - 特点: 均衡性能、经验升级系统
    Infantry,

    /// 空中机器人 - 空中支援单位，发射17mm弹丸  
    /// - 编号: 6号机
    /// - 特点: 飞行能力、第一视角画面、激光检测模块
    Aerial,

    /// 哨兵机器人 - 基地防守单位，可全自动或半自动运行
    /// - 编号: 7号机
    /// - 特点: 自主防御,姿态切换系统,堡垒占领能力
    Sentinel,

    /// 飞镖系统 - 远程打击系统,攻击前哨站和基地
    /// - 编号: 8号机
    /// - 特点: 飞镖发射,目标选择机制,闸门控制
    DartSystem,

    /// 雷达 - 战场信息获取和反制系统
    /// - 编号: 9号机
    /// - 特点: 激光照射,坐标标记,信息波解析
    Radar,
}
