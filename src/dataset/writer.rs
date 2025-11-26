use bevy::prelude::*;
use image::codecs::jpeg::JpegEncoder;
use image::{ImageBuffer, Rgb};
use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ArmorEntry {
    pub color: ArmorColor,
    pub label: ArmorLabel,
    pub points: [Vec2; 4],
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum ArmorColor {
    Blue = 0,
    Red = 1,
    Gray = 2,
    Purple = 3,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum ArmorLabel {
    G = 0,
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    O = 5,
    Bs = 6,
    Bb = 7,
}

#[derive(Resource)]
pub struct DatasetWriter {
    image_dir: PathBuf,
    label_dir: PathBuf,
    seq: u64,
}

impl DatasetWriter {
    pub fn new(directory: &str) -> std::io::Result<Self> {
        let base = Path::new(directory);
        let image_dir = base.join("images");
        let label_dir = base.join("labels");

        create_dir_all(&image_dir)?;
        create_dir_all(&label_dir)?;

        Ok(Self {
            image_dir,
            label_dir,
            seq: 0,
        })
    }

    fn next_frame_name(&mut self) -> String {
        self.seq += 1;
        format!("frame_{:06}", self.seq)
    }

    pub fn write_entry(
        &mut self,
        height: u32,
        width: u32,
        data: Vec<u8>,
        entries: Vec<ArmorEntry>,
    ) -> std::io::Result<()> {
        let frame = self.next_frame_name();

        let img_path = self.image_dir.join(format!("{}.jpg", frame));
        self.save_image(height, width, data, &img_path)?;

        let label_path = self.label_dir.join(format!("{}.txt", frame));
        let file = File::create(label_path)?;
        let mut writer = BufWriter::new(file);

        for entry in entries {
            write!(writer, "{} {}", entry.color as u8, entry.label as u8)?;
            for p in &entry.points {
                write!(writer, " {:.6} {:.6}", p.x, p.y)?;
            }
            writeln!(writer)?;
        }

        writer.flush()?;
        Ok(())
    }

    fn save_image(
        &self,
        height: u32,
        width: u32,
        data: Vec<u8>,
        path: &Path,
    ) -> std::io::Result<()> {
        let buffer = ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, data).unwrap();
        JpegEncoder::new(&mut File::create(path)?)
            .encode_image(&buffer)
            .unwrap();
        Ok(())
    }
}
