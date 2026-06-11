use crate::robomaster::prelude::{ArmorLabel, ArmorType};
use bevy::prelude::*;
use image::ExtendedColorType::Rgb8;
use image::codecs::jpeg::JpegEncoder;
use std::fs::{File, create_dir_all};
use std::io::ErrorKind::Other;
use std::io::{BufWriter, Error, Write};
use std::path::{Path, PathBuf};

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum ArmorColor {
    Blue = 0,
    Red = 1,
    Gray = 2,
    Purple = 3,
}

#[derive(Debug, Clone)]
pub struct ArmorEntry {
    pub color: ArmorColor,
    pub typ: ArmorType,
    pub label: ArmorLabel,
    pub points: [Vec2; 4],
}

pub struct DatasetWriter {
    image_dir: PathBuf,
    label_dir: PathBuf,
    depth_dir: PathBuf,
    seq: u64,
}

impl DatasetWriter {
    pub fn new(directory: &str) -> std::io::Result<Self> {
        let base = Path::new(directory);
        let image_dir = base.join("images");
        let label_dir = base.join("label");
        let depth_dir = base.join("depth");

        create_dir_all(&image_dir)?;
        create_dir_all(&label_dir)?;
        create_dir_all(&depth_dir)?;

        Ok(Self {
            image_dir,
            label_dir,
            depth_dir,
            seq: 0,
        })
    }

    pub fn next_frame_name(&mut self) -> String {
        self.seq += 1;
        format!("frame_{:06}", self.seq)
    }

    pub fn write_color_entry(
        &mut self,
        frame: &str,
        height: u32,
        width: u32,
        data: &[u8],
        entries: &[ArmorEntry],
    ) -> std::io::Result<()> {
        self.save_image(
            height,
            width,
            data,
            &self.image_dir.join(format!("{}.jpg", frame)),
        )?;
        let mut writer =
            BufWriter::new(File::create(self.label_dir.join(format!("{}.txt", frame)))?);

        for entry in entries {
            write!(
                writer,
                "{} {} {}",
                entry.color as u8, entry.typ as u8, entry.label as u8
            )?;
            for p in &entry.points {
                write!(writer, " {:.6} {:.6}", p.x, p.y)?;
            }
            writeln!(writer)?;
        }

        writer.flush()?;
        Ok(())
    }

    pub fn write_depth_entry(
        &mut self,
        frame: &str,
        width: u32,
        height: u32,
        depth_bytes: &[u8],
        near: f32,
        far: f32,
    ) -> std::io::Result<()> {
        let depth_mm = depth_bytes_to_mm(depth_bytes, near, far);
        let mut bytes = Vec::with_capacity(depth_mm.len() * 2);
        for value in depth_mm {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        image::save_buffer(
            self.depth_dir.join(format!("{}.png", frame)),
            bytes.as_slice(),
            width,
            height,
            image::ColorType::L16,
        )
        .map_err(|e| Error::new(Other, e))?;
        Ok(())
    }

    fn save_image(&self, height: u32, width: u32, data: &[u8], path: &Path) -> std::io::Result<()> {
        JpegEncoder::new(&mut File::create(path)?)
            .encode(data, width, height, Rgb8)
            .map_err(|e| Error::new(Other, e))?;
        Ok(())
    }
}

fn depth_bytes_to_mm(data: &[u8], near: f32, far: f32) -> Vec<u16> {
    data.chunks_exact(4)
        .map(|chunk| {
            let depth = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let meters = if depth <= f32::EPSILON {
                far
            } else {
                (near / depth).clamp(near, far)
            };
            (meters * 1000.0).round().clamp(0.0, u16::MAX as f32) as u16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_z_depth_converts_to_mm() {
        let raw = (0.1f32).to_le_bytes();
        let got = depth_bytes_to_mm(&raw, 0.1, 80.0);
        assert_eq!(got[0], 1000);
    }
}
