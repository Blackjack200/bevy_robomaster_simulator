use bevy::prelude::*;
use image::codecs::jpeg::JpegEncoder;
use image::{ImageBuffer, Rgb};
use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ArmorEntry {
    pub color: u8,
    pub label: u8,
    pub points: [Vec2; 4],
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

    pub fn write_entry(&mut self, image: &Image, entries: Vec<ArmorEntry>) -> std::io::Result<()> {
        let frame = self.next_frame_name();

        let img_path = self.image_dir.join(format!("{}.jpg", frame));
        self.save_image(image, &img_path)?;

        let label_path = self.label_dir.join(format!("{}.txt", frame));
        let file = File::create(label_path)?;
        let mut writer = BufWriter::new(file);

        for entry in entries {
            write!(writer, "{} {}", entry.color, entry.label)?;
            for p in &entry.points {
                write!(writer, " {:.6} {:.6}", p.x, p.y)?;
            }
            writeln!(writer)?;
        }

        writer.flush()?;
        Ok(())
    }

    fn save_image(&self, img: &Image, path: &Path) -> std::io::Result<()> {
        let data = img.clone().try_into_dynamic().unwrap().to_rgb8().into_raw();
        let buffer = ImageBuffer::<Rgb<u8>, _>::from_raw(img.width(), img.height(), data).unwrap();
        JpegEncoder::new(&mut File::create(path)?)
            .encode_image(&buffer)
            .unwrap();
        Ok(())
    }
}
