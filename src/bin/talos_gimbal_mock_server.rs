use clap::Parser;
use ffmpeg_next as ffmpeg;
use std::error::Error;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Use the talos-ipc crate instead of local modules
use talos_ipc::{
    CameraInfo, IMAGE_HEIGHT, IMAGE_SIZE, IMAGE_WIDTH, PoseIndex, ShmPublisher, ShmSubscriber,
};

type DynError = Box<dyn Error + Send + Sync + 'static>;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Talos mock server: ffmpeg 视频输入 + 接收 GimbalCmd"
)]
struct Args {
    /// 视频输入地址: 本地文件、RTSP/RTMP/HTTP 均可
    #[arg(long, short)]
    input: String,
    /// 读到 EOF 后循环重播
    #[arg(long, default_value_t = false)]
    r#loop: bool,
    /// 发布节流帧率, <=0 表示不节流
    #[arg(long, default_value_t = 30.0)]
    fps: f64,
    /// 每 N 帧打印一次发布进度
    #[arg(long, default_value_t = 60)]
    log_every: u64,
}

fn main() -> Result<(), DynError> {
    let mut args = Args::parse();
    args.input = resolve_input(&args.input)?;
    ffmpeg::init()?;

    let mut publisher = ShmPublisher::create()?;
    let mut subscriber = ShmSubscriber::connect()?;
    publisher.set_camera_info(default_camera_info());

    let frame_interval = if args.fps > 0.0 {
        Some(Duration::from_secs_f64(1.0 / args.fps))
    } else {
        None
    };
    let mut frame_seq = 0_u64;

    println!(
        "talos mock server started: input={}, loop={}, fps={}",
        args.input, args.r#loop, args.fps
    );

    loop {
        let published = publish_one_input(
            &args.input,
            &mut publisher,
            &mut subscriber,
            &mut frame_seq,
            frame_interval,
            args.log_every.max(1),
        )?;

        if !args.r#loop || published == 0 {
            break;
        }
    }

    println!("talos mock server stopped, total published frames={frame_seq}");
    Ok(())
}

fn publish_one_input(
    input: &str,
    publisher: &mut ShmPublisher,
    subscriber: &mut ShmSubscriber,
    frame_seq: &mut u64,
    frame_interval: Option<Duration>,
    log_every: u64,
) -> Result<u64, DynError> {
    let mut ictx = ffmpeg::format::input(input)?;
    let input_stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| format!("no video stream found in input: {input}"))?;
    let video_stream_index = input_stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(input_stream.parameters())?;
    let mut decoder = context.decoder().video()?;
    let mut scaler = ffmpeg::software::scaling::context::Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        ffmpeg::format::Pixel::RGB24,
        IMAGE_WIDTH,
        IMAGE_HEIGHT,
        ffmpeg::software::scaling::flag::Flags::BILINEAR,
    )?;

    let mut decoded = ffmpeg::util::frame::Video::empty();
    let mut rgb_frame = ffmpeg::util::frame::Video::empty();
    let mut rgb_buf = vec![0_u8; IMAGE_SIZE];
    let mut published = 0_u64;
    let mut next_deadline = Instant::now();

    for (stream, packet) in ictx.packets() {
        if stream.index() != video_stream_index {
            continue;
        }

        decoder.send_packet(&packet)?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            scaler.run(&decoded, &mut rgb_frame)?;
            copy_frame_rgb24(&rgb_frame, &mut rgb_buf)?;
            publish_frame(publisher, subscriber, &rgb_buf, *frame_seq);
            *frame_seq += 1;
            published += 1;

            if published % log_every == 0 {
                println!("published {published} frames in this pass, global seq={frame_seq}");
            }

            if let Some(interval) = frame_interval {
                next_deadline += interval;
                if let Some(wait) = next_deadline.checked_duration_since(Instant::now()) {
                    thread::sleep(wait);
                } else {
                    next_deadline = Instant::now();
                }
            }
        }
    }

    decoder.send_eof()?;
    while decoder.receive_frame(&mut decoded).is_ok() {
        scaler.run(&decoded, &mut rgb_frame)?;
        copy_frame_rgb24(&rgb_frame, &mut rgb_buf)?;
        publish_frame(publisher, subscriber, &rgb_buf, *frame_seq);
        *frame_seq += 1;
        published += 1;
    }

    Ok(published)
}

fn publish_frame(
    publisher: &mut ShmPublisher,
    subscriber: &mut ShmSubscriber,
    rgb_frame: &[u8],
    frame_seq: u64,
) {
    let timestamp_ns = now_ns();
    publisher.publish_image(rgb_frame, frame_seq, timestamp_ns);
    publish_mock_poses(publisher, frame_seq, timestamp_ns);
    publisher.update_heartbeat();

    if let Some(cmd) = subscriber.recv_gimbal_cmd() {
        println!(
            "recv gimbal cmd: ts={} yaw={:.2} pitch={:.2} dist={:.3} fire={}",
            cmd.timestamp_ns, cmd.yaw_deg, cmd.pitch_deg, cmd.distance_m, cmd.fire_advice
        );
    }
}

fn publish_mock_poses(publisher: &mut ShmPublisher, frame_seq: u64, timestamp_ns: u64) {
    let ident = [1.0, 0.0, 0.0, 0.0];
    publisher.publish_pose(
        PoseIndex::Odom,
        [0.0, 0.0, 0.0],
        ident,
        frame_seq,
        timestamp_ns,
    );
    publisher.publish_pose(
        PoseIndex::Gimbal,
        [0.0, 0.0, 0.0],
        ident,
        frame_seq,
        timestamp_ns,
    );
    publisher.publish_pose(
        PoseIndex::Muzzle,
        [0.0, 0.0, 0.2],
        ident,
        frame_seq,
        timestamp_ns,
    );
    publisher.publish_pose(
        PoseIndex::Camera,
        [0.0, 0.0, 0.0],
        ident,
        frame_seq,
        timestamp_ns,
    );
}

fn copy_frame_rgb24(frame: &ffmpeg::util::frame::Video, dst: &mut [u8]) -> Result<(), DynError> {
    if dst.len() != IMAGE_SIZE {
        return Err(format!(
            "rgb dst size mismatch: expect {}, got {}",
            IMAGE_SIZE,
            dst.len()
        )
        .into());
    }

    let data = frame.data(0);
    let stride = frame.stride(0);
    let row_bytes = IMAGE_WIDTH as usize * 3;
    let height = IMAGE_HEIGHT as usize;

    for y in 0..height {
        let src_start = y * stride;
        let src_end = src_start + row_bytes;
        let dst_start = y * row_bytes;
        let dst_end = dst_start + row_bytes;

        if src_end > data.len() {
            return Err(format!(
                "frame buffer too small: src_end={}, data_len={}",
                src_end,
                data.len()
            )
            .into());
        }
        dst[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
    }
    Ok(())
}

fn default_camera_info() -> CameraInfo {
    CameraInfo {
        timestamp_ns: now_ns(),
        fx: IMAGE_WIDTH as f64,
        fy: IMAGE_HEIGHT as f64,
        cx: IMAGE_WIDTH as f64 / 2.0,
        cy: IMAGE_HEIGHT as f64 / 2.0,
        distortion: [0.0; 5],
        width: IMAGE_WIDTH,
        height: IMAGE_HEIGHT,
        _pad: [0; 24],
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn resolve_input(input: &str) -> Result<String, DynError> {
    if input.contains("://") {
        return Ok(input.to_string());
    }
    if Path::new(input).exists() {
        return Ok(input.to_string());
    }

    let decoded = input
        .replace("\\u{202f}", "\u{202f}")
        .replace("\\u202f", "\u{202f}");
    if Path::new(&decoded).exists() {
        return Ok(decoded);
    }

    let nbsp_fixed = input
        .replace(" AM", "\u{202f}AM")
        .replace(" PM", "\u{202f}PM");
    if Path::new(&nbsp_fixed).exists() {
        return Ok(nbsp_fixed);
    }

    Err(format!(
        "input not found: {input}\nHint: macOS screen recording filenames often contain U+202F before AM/PM. \
Use tab completion or wildcard like: .../5.44.33*PM.mov"
    )
    .into())
}
