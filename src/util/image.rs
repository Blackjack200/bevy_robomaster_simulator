use r2r::sensor_msgs::msg::CompressedImage;
use r2r::std_msgs::msg::Header;

pub fn compress_image(header: Header, width: u32, height: u32, data: Vec<u8>) -> CompressedImage {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ImageBuffer, Rgb};
    use std::io::Cursor;

    let buffer: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(width, height, data).unwrap();

    let mut cursor = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new(&mut cursor);
    encoder.encode_image(&buffer).expect("JPEG encode failed");
    let compressed_data = cursor.into_inner();

    CompressedImage {
        header,
        format: "jpeg".to_string(),
        data: compressed_data,
    }
}
