use crate::talos::layout::*;
use crate::talos::shm::{ShmError, ShmRegion};
use crate::talos::triple_buffer::TripleBufferConsumer;

pub struct ShmSubscriber {
    meta_region: ShmRegion,
}

impl ShmSubscriber {
    pub fn connect() -> Result<Self, ShmError> {
        let meta_region = ShmRegion::open(SHM_NAME_META, size_of::<ShmMetaRegion>())?;

        unsafe {
            let meta = meta_region.as_ref::<ShmMetaRegion>();
            if meta.header.magic != SHM_MAGIC {
                return Err(ShmError::InvalidSize);
            }
            if meta.header.version != SHM_VERSION {
                return Err(ShmError::InvalidSize);
            }
        }

        Ok(Self { meta_region })
    }

    pub fn recv_gimbal_cmd(&mut self) -> Option<GimbalCmd> {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            let mut consumer = TripleBufferConsumer::new(
                &meta.gimbal_cmd.state,
                &mut meta.gimbal_cmd.read_idx,
                &meta.gimbal_cmd.slots,
            );

            consumer.borrow().copied()
        }
    }

    pub fn has_gimbal_cmd(&self) -> bool {
        unsafe {
            let meta = self.meta_region.as_ref::<ShmMetaRegion>();
            (meta
                .gimbal_cmd
                .state
                .load(std::sync::atomic::Ordering::Acquire)
                & FLAG_NEW)
                != 0
        }
    }
}
