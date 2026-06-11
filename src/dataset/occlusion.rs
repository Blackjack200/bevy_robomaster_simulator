use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthSample {
    pub pixel: UVec2,
    pub expected_depth_m: f32,
}

pub const DEPTH_EPSILON_M: f32 = 0.05;

pub fn linearize_reverse_z(depth: f32, near: f32) -> f32 {
    if depth <= f32::EPSILON {
        return f32::INFINITY;
    }
    near / depth
}

pub fn entity_fully_visible_in_depth(
    width: u32,
    height: u32,
    depth_bytes: &[u8],
    near: f32,
    tolerance_m: f32,
    samples: &[DepthSample],
) -> bool {
    !samples.is_empty()
        && samples.iter().all(|sample| {
            sample_visible_in_depth(width, height, depth_bytes, near, tolerance_m, *sample)
        })
}

pub fn sample_visible_in_depth(
    width: u32,
    height: u32,
    depth_bytes: &[u8],
    near: f32,
    tolerance_m: f32,
    sample: DepthSample,
) -> bool {
    if sample.pixel.x >= width || sample.pixel.y >= height || sample.expected_depth_m <= 0.0 {
        return false;
    }

    let pixel_index = sample.pixel.y as usize * width as usize + sample.pixel.x as usize;
    let byte_index = pixel_index * 4;
    if byte_index + 4 > depth_bytes.len() {
        return false;
    }

    let depth = f32::from_le_bytes([
        depth_bytes[byte_index],
        depth_bytes[byte_index + 1],
        depth_bytes[byte_index + 2],
        depth_bytes[byte_index + 3],
    ]);
    let measured_depth_m = linearize_reverse_z(depth, near);
    measured_depth_m + tolerance_m >= sample.expected_depth_m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_depths(depths_m: &[f32], near: f32) -> Vec<u8> {
        let mut out = Vec::with_capacity(depths_m.len() * 4);
        for depth_m in depths_m {
            let reverse_z = near / depth_m;
            out.extend_from_slice(&reverse_z.to_le_bytes());
        }
        out
    }

    #[test]
    fn sample_visible_when_depth_matches_target() {
        let near = 0.1;
        let depth = encode_depths(&[2.0], near);
        assert!(sample_visible_in_depth(
            1,
            1,
            &depth,
            near,
            DEPTH_EPSILON_M,
            DepthSample {
                pixel: UVec2::ZERO,
                expected_depth_m: 2.0,
            },
        ));
    }

    #[test]
    fn sample_occluded_when_blocker_is_closer() {
        let near = 0.1;
        let depth = encode_depths(&[1.0], near);
        assert!(!sample_visible_in_depth(
            1,
            1,
            &depth,
            near,
            DEPTH_EPSILON_M,
            DepthSample {
                pixel: UVec2::ZERO,
                expected_depth_m: 2.0,
            },
        ));
    }

    #[test]
    fn entity_requires_all_samples_visible() {
        let near = 0.1;
        let depth = encode_depths(&[2.0, 1.0], near);
        assert!(!entity_fully_visible_in_depth(
            2,
            1,
            &depth,
            near,
            DEPTH_EPSILON_M,
            &[
                DepthSample {
                    pixel: UVec2::new(0, 0),
                    expected_depth_m: 2.0,
                },
                DepthSample {
                    pixel: UVec2::new(1, 0),
                    expected_depth_m: 2.0,
                },
            ],
        ));
    }
}
