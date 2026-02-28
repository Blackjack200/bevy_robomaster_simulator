//! Talos 共享内存 IPC 模块
//!
//! 提供与 C++ talos-cpp 通信的零拷贝共享内存接口。
//! 纯 IPC 代码已移至 `talos-ipc` crate，此处只保留 Bevy 集成。

mod capture;
mod plugin;

pub use plugin::TalosPlugin;
