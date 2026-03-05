use crate::layout::{FLAG_NEW, INDEX_MASK};
use std::sync::atomic::Ordering;

pub struct TripleBufferProducer<'a, S> {
    state: &'a std::sync::atomic::AtomicU8,
    write_idx: &'a mut u8,
    slots: &'a mut [S; 3],
}

impl<'a, S> TripleBufferProducer<'a, S> {
    /// 创建生产者
    ///
    /// # Safety
    /// 调用者必须确保只有一个生产者存在
    pub unsafe fn new(
        state: &'a std::sync::atomic::AtomicU8,
        write_idx: &'a mut u8,
        slots: &'a mut [S; 3],
    ) -> Self {
        Self {
            state,
            write_idx,
            slots,
        }
    }

    /// 获取可写槽位的可变引用
    pub fn borrow_mut(&mut self) -> &mut S {
        &mut self.slots[*self.write_idx as usize]
    }

    /// 发布数据
    pub fn publish(&mut self) {
        let old = self
            .state
            .swap(*self.write_idx | FLAG_NEW, Ordering::AcqRel);
        *self.write_idx = old & INDEX_MASK;
    }
}

/// TripleBuffer 消费者操作
pub struct TripleBufferConsumer<'a, S> {
    state: &'a std::sync::atomic::AtomicU8,
    read_idx: &'a mut u8,
    slots: &'a [S; 3],
}

impl<'a, S> TripleBufferConsumer<'a, S> {
    /// # Safety
    /// caller must ensure only no more than one consumer exists
    pub unsafe fn new(
        state: &'a std::sync::atomic::AtomicU8,
        read_idx: &'a mut u8,
        slots: &'a [S; 3],
    ) -> Self {
        Self {
            state,
            read_idx,
            slots,
        }
    }

    pub fn borrow(&mut self) -> Option<&S> {
        let mut expected = self.state.load(Ordering::Acquire);

        // 没有新数据
        if (expected & FLAG_NEW) == 0 {
            return None;
        }

        let mut ready_idx = expected & INDEX_MASK;
        let mut desired = *self.read_idx;

        // 第一次 CAS
        match self.state.compare_exchange_weak(
            expected,
            desired,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                *self.read_idx = ready_idx;
                Some(&self.slots[ready_idx as usize])
            }
            Err(new_expected) => {
                // 生产者刚发布了新数据，再试一次
                expected = new_expected;
                if (expected & FLAG_NEW) == 0 {
                    return None;
                }
                ready_idx = expected & INDEX_MASK;
                desired = *self.read_idx;

                match self.state.compare_exchange_weak(
                    expected,
                    desired,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        *self.read_idx = ready_idx;
                        Some(&self.slots[ready_idx as usize])
                    }
                    Err(_) => None,
                }
            }
        }
    }

    /// Check if new data is available without consuming it
    ///
    /// This is useful for non-blocking polling of data availability.
    /// Returns `true` if a call to `borrow()` would return `Some(_)`.
    #[must_use]
    #[allow(dead_code)]
    pub fn has_new_data(&self) -> bool {
        (self.state.load(Ordering::Acquire) & FLAG_NEW) != 0
    }
}
