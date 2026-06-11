//! 视频播放集成回归：解码线程产帧 → 主线程上传队列 → Canvas emit DrawCmd::Image。

use std::thread;
use std::time::Duration;

use compose::foundation::Constraints;
use compose::layout::{Alignment, DrawCmd, Modifier, create_root_node, draw_tree, perform_layout};
use compose::runtime::{Composition, Recomposer};
use compose_material::{
    MockVideoBackend, VideoPlayback, VideoPlayer, VideoPlayerController, VideoSource,
};

#[test]
fn decoded_frame_is_drawn_as_image() {
    let pb = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(32, 18, 2_000_000, 12)));
    let c = VideoPlayerController::new(pb);
    c.initialize();
    c.play();

    let mut got = false;
    for _ in 0..100 {
        c.on_frame();
        if c.frame().is_some() {
            got = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(got, "解码线程应把一帧送进主线程邮箱");

    let ctl = c.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        VideoPlayer(ctl.clone(), Modifier::new().fill_max_width());
    });
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 240.0));
    let cmds = draw_tree(root);
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::Image { .. })),
        "视频画面应 emit DrawCmd::Image，命令: {cmds:?}"
    );

    c.dispose();
}

#[cfg(compose_ffmpeg)]
#[test]
fn ffmpeg_backend_decodes_real_bytes() {
    use image::{RgbaImage, codecs::gif::GifEncoder};

    let img = RgbaImage::new(8, 8);
    let mut bytes = Vec::new();
    GifEncoder::new(&mut bytes)
        .encode(img.as_raw(), 8, 8, image::ColorType::Rgba8.into())
        .expect("生成测试 GIF");

    let path = std::env::temp_dir().join("compose_video_smoke.gif");
    std::fs::write(&path, &bytes).expect("写入测试 GIF");
    let c = VideoPlayerController::file(path.to_string_lossy().to_string())
        .expect("FFmpeg 应能打开 GIF 文件");
    assert!(c.value().is_initialized);
    c.play();
    let mut got = false;
    for _ in 0..100 {
        c.on_frame();
        if let Some(f) = c.frame() {
            assert_eq!((f.width, f.height), (8, 8));
            got = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(got, "FFmpeg 后端应解出一帧 RGBA");
    c.dispose();
}

#[cfg(compose_ffmpeg)]
#[test]
fn ffmpeg_backend_decodes_memory_bytes() {
    use image::{RgbaImage, codecs::gif::GifEncoder};

    let img = RgbaImage::new(8, 8);
    let mut bytes = Vec::new();
    GifEncoder::new(&mut bytes)
        .encode(img.as_raw(), 8, 8, image::ColorType::Rgba8.into())
        .expect("生成测试 GIF");
    let c = VideoPlayerController::memory(bytes, "gif").expect("FFmpeg 应能打开内存 GIF");
    c.play();
    let mut got = false;
    for _ in 0..100 {
        c.on_frame();
        if c.frame().is_some() {
            got = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(got, "内存来源也应解出帧");
    c.dispose();
}

#[cfg(compose_ffmpeg)]
#[test]
fn ffmpeg_invalid_memory_source_fails_cleanly() {
    let c = VideoPlayerController::memory(vec![0, 1, 2, 3], "");
    assert!(c.is_none(), "无效字节不应打开成功，也不能在失败路径双重释放");
}

#[test]
fn open_with_error_reports_reason() {
    let err = VideoPlayerController::open_with_error(VideoSource::file(
        "/definitely/not/existing/video.mp4",
    ))
    .err()
    .expect("不存在的文件应打开失败");
    assert!(!err.is_empty(), "错误原因不能是空串，UI 才能回显");
}

#[test]
fn controller_seek_updates_value_immediately() {
    let pb = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(32, 18, 4_000_000, 20)));
    let c = VideoPlayerController::new(pb);
    c.initialize();
    c.seek_to(1_000_000);
    assert_eq!(c.value().position_us, 1_000_000);
    c.seek_to(9_000_000);
    assert_eq!(c.value().position_us, 4_000_000, "seek 应夹取到时长");
    c.dispose();
}
