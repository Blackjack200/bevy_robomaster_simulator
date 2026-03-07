use crate::layout::*;
use crate::shm::{ShmError, ShmRegion};
use crate::triple_buffer::TripleBufferProducer;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ShmPublisher {
    meta_region: ShmRegion,
    image_pool: ShmRegion,
    current_buffer_id: u8,
}

impl ShmPublisher {
    pub fn create() -> Result<Self, ShmError> {
        let mut meta_region = ShmRegion::create(SHM_NAME_META, size_of::<ShmMetaRegion>())?;
        let image_pool = ShmRegion::create(SHM_NAME_IMAGE_POOL, IMAGE_POOL_SIZE)?;

        unsafe {
            let meta = meta_region.as_mut::<ShmMetaRegion>();

            // 初始化 header
            meta.header = ShmHeader {
                magic: SHM_MAGIC,
                version: SHM_VERSION,
                created_ns: Self::now_ns(),
                heartbeat_ns: Self::now_ns(),
                image_width: IMAGE_WIDTH,
                image_height: IMAGE_HEIGHT,
                _pad: [0; 32],
            };

            // 初始化所有 TripleBuffer (CRITICAL: 零填充破坏了正确的初始状态)
            // 正确初始状态: state=1 (ready slot), write_idx=0, read_idx=2
            Self::init_triple_buffer(&mut meta.image);
            for pose in &mut meta.poses {
                Self::init_triple_buffer(pose);
            }
            Self::init_triple_buffer(&mut meta.gimbal_cmd);
        }

        Ok(Self {
            meta_region,
            image_pool,
            current_buffer_id: 0,
        })
    }

    pub fn publish_image(&mut self, data: &[u8], seq: u64, timestamp_ns: u64) {
        assert_eq!(data.len(), IMAGE_SIZE, "Image size mismatch");

        let buffer_id = self.current_buffer_id;
        self.current_buffer_id = (self.current_buffer_id + 1) % 3;

        unsafe {
            let pool_ptr = self.image_pool.as_ptr();
            let dst = pool_ptr.add(buffer_id as usize * IMAGE_SIZE);
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, IMAGE_SIZE);
        }

        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            let mut producer = TripleBufferProducer::new(
                &meta.image.state,
                &mut meta.image.write_idx,
                &mut meta.image.slots,
            );

            let slot = producer.borrow_mut();
            slot.seq = seq;
            slot.timestamp_ns = timestamp_ns;
            slot.width = IMAGE_WIDTH;
            slot.height = IMAGE_HEIGHT;
            slot.buffer_id = buffer_id;
            slot.format = 0;
            producer.publish();
        }
    }

    pub fn publish_pose(
        &mut self,
        index: PoseIndex,
        position: [f32; 3],
        quaternion: [f32; 4],
        frame_seq: u64,
        timestamp_ns: u64,
    ) {
        self.publish_pose_with_aux(
            index,
            position,
            quaternion,
            [0.0; 4],
            frame_seq,
            timestamp_ns,
        );
    }

    pub fn publish_pose_with_aux(
        &mut self,
        index: PoseIndex,
        position: [f32; 3],
        quaternion: [f32; 4],
        aux_f32: [f32; 4],
        frame_seq: u64,
        timestamp_ns: u64,
    ) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            let pose_buf = &mut meta.poses[index as usize];
            let mut producer = TripleBufferProducer::new(
                &pose_buf.state,
                &mut pose_buf.write_idx,
                &mut pose_buf.slots,
            );

            let slot = producer.borrow_mut();
            slot.frame_seq = frame_seq;
            slot.position = position;
            slot.quaternion = quaternion;
            slot.timestamp_ns = timestamp_ns;
            slot._pad = aux_f32_to_bytes(aux_f32);

            producer.publish();
        }
    }

    pub fn set_camera_info(&mut self, info: CameraInfo) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.camera_info = info;
        }
    }

    pub fn publish_chassis_observation(&mut self, observation: ChassisObservation) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.chassis_observation = observation;
        }
    }

    pub fn update_heartbeat(&mut self) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.header.heartbeat_ns = Self::now_ns();
        }
    }

    fn now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    /// 初始化 TripleBuffer 到正确的初始状态
    ///
    /// ShmRegion::create() 使用零填充，会破坏 TripleBuffer 的正确初始状态。
    /// 必须手动重新初始化。
    ///
    /// 正确初始状态:
    /// - state = 1 (ready slot 是 1, 无 FLAG_NEW)
    /// - write_idx = 0 (生产者写入 slot 0)
    /// - read_idx = 2 (消费者上次读取 slot 2)
    fn init_triple_buffer(buf: &mut impl TripleBufferInit) {
        buf.init_state();
    }
}

fn aux_f32_to_bytes(aux_f32: [f32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (i, value) in aux_f32.iter().enumerate() {
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Trait for initializing triple buffer state
trait TripleBufferInit {
    fn init_state(&mut self);
}

impl TripleBufferInit for ImageTripleBuffer {
    fn init_state(&mut self) {
        self.state.store(1, Ordering::Relaxed);
        self.write_idx = 0;
        self.read_idx = 2;
    }
}

impl TripleBufferInit for PoseTripleBuffer {
    fn init_state(&mut self) {
        self.state.store(1, Ordering::Relaxed);
        self.write_idx = 0;
        self.read_idx = 2;
    }
}

impl TripleBufferInit for GimbalTripleBuffer {
    fn init_state(&mut self) {
        self.state.store(1, Ordering::Relaxed);
        self.write_idx = 0;
        self.read_idx = 2;
    }
}
