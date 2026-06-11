//! 视频播放演示：内置 Mock 帧源可直接播放；也可通过原生文件对话框打开本地视频。
//!
//! 解码线程产出 RGBA → 单槽邮箱 → 主线程帧驱动上传纹理 → `Video` 画面逐帧 emit
//! `DrawCmd::Image`。进度条点击/拖动 seek，控制条显示媒体时钟与解码信息。

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::prelude::*;
use compose::runtime::add_frame_driver;

fn mock_controller() -> VideoPlayerController {
    let pb = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(320, 180, 8_000_000, 240)));
    let c = VideoPlayerController::new(pb);
    c.initialize();
    c.set_looping(true);
    c.play();
    c
}

pub(crate) fn page_video() {
    let slot = remember(|| Rc::new(RefCell::new(mock_controller())));
    let rev = remember(|| mutableStateOf(0u64));
    let wait = remember(|| Arc::new(AtomicBool::new(false)));
    let err = remember(|| mutableStateOf(String::new()));
    // 控制条来源：false = 包内内置（自动隐藏），true = 下方 controls_builder 换掉的自定义控件。
    let custom = remember(|| mutableStateOf(false));
    let _ = rev.get();

    // 通道与帧驱动都必须只创建一次。本页每帧都会因进度/时钟状态重组：若在组合体里
    // 直接 `channel()`，remember 留住首次的 Receiver，而每次重组拿到的都是新 Sender，
    // 新 Sender 发往一个已被丢弃的 Receiver → 路径永远收不到；同时重复注册的帧驱动
    // 又会让 wait 一直续帧，表现为「不停闪」。把它们放进 remember 与驱动闭包里固定身份。
    let channel = remember(|| {
        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        Rc::new((tx, RefCell::new(rx)))
    });
    let wait_driver = wait.clone();
    let slot_driver = slot.clone();
    let rev_driver = rev;
    let err_driver = err;
    let driver_channel = channel.clone();
    let _driver = remember(|| {
        add_frame_driver(move || {
            while let Ok(path) = driver_channel.1.borrow().try_recv() {
                match VideoPlayerController::open_with_error(VideoSource::file(
                    path.to_string_lossy().to_string(),
                )) {
                    Ok(c) => {
                        c.play();
                        let old = std::mem::replace(&mut *slot_driver.borrow_mut(), c);
                        old.dispose();
                        err_driver.set(String::new());
                        rev_driver.set(rev_driver.get().wrapping_add(1));
                    }
                    Err(e) => err_driver.set(format!("打开 {} 失败：{e}", path.display())),
                }
                wait_driver.store(false, Ordering::SeqCst);
            }
            if wait_driver.load(Ordering::SeqCst) {
                request_anim_frames();
            }
        })
    });

    let controller = slot.borrow().clone();
    let cap = video_capabilities();

    Text(
        "视频播放：默认播放内置 Mock 帧；点“打开文件”选择本地 mp4/mkv 等（需 FFmpeg）。",
        TextStyle::new(),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    let controller_row = controller.clone();
    let tx = channel.0.clone();
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let c = controller_row.clone();
            TextButton(if c.value().is_playing { "暂停" } else { "播放" }, Modifier::new(), move || {
                c.toggle_play();
                request_anim_frames();
            });

            let c = controller_row.clone();
            TextButton("停止", Modifier::new(), move || {
                c.pause();
                c.seek_to(0);
            });

            let c = controller_row.clone();
            TextButton("重播", Modifier::new(), move || {
                c.seek_to(0);
                c.play();
            });

            let wait_click = wait.clone();
            let tx = tx.clone();
            TextButton("打开文件…", Modifier::new(), move || {
                wait_click.store(true, Ordering::SeqCst);
                request_anim_frames();
                let tx = tx.clone();
                let wait_cb = wait_click.clone();
                open_file_dialog(
                    "打开视频",
                    &[
                        FileFilter::new("视频", &["mp4", "mkv", "avi", "mov", "webm", "gif", "ts", "flv", "m4v"]),
                        FileFilter::new("所有文件", &["*"]),
                    ],
                    move |p| {
                        if let Some(p) = p {
                            // 回调已在主线程，直接入队；wait 由帧驱动在消费后清除。
                            let _ = tx.send(p);
                            // advance_frame 早于 drain_main_tasks，显式再要一帧确保下次驱动消费到路径。
                            request_anim_frames();
                        } else {
                            wait_cb.store(false, Ordering::SeqCst);
                        }
                    },
                );
            });
        },
    );

    Spacer(Modifier::new().height(12.0));

    // 画面 + 底部播放控件（自动隐藏）：鼠标移入即显形、静止 2s 或移出画面收起，
    // 点画面切换播放/暂停。`custom` 打开时整条控制条换成下面 emit 的自定义内容。
    let is_custom = custom.get();
    let stage_ctl = controller.clone();
    let builder: Option<VideoControlsBuilder> = if is_custom {
        let c = stage_ctl.clone();
        Some(Rc::new(move |show: bool| {
            let v = c.value();
            let label = format!(
                "自定义控件 · {} · {}（点我切换播放）",
                if v.is_playing { "暂停" } else { "播放" },
                if show { "显示" } else { "隐藏" }
            );
            let tap_ctl = c.clone();
            let text_ctl = c.clone();
            Clickable(
                Modifier::new()
                    .fill_max_width()
                    .background(Color::rgba(0.13, 0.35, 0.75, 0.85), 0.0),
                move || {
                    tap_ctl.toggle_play();
                },
                move || {
                    let t = format!(
                        "{} · {:.1}s",
                        label,
                        text_ctl.value().position_us as f64 / 1_000_000.0
                    );
                    Text(
                        t,
                        TextStyle::new().color(Color::rgb(1.0, 1.0, 1.0)).size(13.0),
                        Modifier::new().padding(8.0),
                    );
                },
            );
        }))
    } else {
        None
    };
    VideoPlayerWidget(
        stage_ctl,
        Modifier::new().fill_max_width().height(270.0),
        true,
        builder,
    );

    Spacer(Modifier::new().height(8.0));
    let cu = custom;
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let flag = cu;
            TextButton(
                if flag.get() { "切回内置控件" } else { "使用自定义控件" },
                Modifier::new(),
                move || flag.set(!flag.get()),
            );
        },
    );

    Spacer(Modifier::new().height(10.0));
    let v = controller.value();
    let open_error = err.read_clone();
    Text(
        format!(
            "后端: {} | FFmpeg: {} | 画面: {}×{} | 时长: {:.2}s | 位置: {:.2}s | 音量: {:.0}%{}{}",
            cap.summary(),
            if ffmpeg_compiled() { "是" } else { "否" },
            v.width,
            v.height,
            v.duration_us as f64 / 1_000_000.0,
            v.position_us as f64 / 1_000_000.0,
            v.volume * 100.0,
            v.error_description
                .as_ref()
                .map(|e| format!(" | 解码错误: {e}"))
                .unwrap_or_default(),
            if open_error.is_empty() {
                String::new()
            } else {
                format!(" | {open_error}")
            },
        ),
        TextStyle::new(),
        Modifier::new(),
    );
}
