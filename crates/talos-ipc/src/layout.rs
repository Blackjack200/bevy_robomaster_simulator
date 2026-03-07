use std::sync::atomic::AtomicU8;

pub const IMAGE_WIDTH: u32 = 1440;
pub const IMAGE_HEIGHT: u32 = 1080;

pub const CACHE_LINE_SIZE: usize = 64;
pub const SHM_MAGIC: u32 = 0x54414C05;
pub const SHM_VERSION: u32 = 1;

pub const IMAGE_CHANNELS: u32 = 3;
pub const IMAGE_SIZE: usize = (IMAGE_WIDTH * IMAGE_HEIGHT * IMAGE_CHANNELS) as usize;
pub const IMAGE_POOL_SIZE: usize = IMAGE_SIZE * 3;
pub const SHM_NAME_META: &str = "talos_ipc_meta";
pub const SHM_NAME_IMAGE_POOL: &str = "talos_ipc_image_pool";

pub const FLAG_NEW: u8 = 0x80;
pub const INDEX_MASK: u8 = 0x03;

#[repr(C, align(32))]
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageMeta {
    pub seq: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    pub buffer_id: u8,
    pub format: u8,
    pub _pad: [u8; 6],
}
const _: () = assert!(size_of::<ImageMeta>() == 32);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct PoseMeta {
    pub frame_seq: u64,
    pub position: [f32; 3],
    pub quaternion: [f32; 4],
    pub timestamp_ns: u64,
    pub _pad: [u8; 16],
}
const _: () = assert!(size_of::<PoseMeta>() == 64);

impl Default for PoseMeta {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            position: [0.0; 3],
            quaternion: [0.0; 4],
            timestamp_ns: 0,
            _pad: [0; 16],
        }
    }
}

#[repr(C, align(32))]
#[derive(Debug, Clone, Copy, Default)]
pub struct GimbalCmd {
    pub timestamp_ns: u64,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub distance_m: f32,
    pub fire_advice: u8,
    pub _pad: [u8; 11],
}
const _: () = assert!(size_of::<GimbalCmd>() == 32);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraInfo {
    pub timestamp_ns: u64,
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub distortion: [f64; 5],
    pub width: u32,
    pub height: u32,
    pub _pad: [u8; 24],
}
const _: () = assert!(size_of::<CameraInfo>() == 128);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct ChassisObservation {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub dt_s: f32,
    pub v_body: [f32; 2],
    pub wz_radps: f32,
    pub wheel_linear_mps: [f32; 4],
    pub wheel_angular_radps: [f32; 4],
    pub a_body: [f32; 2],
    pub alpha_z_radps2: f32,
    pub rpy_rad: [f32; 3],
    pub gyro_xyz_radps: [f32; 3],
    pub accel_xyz_mps2: [f32; 3],
    pub _pad: [u8; 16],
}
const _: () = assert!(size_of::<ChassisObservation>() == 128);

impl Default for ChassisObservation {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            timestamp_ns: 0,
            dt_s: 0.0,
            v_body: [0.0; 2],
            wz_radps: 0.0,
            wheel_linear_mps: [0.0; 4],
            wheel_angular_radps: [0.0; 4],
            a_body: [0.0; 2],
            alpha_z_radps2: 0.0,
            rpy_rad: [0.0; 3],
            gyro_xyz_radps: [0.0; 3],
            accel_xyz_mps2: [0.0; 3],
            _pad: [0; 16],
        }
    }
}

#[repr(C, align(64))]
pub struct ImageTripleBuffer {
    pub state: AtomicU8,
    pub write_idx: u8,
    pub read_idx: u8,
    pub _pad1: [u8; 61],
    pub slots: [ImageMeta; 3],
}
const _: () = assert!(size_of::<ImageTripleBuffer>() == 192);

#[repr(C, align(64))]
pub struct PoseTripleBuffer {
    pub state: AtomicU8,
    pub write_idx: u8,
    pub read_idx: u8,
    pub _pad1: [u8; 61],
    pub slots: [PoseMeta; 3],
}
const _: () = assert!(size_of::<PoseTripleBuffer>() == 256);

#[repr(C, align(64))]
pub struct GimbalTripleBuffer {
    pub state: AtomicU8,
    pub write_idx: u8,
    pub read_idx: u8,
    pub _pad1: [u8; 61],
    pub slots: [GimbalCmd; 3],
}
const _: () = assert!(size_of::<GimbalTripleBuffer>() == 192);

#[repr(C, align(64))]
pub struct ShmHeader {
    pub magic: u32,
    pub version: u32,
    pub created_ns: u64,
    pub heartbeat_ns: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub _pad: [u8; 32],
}
const _: () = assert!(size_of::<ShmHeader>() == 64);

#[repr(C)]
pub struct ShmMetaRegion {
    pub header: ShmHeader,
    pub image: ImageTripleBuffer,
    pub poses: [PoseTripleBuffer; 5],
    pub gimbal_cmd: GimbalTripleBuffer,
    pub camera_info: CameraInfo,
    // ABI-compatible extension:
    // Reuse tail reserved bytes as a dedicated chassis observation payload.
    // Existing readers that treat this area as padding remain compatible.
    pub chassis_observation: ChassisObservation,
    pub _pad: [u8; 64],
}
const _: () = assert!(size_of::<ShmMetaRegion>() == 2048);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, camera_info) == 1728);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, chassis_observation) == 1856);

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoseIndex {
    Gimbal = 0,
    Odom = 1,
    Muzzle = 2,
    Camera = 3,
    // Legacy compatibility channel.
    // New integrations should consume `ShmMetaRegion::chassis_observation` instead.
    ChassisObservation = 4,
}

impl Default for ImageTripleBuffer {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(1),
            write_idx: 0,
            read_idx: 2,
            _pad1: [0; 61],
            slots: [ImageMeta::default(); 3],
        }
    }
}

impl Default for PoseTripleBuffer {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(1),
            write_idx: 0,
            read_idx: 2,
            _pad1: [0; 61],
            slots: [PoseMeta::default(); 3],
        }
    }
}

impl Default for GimbalTripleBuffer {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(1),
            write_idx: 0,
            read_idx: 2,
            _pad1: [0; 61],
            slots: [GimbalCmd::default(); 3],
        }
    }
}

impl Default for ShmHeader {
    fn default() -> Self {
        Self {
            magic: SHM_MAGIC,
            version: SHM_VERSION,
            created_ns: 0,
            heartbeat_ns: 0,
            image_width: IMAGE_WIDTH,
            image_height: IMAGE_HEIGHT,
            _pad: [0; 32],
        }
    }
}

impl Default for ShmMetaRegion {
    fn default() -> Self {
        Self {
            header: ShmHeader::default(),
            image: ImageTripleBuffer::default(),
            poses: [
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
            ],
            gimbal_cmd: GimbalTripleBuffer::default(),
            camera_info: CameraInfo::default(),
            chassis_observation: ChassisObservation::default(),
            _pad: [0; 64],
        }
    }
}
