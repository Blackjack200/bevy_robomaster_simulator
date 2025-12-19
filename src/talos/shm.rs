//! 共享内存区域封装 (基于 memmap2)
//!
//! 使用内存映射文件替代 POSIX 共享内存，纯 Rust 实现。

use memmap2::MmapMut;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug)]
pub enum ShmError {
    IoError(io::Error),
    MapFailed,
    InvalidSize,
}

impl std::fmt::Display for ShmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShmError::IoError(e) => write!(f, "IO error: {}", e),
            ShmError::MapFailed => write!(f, "mmap failed"),
            ShmError::InvalidSize => write!(f, "invalid size"),
        }
    }
}

impl std::error::Error for ShmError {}

impl From<io::Error> for ShmError {
    fn from(e: io::Error) -> Self {
        ShmError::IoError(e)
    }
}

/// 获取共享内存文件路径
fn shm_path(name: &str) -> PathBuf {
    // 使用 /tmp 目录，移除开头的 '/'
    let clean_name = name.trim_start_matches('/');
    PathBuf::from("/tmp").join(clean_name)
}

/// RAII 封装的共享内存区域
pub struct ShmRegion {
    mmap: MmapMut,
    path: PathBuf,
    is_owner: bool,
}

unsafe impl Send for ShmRegion {}
unsafe impl Sync for ShmRegion {}

impl ShmRegion {
    /// 创建新的共享内存区域 (生产者)
    pub fn create(name: &str, size: usize) -> Result<Self, ShmError> {
        let path = shm_path(name);

        // 创建或覆盖文件
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;

        // 设置文件大小
        file.set_len(size as u64)?;

        // 写入零填充
        file.write_all(&vec![0u8; size])?;
        file.sync_all()?;

        // 重新打开并映射
        let file = OpenOptions::new().read(true).write(true).open(&path)?;

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            mmap,
            path,
            is_owner: true,
        })
    }

    /// 打开已存在的共享内存区域 (消费者)
    pub fn open(name: &str, size: usize) -> Result<Self, ShmError> {
        let path = shm_path(name);

        let file = OpenOptions::new().read(true).write(true).open(&path)?;

        // 验证大小
        let metadata = file.metadata()?;
        if metadata.len() < size as u64 {
            return Err(ShmError::InvalidSize);
        }

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            mmap,
            path,
            is_owner: false,
        })
    }

    /// 获取指向共享内存的指针
    pub fn as_ptr(&self) -> *mut u8 {
        self.mmap.as_ptr() as *mut u8
    }

    /// 获取共享内存大小
    pub fn size(&self) -> usize {
        self.mmap.len()
    }

    /// 将共享内存解释为指定类型
    ///
    /// # Safety
    /// 调用者必须确保类型 T 与共享内存布局匹配
    pub unsafe fn as_ref<T>(&self) -> &T {
        &*(self.mmap.as_ptr() as *const T)
    }

    /// 将共享内存解释为指定类型 (可变)
    ///
    /// # Safety
    /// 调用者必须确保类型 T 与共享内存布局匹配
    pub unsafe fn as_mut<T>(&mut self) -> &mut T {
        &mut *(self.mmap.as_ptr() as *mut T)
    }

    /// 刷新内存映射到文件
    pub fn flush(&self) -> Result<(), ShmError> {
        self.mmap.flush()?;
        Ok(())
    }
}

impl Drop for ShmRegion {
    fn drop(&mut self) {
        if self.is_owner {
            // 删除文件
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
