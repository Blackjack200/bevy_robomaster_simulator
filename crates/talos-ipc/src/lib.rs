//! Talos IPC 共享内存通信库
//!
//! 纯 Rust 实现的零拷贝共享内存 IPC，与 C++ talos-cpp 兼容。
//! 不依赖 bevy 或任何游戏引擎，可用于独立工具和服务。

mod layout;
mod publisher;
mod shm;
mod subscriber;
mod triple_buffer;

pub use layout::*;
pub use publisher::ShmPublisher;
pub use shm::{ShmError, ShmRegion};
pub use subscriber::ShmSubscriber;
