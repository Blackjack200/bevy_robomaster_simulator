//! Talos 共享内存 IPC 模块
//!
//! 提供与 C++ talos-cpp 通信的零拷贝共享内存接口。

mod capture;
mod layout;
mod plugin;
mod publisher;
mod shm;
mod subscriber;
mod triple_buffer;

pub use plugin::TalosPlugin;
