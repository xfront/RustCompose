//! 视频播放：来源探测 / 解码后端表 / FFmpeg 真解码 / 播放状态机 / 控制器 / Composable 控件。
//!
//! 对齐 `/work/study/cui/rui_packages/src/video.rs` 的分层：
//! - 数据层：`VideoSource` / `VideoInfo` / `VideoFrame` / `AudioChunk`；
//! - 后端层：`VideoDecoderBackend` trait + 后端目录/偏好/能力（FFmpeg 由 `build.rs`
//!   探测到系统/vcpkg FFmpeg 时启用 `cfg(compose_ffmpeg)`，否则只有 Mock）；
//! - 播放层：`VideoPlayback`（媒体时钟 + seek/循环/倍速/音量）在解码线程上运行；
//! - 控件层：`Video` 画面、可拖动进度条、`VideoControls`、`VideoPlayer`。
//!
//! 帧显示复用 core 的 `image_cache`：解码线程只把 RGBA 帧送进有界 channel，主线程帧驱动
//! 取最新帧、逐帧分配新 `image_id` 并 `submit_decoded`，`Canvas` 的绘制闭包 emit
//! `DrawCmd::Image`。旧纹理同帧 `drop_image`，避免每帧新建纹理导致显存无限增长。
//!
//! 音频：`FFmpeg` 后端能探测音轨并把 `has_audio_stream` 报给上层，但本工程尚无音频输出
//! 后端，因此播放是静音的（与参考实现的 FFmpeg 后端能力边界一致）。

use std::cell::{Cell, RefCell};
#[cfg(compose_ffmpeg)]
use std::ffi::CString;
use std::ffi::{c_char, c_int, c_void};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use compose::animation::{animate_float_as_state, request_anim_frames, Tween};
use compose::foundation::Color;
use compose::gesture::{detect_tap_gestures, register, tap_config, unregister};
use compose::layout::{
    add_node, node_abs_rect, Alignment, Arrangement, Box, Canvas, Clickable, Column,
    CrossAxisAlignment, DrawBuilder, Modifier, Policy, Row, TextStyle, hoverable,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::render::image_cache;
use compose::runtime::{
    add_frame_driver, disposable_effect, mutableStateOf, remember, remove_frame_driver, with_rt_pub,
    MutableState,
};

use super::basic::TextButton;
use super::file_dialog::FileFilter;
use super::{open_file_dialog, Text};

// ── 数据与后端接口 ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoMediaKind {
    #[default]
    Unknown,
    VideoContainer,
    AnimatedImage,
    StaticImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoSourceType {
    Asset,
    Network,
    File,
    ContentUri,
}
impl Default for VideoSourceType {
    fn default() -> Self {
        Self::File
    }
}

#[derive(Clone, Debug, Default)]
pub struct VideoSource {
    pub source_type: VideoSourceType,
    pub uri: String,
    pub format_hint: String,
    pub bytes: Vec<u8>,
}
impl VideoSource {
    pub fn file(path: impl Into<String>) -> Self {
        Self { source_type: VideoSourceType::File, uri: path.into(), ..Default::default() }
    }
    pub fn network(url: impl Into<String>) -> Self {
        Self { source_type: VideoSourceType::Network, uri: url.into(), ..Default::default() }
    }
    pub fn asset(name: impl Into<String>) -> Self {
        Self { source_type: VideoSourceType::Asset, uri: name.into(), ..Default::default() }
    }
    pub fn memory(bytes: Vec<u8>, format_hint: impl Into<String>) -> Self {
        Self {
            source_type: VideoSourceType::File,
            uri: String::new(),
            format_hint: format_hint.into(),
            bytes,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub duration_us: i64,
    pub frame_rate: f64,
    pub rotation: i32,
    pub has_audio: bool,
    pub kind: VideoMediaKind,
    pub codec: String,
}

#[derive(Clone, Debug, Default)]
pub struct VideoFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub pts_us: i64,
    pub seq: u64,
}
impl VideoFrame {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn valid(&self) -> bool {
        self.width > 0 && self.height > 0 && !self.rgba.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug, Default)]
pub struct AudioChunk {
    pub format: AudioFormat,
    pub pts_us: i64,
    pub samples: Vec<i16>,
}

/// 解码后端 trait：宿主可注入真后端，控制层只依赖此接口。
pub trait VideoDecoderBackend: Send {
    fn kind(&self) -> VideoBackendKind;
    fn info(&self) -> Option<VideoInfo>;
    fn read_frame(&mut self) -> Option<VideoFrame>;
    fn seek_to(&mut self, pts_us: i64);
    fn at_end(&self) -> bool;
    fn error(&self) -> Option<String> {
        None
    }
    fn has_audio_stream(&self) -> bool {
        false
    }
    fn audio_format(&self) -> Option<AudioFormat> {
        None
    }
    fn read_audio(&mut self) -> Option<AudioChunk> {
        None
    }
    fn flush_audio(&mut self) {}
    fn pump_driven(&self) -> bool {
        false
    }
    fn pump_set_playing(&mut self, _playing: bool) {}
    fn pump_set_speed(&mut self, _speed: f64) {}
    fn pump_set_volume(&mut self, _volume: f64) {}
    fn pump_position_us(&self) -> Option<i64> {
        None
    }
}

// ── 后端表 / 偏好 / 能力 ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VideoBackendKind {
    System,
    Ffmpeg,
    AnimatedImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoBackendPreference {
    Auto,
    SystemFirst,
    FfmpegFirst,
    SystemOnly,
    FfmpegOnly,
}
impl Default for VideoBackendPreference {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoBackendStatus {
    pub name: String,
    pub kind: VideoBackendKind,
    pub compiled: bool,
    pub available: bool,
    pub self_audio: bool,
    pub audio_name: String,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct VideoBackendCatalog {
    entries: Vec<VideoBackendStatus>,
}
impl VideoBackendCatalog {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn catalog(&self) -> Vec<VideoBackendStatus> {
        self.entries.clone()
    }
    pub fn preferred(&self) -> Option<VideoBackendKind> {
        self.entries.iter().find(|e| e.available).map(|e| e.kind)
    }
}

#[derive(Clone, Debug, Default)]
pub struct VideoCapabilities {
    pub ffmpeg: bool,
    pub animated_image: bool,
    pub native_audio: bool,
    pub platform_decoder: bool,
    pub system_decoder: bool,
    pub system_name: String,
    pub system_audio_name: String,
    pub native_name: String,
}
impl VideoCapabilities {
    pub fn summary(&self) -> String {
        if self.native_name.is_empty() {
            "只有 Mock 后端（无 FFmpeg / 系统解码）".into()
        } else if self.native_audio {
            format!("{}（有声：{}）", self.native_name, self.system_audio_name)
        } else {
            format!("{}（无声 + 墙钟）", self.native_name)
        }
    }
}

pub type VideoBackendFactory =
    Rc<dyn Fn(&VideoSource, &mut String) -> Option<Box<dyn VideoDecoderBackend>>>;

#[derive(Clone)]
pub struct VideoBackendRegistration {
    pub name: String,
    pub audio_name: String,
    pub factory: Option<VideoBackendFactory>,
    pub platform_decoder: bool,
}

thread_local! {
    static PREFERENCE: Cell<VideoBackendPreference> = const { Cell::new(VideoBackendPreference::Auto) };
    static REGISTRY: RefCell<Vec<VideoBackendRegistration>> = const { RefCell::new(Vec::new()) };
}

pub fn set_video_backend_preference(p: VideoBackendPreference) {
    PREFERENCE.with(|c| c.set(p));
}
pub fn video_backend_preference() -> VideoBackendPreference {
    PREFERENCE.with(|c| c.get())
}
pub fn register_video_backend(reg: VideoBackendRegistration) -> bool {
    if reg.factory.is_none() {
        return false;
    }
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(old) = r.iter_mut().find(|x| x.name == reg.name) {
            *old = reg;
        } else {
            r.push(reg);
        }
    });
    true
}
pub fn clear_registered_video_backends() {
    REGISTRY.with(|r| r.borrow_mut().clear());
}
pub fn registered_video_backends() -> Vec<VideoBackendRegistration> {
    REGISTRY.with(|r| r.borrow().clone())
}

pub fn ffmpeg_compiled() -> bool {
    cfg!(compose_ffmpeg)
}

fn preference_order(pref: VideoBackendPreference) -> Vec<VideoBackendKind> {
    match pref {
        VideoBackendPreference::FfmpegFirst => {
            vec![VideoBackendKind::Ffmpeg, VideoBackendKind::System]
        }
        VideoBackendPreference::SystemOnly => vec![VideoBackendKind::System],
        VideoBackendPreference::FfmpegOnly => vec![VideoBackendKind::Ffmpeg],
        VideoBackendPreference::Auto | VideoBackendPreference::SystemFirst => {
            vec![VideoBackendKind::System, VideoBackendKind::Ffmpeg]
        }
    }
}

pub fn video_backend_catalog() -> VideoBackendCatalog {
    let order = preference_order(video_backend_preference());
    let mut entries = Vec::new();
    for kind in order {
        match kind {
            VideoBackendKind::System => {
                for reg in registered_video_backends() {
                    entries.push(VideoBackendStatus {
                        name: reg.name,
                        kind: VideoBackendKind::System,
                        compiled: reg.factory.is_some(),
                        available: reg.factory.is_some(),
                        self_audio: false,
                        audio_name: reg.audio_name,
                        note: String::new(),
                    });
                }
            }
            VideoBackendKind::Ffmpeg if cfg!(compose_ffmpeg) => entries.push(VideoBackendStatus {
                name: "FFmpeg".into(),
                kind: VideoBackendKind::Ffmpeg,
                compiled: true,
                available: true,
                self_audio: false,
                audio_name: "FFmpeg+平台输出（待接入）".into(),
                note: "视频解码可用，当前无音频输出".into(),
            }),
            _ => {}
        }
    }
    VideoBackendCatalog { entries }
}

pub fn video_capabilities() -> VideoCapabilities {
    let catalog = video_backend_catalog();
    let first = catalog.entries.iter().find(|e| e.available);
    let name = first.map(|e| e.name.clone()).unwrap_or_default();
    VideoCapabilities {
        ffmpeg: cfg!(compose_ffmpeg),
        animated_image: false,
        native_audio: false,
        platform_decoder: first.map(|e| e.kind == VideoBackendKind::System).unwrap_or(false),
        system_decoder: first.map(|e| e.kind == VideoBackendKind::System).unwrap_or(false),
        system_audio_name: first.map(|e| e.audio_name.clone()).unwrap_or_default(),
        system_name: name.clone(),
        native_name: name,
    }
}

fn create_backend(source: &VideoSource, error: &mut String) -> Option<Box<dyn VideoDecoderBackend>> {
    for status in video_backend_catalog().catalog() {
        if !status.available {
            continue;
        }
        if status.kind == VideoBackendKind::Ffmpeg {
            #[cfg(compose_ffmpeg)]
            if let Some(b) = FfmpegVideoBackend::open(source, error) {
                return Some(Box::new(b));
            }
            #[cfg(not(compose_ffmpeg))]
            let _ = error;
        } else {
            for reg in registered_video_backends() {
                if reg.name != status.name {
                    continue;
                }
                if let Some(factory) = &reg.factory {
                    if let Some(b) = factory(source, error) {
                        return Some(b);
                    }
                }
            }
        }
    }
    if error.is_empty() {
        *error = if cfg!(compose_ffmpeg) {
            "没有可用解码后端".into()
        } else {
            "FFmpeg 未编译进来，也没有注册后端".into()
        };
    }
    None
}

// ── FFmpeg 后端 ─────────────────────────────────────────────────────────────

#[cfg(compose_ffmpeg)]
mod ffi {
    use super::*;

    unsafe extern "C" {
        pub fn compose_ffmpeg_open(
            uri: *const c_char,
            bytes: *const u8,
            bytes_len: c_int,
            format_hint: *const c_char,
            width: *mut c_int,
            height: *mut c_int,
            duration_us: *mut i64,
            frame_rate: *mut f64,
            rotation: *mut c_int,
            has_audio: *mut c_int,
            codec_name: *mut c_char,
            codec_name_len: c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> *mut c_void;
        pub fn compose_ffmpeg_read(
            ctx: *mut c_void,
            rgba: *mut *mut u8,
            width: *mut c_int,
            height: *mut c_int,
            pts_us: *mut i64,
        ) -> c_int;
        pub fn compose_ffmpeg_release_buffer(ptr: *mut u8);
        pub fn compose_ffmpeg_seek(ctx: *mut c_void, pts_us: i64);
        pub fn compose_ffmpeg_at_end(ctx: *mut c_void) -> c_int;
        pub fn compose_ffmpeg_error(ctx: *mut c_void) -> *const c_char;
        pub fn compose_ffmpeg_close(ctx: *mut c_void);
    }
}

#[cfg(compose_ffmpeg)]
pub struct FfmpegVideoBackend {
    ctx: *mut c_void,
    info: VideoInfo,
}
#[cfg(compose_ffmpeg)]
unsafe impl Send for FfmpegVideoBackend {}
#[cfg(compose_ffmpeg)]
impl FfmpegVideoBackend {
    pub fn open(source: &VideoSource, error: &mut String) -> Option<Self> {
        let uri = CString::new(source.uri.as_str()).unwrap_or_default();
        let hint = CString::new(source.format_hint.as_str()).unwrap_or_default();
        let mut w: c_int = 0;
        let mut h: c_int = 0;
        let mut duration = 0i64;
        let mut fps = 0.0f64;
        let mut rotation: c_int = 0;
        let mut audio: c_int = 0;
        let mut codec = [0 as c_char; 64];
        let mut err = [0 as c_char; 256];
        let bytes_ptr = if source.bytes.is_empty() {
            std::ptr::null()
        } else {
            source.bytes.as_ptr()
        };
        let ctx = unsafe {
            ffi::compose_ffmpeg_open(
                if source.bytes.is_empty() { uri.as_ptr() } else { std::ptr::null() },
                bytes_ptr,
                source.bytes.len() as c_int,
                if source.format_hint.is_empty() { std::ptr::null() } else { hint.as_ptr() },
                &mut w,
                &mut h,
                &mut duration,
                &mut fps,
                &mut rotation,
                &mut audio,
                codec.as_mut_ptr(),
                codec.len() as c_int,
                err.as_mut_ptr(),
                err.len() as c_int,
            )
        };
        if ctx.is_null() {
            let msg = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .to_string();
            *error = if msg.is_empty() { "FFmpeg 打开失败".into() } else { msg };
            return None;
        }
        let codec_name = unsafe { std::ffi::CStr::from_ptr(codec.as_ptr()) }
            .to_string_lossy()
            .to_string();
        let kind = if w == 0 || h == 0 {
            VideoMediaKind::Unknown
        } else if fps > 0.0 {
            VideoMediaKind::VideoContainer
        } else {
            VideoMediaKind::StaticImage
        };
        Some(Self {
            ctx,
            info: VideoInfo {
                width: w as u32,
                height: h as u32,
                duration_us: duration,
                frame_rate: fps,
                rotation: rotation as i32,
                has_audio: audio != 0,
                kind,
                codec: codec_name,
            },
        })
    }
}
#[cfg(compose_ffmpeg)]
impl Drop for FfmpegVideoBackend {
    fn drop(&mut self) {
        unsafe { ffi::compose_ffmpeg_close(self.ctx) };
    }
}
#[cfg(compose_ffmpeg)]
impl VideoDecoderBackend for FfmpegVideoBackend {
    fn kind(&self) -> VideoBackendKind {
        VideoBackendKind::Ffmpeg
    }
    fn info(&self) -> Option<VideoInfo> {
        Some(self.info.clone())
    }
    fn read_frame(&mut self) -> Option<VideoFrame> {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let (mut w, mut h, mut pts) = (0 as c_int, 0 as c_int, 0i64);
        let ret = unsafe { ffi::compose_ffmpeg_read(self.ctx, &mut ptr, &mut w, &mut h, &mut pts) };
        if ret <= 0 || ptr.is_null() {
            return None;
        }
        let len = (w.max(0) as usize) * (h.max(0) as usize) * 4;
        let rgba = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { ffi::compose_ffmpeg_release_buffer(ptr) };
        Some(VideoFrame {
            rgba,
            width: w as u32,
            height: h as u32,
            pts_us: pts,
            seq: 0,
        })
    }
    fn seek_to(&mut self, pts_us: i64) {
        unsafe { ffi::compose_ffmpeg_seek(self.ctx, pts_us) };
    }
    fn at_end(&self) -> bool {
        unsafe { ffi::compose_ffmpeg_at_end(self.ctx) != 0 }
    }
    fn error(&self) -> Option<String> {
        let p = unsafe { ffi::compose_ffmpeg_error(self.ctx) };
        if p.is_null() {
            return None;
        }
        let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
    fn has_audio_stream(&self) -> bool {
        self.info.has_audio
    }
}

// ── 播放状态机 ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Idle,
    Initialized,
    Playing,
    Paused,
    Ended,
    Error,
}

fn state_index(s: PlaybackState) -> u8 {
    match s {
        PlaybackState::Idle => 0,
        PlaybackState::Initialized => 1,
        PlaybackState::Playing => 2,
        PlaybackState::Paused => 3,
        PlaybackState::Ended => 4,
        PlaybackState::Error => 5,
    }
}
fn state_from_index(v: u8) -> PlaybackState {
    match v {
        1 => PlaybackState::Initialized,
        2 => PlaybackState::Playing,
        3 => PlaybackState::Paused,
        4 => PlaybackState::Ended,
        5 => PlaybackState::Error,
        _ => PlaybackState::Idle,
    }
}

pub struct VideoPlayback {
    backend: Box<dyn VideoDecoderBackend>,
    state: PlaybackState,
    initialized: bool,
    completed: bool,
    looping: bool,
    speed: f64,
    volume: f64,
    position_us: i64,
    base_position_us: i64,
    base_instant: Option<Instant>,
    last_frame: VideoFrame,
    seq: Arc<AtomicU64>,
    error: Option<String>,
    disposed: bool,
}
impl VideoPlayback {
    pub fn new(backend: Box<dyn VideoDecoderBackend>) -> Self {
        Self::with_backend(backend)
    }
    pub fn with_backend(backend: Box<dyn VideoDecoderBackend>) -> Self {
        Self {
            backend,
            state: PlaybackState::Idle,
            initialized: false,
            completed: false,
            looping: false,
            speed: 1.0,
            volume: 1.0,
            position_us: 0,
            base_position_us: 0,
            base_instant: None,
            last_frame: VideoFrame::new(),
            seq: Arc::new(AtomicU64::new(0)),
            error: None,
            disposed: false,
        }
    }
    pub fn create(source: &VideoSource, error: &mut String) -> Option<Self> {
        let backend = create_backend(source, error)?;
        let mut p = Self::with_backend(backend);
        if !p.initialize() {
            *error = p.error().unwrap_or_else(|| "视频初始化失败".into());
            return None;
        }
        Some(p)
    }
    pub fn state(&self) -> PlaybackState {
        self.state
    }
    pub fn info(&self) -> Option<VideoInfo> {
        self.backend.info()
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub fn is_playing(&self) -> bool {
        self.state == PlaybackState::Playing
    }
    pub fn is_completed(&self) -> bool {
        self.completed
    }
    pub fn position_us(&self) -> i64 {
        if self.completed {
            return self.duration_us();
        }
        if self.state == PlaybackState::Playing {
            if let Some(t) = self.backend.pump_position_us() {
                return (t.max(0) as i64).min(self.duration_us().max(0));
            }
            if let Some(base) = self.base_instant {
                let elapsed = base.elapsed().as_micros() as f64 * self.speed;
                return (self.base_position_us as f64 + elapsed).round() as i64;
            }
        }
        self.position_us
    }
    pub fn duration_us(&self) -> i64 {
        self.info().map(|i| i.duration_us).unwrap_or(0).max(0)
    }
    pub fn speed(&self) -> f64 {
        self.speed
    }
    pub fn volume(&self) -> f64 {
        self.volume
    }
    pub fn error(&self) -> Option<String> {
        self.error.clone().or_else(|| self.backend.error())
    }
    pub fn has_audio_output(&self) -> bool {
        self.backend.has_audio_stream()
    }

    pub fn initialize(&mut self) -> bool {
        if self.disposed {
            return false;
        }
        if self.backend.info().is_some() {
            self.initialized = true;
            self.state = PlaybackState::Initialized;
            self.error = None;
            self.seek_to(0);
            true
        } else {
            self.state = PlaybackState::Error;
            self.error = Some("视频打开失败".into());
            false
        }
    }
    pub fn play(&mut self) {
        if self.disposed || self.state == PlaybackState::Error {
            return;
        }
        if self.completed || self.state == PlaybackState::Ended {
            self.completed = false;
            self.seek_to(0);
        }
        self.state = PlaybackState::Playing;
        self.base_position_us = self.position_us;
        self.base_instant = Some(Instant::now());
        self.backend.pump_set_playing(true);
    }
    pub fn pause(&mut self) {
        if self.disposed {
            return;
        }
        if self.state == PlaybackState::Playing {
            self.position_us = self.position_us();
            self.base_instant = None;
            self.state = PlaybackState::Paused;
            self.backend.pump_set_playing(false);
        }
    }
    pub fn seek_to(&mut self, pts_us: i64) {
        if self.disposed {
            return;
        }
        let duration = self.duration_us();
        let target = pts_us.max(0).min(duration.max(0));
        self.position_us = target;
        self.base_position_us = target;
        self.base_instant = if self.state == PlaybackState::Playing {
            Some(Instant::now())
        } else {
            None
        };
        self.completed = !self.looping && duration > 0 && target >= duration;
        if self.completed {
            self.state = PlaybackState::Ended;
            self.backend.pump_set_playing(false);
        }
        self.backend.seek_to(target);
        self.backend.flush_audio();
    }
    pub fn set_playback_speed(&mut self, s: f64) -> bool {
        if self.disposed || !s.is_finite() || s <= 0.0 {
            return false;
        }
        self.position_us = self.position_us();
        self.base_position_us = self.position_us;
        self.base_instant = if self.state == PlaybackState::Playing {
            Some(Instant::now())
        } else {
            None
        };
        self.speed = s;
        self.backend.pump_set_speed(s);
        true
    }
    pub fn set_volume(&mut self, v: f64) {
        if self.disposed {
            return;
        }
        self.volume = v.clamp(0.0, 1.0);
        self.backend.pump_set_volume(self.volume);
    }
    pub fn set_looping(&mut self, v: bool) {
        if self.disposed {
            return;
        }
        self.looping = v;
        if v {
            self.completed = false;
        }
    }
    pub fn looping(&self) -> bool {
        self.looping
    }

    /// 帧循环：用媒体时钟推进位置并取最新帧（由解码线程独占调用）。
    pub fn pump(&mut self) -> Option<VideoFrame> {
        if self.disposed {
            return None;
        }
        if self.state != PlaybackState::Playing {
            return self.last_frame.valid().then(|| self.last_frame.clone());
        }
        let duration = self.duration_us();
        let pos = self.position_us();
        if duration > 0 && pos >= duration {
            if self.looping {
                self.seek_to(0);
                self.state = PlaybackState::Playing;
                self.base_instant = Some(Instant::now());
            } else {
                self.position_us = duration;
                self.completed = true;
                self.state = PlaybackState::Ended;
                self.base_instant = None;
                self.backend.pump_set_playing(false);
                return None;
            }
        }
        let frame = self.backend.read_frame();
        if let Some(mut f) = frame {
            f.seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
            self.last_frame = f.clone();
            return Some(f);
        }
        if self.backend.at_end() {
            if self.looping {
                self.seek_to(0);
                self.state = PlaybackState::Playing;
                self.base_instant = Some(Instant::now());
            } else if let Some(e) = self.backend.error() {
                self.error = Some(e);
                self.state = PlaybackState::Error;
                self.base_instant = None;
            } else {
                self.completed = true;
                self.state = PlaybackState::Ended;
                self.base_instant = None;
            }
        }
        None
    }
    pub fn take_latest_frame(&self) -> Option<VideoFrame> {
        self.last_frame.valid().then(|| self.last_frame.clone())
    }
    pub fn has_pending_frame(&self) -> bool {
        self.last_frame.valid()
    }
    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.state = PlaybackState::Ended;
        self.base_instant = None;
        self.backend.pump_set_playing(false);
    }
}

// ── 解码线程与控制器 ────────────────────────────────────────────────────────

const NO_SEEK: i64 = i64::MIN;

struct WorkerCommands {
    init: AtomicBool,
    playing: AtomicBool,
    stop: AtomicBool,
    seek_us: AtomicI64,
    speed_bits: AtomicU64,
    volume_bits: AtomicU64,
    looping: AtomicBool,
    position_us: AtomicI64,
    state: AtomicU8,
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameSlot {
    id: u64,
    token: u64,
    w: u32,
    h: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DurationRange {
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoPlayerValue {
    pub duration_us: i64,
    pub position_us: i64,
    pub width: u32,
    pub height: u32,
    pub buffered: Vec<DurationRange>,
    pub is_initialized: bool,
    pub is_playing: bool,
    pub is_looping: bool,
    pub is_buffering: bool,
    pub is_completed: bool,
    pub volume: f64,
    pub playback_speed: f64,
    pub rotation_correction: i32,
    pub error_description: Option<String>,
    pub prevents_display_sleep: bool,
}
impl Default for VideoPlayerValue {
    fn default() -> Self {
        Self {
            duration_us: 0,
            position_us: 0,
            width: 0,
            height: 0,
            buffered: Vec::new(),
            is_initialized: false,
            is_playing: false,
            is_looping: false,
            is_buffering: false,
            is_completed: false,
            volume: 1.0,
            playback_speed: 1.0,
            rotation_correction: 0,
            error_description: None,
            prevents_display_sleep: true,
        }
    }
}
impl VideoPlayerValue {
    pub fn has_error(&self) -> bool {
        self.error_description.is_some()
    }
    pub fn aspect_ratio(&self) -> f64 {
        if !self.is_initialized || self.width == 0 || self.height == 0 {
            return 1.0;
        }
        let r = self.width as f64 / self.height as f64;
        if r > 0.0 {
            r
        } else {
            1.0
        }
    }
}

type ValueListener = Rc<dyn Fn(VideoPlayerValue)>;

pub struct VideoPlayerController {
    cmds: Arc<WorkerCommands>,
    info: Arc<Mutex<Option<VideoInfo>>>,
    error: Arc<Mutex<Option<String>>>,
    value: MutableState<VideoPlayerValue>,
    frame: Rc<Cell<FrameSlot>>,
    latest: Rc<RefCell<Option<VideoFrame>>>,
    mailbox: Arc<Mutex<Option<VideoFrame>>>,
    worker: Rc<RefCell<Option<JoinHandle<()>>>>,
    listeners: Rc<RefCell<Vec<(u64, ValueListener)>>>,
    next_listener: Rc<Cell<u64>>,
    disposed: Rc<Cell<bool>>,
    source: VideoSource,
    session: u64,
}
impl Clone for VideoPlayerController {
    fn clone(&self) -> Self {
        Self {
            cmds: self.cmds.clone(),
            info: self.info.clone(),
            error: self.error.clone(),
            value: self.value,
            frame: self.frame.clone(),
            latest: self.latest.clone(),
            mailbox: self.mailbox.clone(),
            worker: self.worker.clone(),
            listeners: self.listeners.clone(),
            next_listener: self.next_listener.clone(),
            disposed: self.disposed.clone(),
            source: self.source.clone(),
            session: self.session,
        }
    }
}
impl VideoPlayerController {
    pub fn new(playback: VideoPlayback) -> Self {
        Self::from_playback(playback, VideoSource::default())
    }
    pub fn with_backend(backend: Box<dyn VideoDecoderBackend>) -> Self {
        Self::from_playback(VideoPlayback::with_backend(backend), VideoSource::default())
    }
    fn from_playback(mut playback: VideoPlayback, source: VideoSource) -> Self {
        let cmds = Arc::new(WorkerCommands {
            init: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            seek_us: AtomicI64::new(NO_SEEK),
            speed_bits: AtomicU64::new(1.0f64.to_bits()),
            volume_bits: AtomicU64::new(1.0f64.to_bits()),
            looping: AtomicBool::new(false),
            position_us: AtomicI64::new(0),
            state: AtomicU8::new(state_index(PlaybackState::Idle)),
        });
        let session = Arc::as_ptr(&cmds) as usize as u64;
        let info = Arc::new(Mutex::new(playback.info()));
        let error = Arc::new(Mutex::new(playback.error()));
        let mailbox = Arc::new(Mutex::new(None::<VideoFrame>));
        let worker_cmds = cmds.clone();
        let worker_info = info.clone();
        let worker_error = error.clone();
        let worker_mailbox = mailbox.clone();
        let worker = thread::spawn(move || {
            run_decoder_loop(
                &mut playback,
                &worker_cmds,
                &worker_info,
                &worker_error,
                &worker_mailbox,
            );
        });
        let c = Self {
            cmds,
            info,
            error,
            value: mutableStateOf(VideoPlayerValue::default()),
            frame: Rc::new(Cell::new(FrameSlot::default())),
            latest: Rc::new(RefCell::new(None)),
            mailbox,
            worker: Rc::new(RefCell::new(Some(worker))),
            listeners: Rc::new(RefCell::new(Vec::new())),
            next_listener: Rc::new(Cell::new(1)),
            disposed: Rc::new(Cell::new(false)),
            source,
            session,
        };
        c.sync_value();
        c
    }
    pub fn file(path: impl Into<String>) -> Option<Self> {
        Self::open(VideoSource::file(path))
    }
    pub fn network_url(url: impl Into<String>) -> Option<Self> {
        Self::open(VideoSource::network(url))
    }
    pub fn asset(name: impl Into<String>) -> Option<Self> {
        Self::open(VideoSource::asset(name))
    }
    pub fn memory(bytes: Vec<u8>, hint: impl Into<String>) -> Option<Self> {
        Self::open(VideoSource::memory(bytes, hint))
    }
    pub fn open(source: VideoSource) -> Option<Self> {
        Self::open_with_error(source).ok()
    }
    /// 打开视频源；失败时返回具体原因（供 UI 回显，避免静默无反应）。
    pub fn open_with_error(source: VideoSource) -> Result<Self, String> {
        let mut error = String::new();
        let playback = VideoPlayback::create(&source, &mut error).ok_or_else(|| {
            if error.is_empty() {
                "视频打开失败（无可用解码后端）".to_string()
            } else {
                error
            }
        })?;
        let c = Self::from_playback(playback, source);
        c.initialize();
        Ok(c)
    }
    pub fn value(&self) -> VideoPlayerValue {
        self.value.read_clone()
    }
    pub fn data_source(&self) -> String {
        self.source.uri.clone()
    }
    pub fn is_same_session(&self, other: &Self) -> bool {
        self.session == other.session
    }
    pub fn is_initialized(&self) -> bool {
        self.value().is_initialized
    }
    pub fn session_id(&self) -> u64 {
        self.session
    }

    pub fn add_listener(&self, cb: impl Fn(VideoPlayerValue) + 'static) -> u64 {
        let id = self.next_listener.get();
        self.next_listener.set(id + 1);
        self.listeners.borrow_mut().push((id, Rc::new(cb)));
        id
    }
    pub fn remove_listener_by_handle(&self, id: u64) {
        self.listeners.borrow_mut().retain(|(h, _)| *h != id);
    }
    fn notify(&self, v: VideoPlayerValue) {
        let cbs: Vec<ValueListener> = self.listeners.borrow().iter().map(|(_, cb)| cb.clone()).collect();
        for cb in cbs {
            cb(v.clone());
        }
    }
    fn sync_value(&self) {
        let mut v = self.value();
        let old = v.clone();
        let pos = self.cmds.position_us.load(Ordering::SeqCst);
        let state = state_from_index(self.cmds.state.load(Ordering::SeqCst));
        let info = self.info.lock().unwrap().clone();
        v.duration_us = info.as_ref().map(|i| i.duration_us).unwrap_or(0).max(0);
        v.width = info.as_ref().map(|i| i.width).unwrap_or(0);
        v.height = info.as_ref().map(|i| i.height).unwrap_or(0);
        v.rotation_correction = info.as_ref().map(|i| i.rotation).unwrap_or(0);
        v.position_us = pos.clamp(0, v.duration_us.max(0));
        v.is_initialized = matches!(state, PlaybackState::Initialized | PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended);
        v.is_playing = state == PlaybackState::Playing && self.cmds.playing.load(Ordering::SeqCst);
        v.is_looping = self.cmds.looping.load(Ordering::SeqCst);
        v.is_completed = state == PlaybackState::Ended;
        v.volume = f64::from_bits(self.cmds.volume_bits.load(Ordering::SeqCst));
        v.playback_speed = f64::from_bits(self.cmds.speed_bits.load(Ordering::SeqCst));
        v.error_description = self.error.lock().unwrap().clone();
        if v != old {
            self.value.set(v.clone());
            self.notify(v);
        }
    }

    /// 主线程帧驱动：取解码线程的最新帧 → 上传队列；返回是否仍在播放。
    pub fn on_frame(&self) -> bool {
        if self.disposed.get() {
            return false;
        }
        let last = self.mailbox.lock().unwrap().take();
        if let Some(f) = last {
            if f.valid() {
                let id = image_cache::next_image_id();
                image_cache::submit_decoded(id, f.rgba.clone(), f.width, f.height);
                let prev = self.frame.get();
                if prev.id != 0 {
                    image_cache::drop_image(prev.id);
                }
                self.frame.set(FrameSlot {
                    id,
                    token: prev.token.wrapping_add(1),
                    w: f.width,
                    h: f.height,
                });
                *self.latest.borrow_mut() = Some(f);
            }
        }
        self.sync_value();
        let playing = self.cmds.playing.load(Ordering::SeqCst);
        if playing {
            request_anim_frames();
        }
        playing
    }

    pub fn initialize(&self) -> bool {
        if self.disposed.get() {
            return false;
        }
        self.cmds.init.store(true, Ordering::SeqCst);
        self.cmds.state.store(state_index(PlaybackState::Initialized), Ordering::SeqCst);
        self.sync_value();
        true
    }
    pub fn play(&self) {
        if self.disposed.get() {
            return;
        }
        self.cmds.playing.store(true, Ordering::SeqCst);
        self.sync_value();
        request_anim_frames();
    }
    pub fn pause(&self) {
        if self.disposed.get() {
            return;
        }
        self.cmds.playing.store(false, Ordering::SeqCst);
        self.sync_value();
    }
    pub fn toggle_play(&self) {
        if self.cmds.playing.load(Ordering::SeqCst) {
            self.pause();
        } else {
            self.play();
        }
    }
    pub fn seek_to(&self, us: i64) {
        if self.disposed.get() {
            return;
        }
        let duration = self.duration_us();
        let target = us.max(0).min(duration.max(0));
        // 乐观反馈：位置立刻反映到 value，不等解码线程下一拍（Compose 状态写入语义）。
        self.cmds.position_us.store(target, Ordering::SeqCst);
        self.cmds.seek_us.store(us, Ordering::SeqCst);
        if duration > 0 && target >= duration && !self.cmds.looping.load(Ordering::SeqCst) {
            self.cmds.state.store(state_index(PlaybackState::Ended), Ordering::SeqCst);
            self.cmds.playing.store(false, Ordering::SeqCst);
        }
        self.sync_value();
        request_anim_frames();
    }
    pub fn set_volume(&self, v: f64) {
        if self.disposed.get() {
            return;
        }
        self.cmds.volume_bits.store(v.clamp(0.0, 1.0).to_bits(), Ordering::SeqCst);
        self.sync_value();
    }
    pub fn set_playback_speed(&self, s: f64) -> bool {
        if self.disposed.get() || !s.is_finite() || s <= 0.0 {
            return false;
        }
        self.cmds.speed_bits.store(s.to_bits(), Ordering::SeqCst);
        self.sync_value();
        true
    }
    pub fn set_looping(&self, v: bool) {
        if self.disposed.get() {
            return;
        }
        self.cmds.looping.store(v, Ordering::SeqCst);
        self.sync_value();
    }
    pub fn set_prevents_display_sleep(&self, v: bool) {
        if self.disposed.get() {
            return;
        }
        self.value.peek_mut(|x| x.prevents_display_sleep = v);
    }
    pub fn position_us(&self) -> i64 {
        self.value().position_us
    }
    pub fn duration_us(&self) -> i64 {
        self.value().duration_us
    }
    pub fn frame(&self) -> Option<VideoFrame> {
        self.latest.borrow().clone()
    }
    pub fn pump(&self) -> Option<VideoFrame> {
        self.on_frame();
        self.frame()
    }
    pub fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.cmds.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.worker.borrow_mut().take() {
            let _ = h.join();
        }
        let prev = self.frame.get();
        if prev.id != 0 {
            image_cache::drop_image(prev.id);
            self.frame.set(FrameSlot::default());
        }
        self.listeners.borrow_mut().clear();
    }
}
impl Drop for VideoPlayerController {
    fn drop(&mut self) {
        // 控制器是可 Clone 的共享句柄：只有最后一个句柄离开时才结束解码线程。
        // 否则任一临时 clone 被丢弃都会把会话 dispose 掉（帧 slot 一并清空）。
        if Rc::strong_count(&self.worker) == 1 {
            self.dispose();
        }
    }
}

fn run_decoder_loop(
    p: &mut VideoPlayback,
    cmds: &WorkerCommands,
    info: &Mutex<Option<VideoInfo>>,
    error: &Mutex<Option<String>>,
    mailbox: &Mutex<Option<VideoFrame>>,
) {
    let mut was_playing = false;
    let mut last_speed = 1.0f64;
    let mut last_volume = 1.0f64;
    let mut pending: Option<VideoFrame> = None;

    while !cmds.stop.load(Ordering::SeqCst) {
        if cmds.init.swap(false, Ordering::SeqCst) {
            p.initialize();
            *info.lock().unwrap() = p.info();
        }
        let seek = cmds.seek_us.swap(NO_SEEK, Ordering::SeqCst);
        if seek != NO_SEEK {
            p.seek_to(seek);
            pending = None;
        }
        let speed = f64::from_bits(cmds.speed_bits.load(Ordering::SeqCst));
        if speed != last_speed {
            p.set_playback_speed(speed);
            last_speed = speed;
        }
        let volume = f64::from_bits(cmds.volume_bits.load(Ordering::SeqCst));
        if volume != last_volume {
            p.set_volume(volume);
            last_volume = volume;
        }
        if p.looping() != cmds.looping.load(Ordering::SeqCst) {
            p.set_looping(cmds.looping.load(Ordering::SeqCst));
        }

        let playing = cmds.playing.load(Ordering::SeqCst);
        if playing && !was_playing {
            p.play();
        } else if !playing && was_playing {
            p.pause();
        }
        was_playing = playing;

        if playing {
            let pos = p.position_us();
            if let Some(f) = pending.take() {
                // 按帧 PTS 对齐墙钟：早到的帧先扣住，避免解码线程跑得比显示快而积压。
                if f.pts_us <= pos.saturating_add(20_000) {
                    *mailbox.lock().unwrap() = Some(f);
                } else {
                    pending = Some(f);
                    thread::sleep(Duration::from_millis(1));
                }
            } else {
                match p.pump() {
                    Some(f) => pending = Some(f),
                    None => {
                        if !p.is_playing() {
                            cmds.playing.store(false, Ordering::SeqCst);
                            was_playing = false;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                }
            }
        } else {
            thread::sleep(Duration::from_millis(4));
        }
        cmds.position_us.store(p.position_us(), Ordering::SeqCst);
        cmds.state.store(state_index(p.state()), Ordering::SeqCst);
        *error.lock().unwrap() = p.error();
    }
    p.dispose();
}

// ── Composable 控件 ─────────────────────────────────────────────────────────

/// 视频画面。逐帧从控制器的帧槽读取 `image_id`，不触发整页重组。
pub fn Video(controller: VideoPlayerController, modifier: Modifier) {
    let slot = remember(|| Rc::new(RefCell::new(controller.clone())));
    *slot.borrow_mut() = controller.clone();
    let driver = remember(|| {
        let slot = slot.clone();
        add_frame_driver(move || {
            let c = slot.borrow().clone();
            let _ = c.on_frame();
        })
    });
    disposable_effect(0).on_dispose(move || remove_frame_driver(driver));

    let frame = controller.frame.clone();
    Canvas(
        modifier.background(Color::rgb(0.04, 0.05, 0.08), 4.0).clip(4.0),
        move |b: &mut DrawBuilder, x, y, w, h| {
            let slot = frame.get();
            if slot.id == 0 || slot.w == 0 || slot.h == 0 {
                return;
            }
            b.image(slot.id, slot.token, x, y, w, h, 1.0);
        },
    );
}

/// 可点击/拖动的进度条：点击或拖动按比例 seek。
fn VideoProgress(controller: VideoPlayerController, modifier: Modifier) {
    let id = add_node(Policy::Leaf, modifier);
    // 绘制闭包也会在 measure 期的 leaf intrinsic 预探测里被调用，此时 runtime 已被
    // 可变借用；不能走 MutableState::peek_mut（内部 with_rt 会二次 borrow_mut）。
    // 直接读控制器的原子位置/媒体信息，既无重入也不注册组合依赖。
    let draw_cmds = controller.cmds.clone();
    let draw_info = controller.info.clone();
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            let pos = draw_cmds.position_us.load(Ordering::Relaxed);
            let dur = draw_info.lock().unwrap().as_ref().map(|i| i.duration_us).unwrap_or(0);
            let r = h * 0.5;
            b.fill_rrect(x, y, w, h, r, Color::rgba(0.35, 0.38, 0.45, 1.0));
            let ratio = if dur > 0 {
                (pos as f64 / dur as f64).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            if ratio > 0.0 {
                b.fill_rrect(x, y, w * ratio, h, r, Color::rgb(0.26, 0.55, 0.92));
            }
        }));
    });
    let c_tap = controller.clone();
    register(
        id.index() as u32,
        Box::new(detect_tap_gestures(tap_config(
            move |gx, _| {
                let (nx, _, nw, _) = node_abs_rect(id);
                let ratio = ((gx - nx) / nw.max(1.0)).clamp(0.0, 1.0);
                let dur = c_tap.duration_us();
                if dur > 0 {
                    c_tap.seek_to((dur as f64 * ratio as f64).round() as i64);
                }
            },
            |_, _| {},
            |_, _| {},
        ))),
    );
    let node_key = id.index() as u64;
    disposable_effect(node_key).on_dispose(move || unregister(node_key as u32));
}

/// 传输控制条：播放/暂停、首尾、±5 秒、进度条、状态文本。
pub fn VideoControls(controller: VideoPlayerController, modifier: Modifier) {
    let v = controller.value();
    let c_toggle = controller.clone();
    let label = if v.is_playing { "暂停" } else { "播放" };
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let c = c_toggle.clone();
            TextButton(label, Modifier::new(), move || c.toggle_play());

            let c = controller.clone();
            TextButton("开头", Modifier::new(), move || c.seek_to(0));

            let c = controller.clone();
            TextButton("-5s", Modifier::new(), move || {
                c.seek_to(c.position_us() - 5_000_000);
            });

            let c = controller.clone();
            VideoProgress(c, Modifier::new().width(220.0).height(8.0));

            let c = controller.clone();
            TextButton("+5s", Modifier::new(), move || {
                c.seek_to(c.position_us() + 5_000_000);
            });

            let c = controller.clone();
            TextButton("结尾", Modifier::new(), move || {
                let d = c.duration_us();
                c.seek_to(d);
            });

            let c = controller.clone();
            let mute_label = if v.volume <= 0.0 { "取消静音" } else { "静音" };
            TextButton(mute_label, Modifier::new(), move || {
                let target = if c.value().volume > 0.0 { 0.0 } else { 1.0 };
                c.set_volume(target);
            });

            let c = controller.clone();
            TextButton("音量−", Modifier::new(), move || {
                let v = (c.value().volume - 0.1).max(0.0);
                c.set_volume(v);
            });

            let c = controller.clone();
            TextButton("音量+", Modifier::new(), move || {
                let v = (c.value().volume + 0.1).min(1.0);
                c.set_volume(v);
            });

            let secs = |us: i64| (us.max(0) as f64) / 1_000_000.0;
            Text(
                format!(
                    "{:.1}/{:.1}s{}",
                    secs(v.position_us),
                    secs(v.duration_us),
                    if v.error_description.is_some() { " [错误]" } else { "" }
                ),
                TextStyle::new(),
                Modifier::new(),
            );
        },
    );
}

/// 画面 + 控制条的组合播放器（控制条摆在画面**下方**，常显）。
pub fn VideoPlayer(controller: VideoPlayerController, modifier: Modifier) {
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Video(controller.clone(), Modifier::new().width(480.0).height(270.0));
            VideoControls(controller.clone(), Modifier::new().height(40.0));
        },
    );
}

// ── 画面底部叠放控制条（自动隐藏 / 可换自定义控件） ──────────────────────────

/// 控制条静止多久后自动隐藏（对齐 rui_extra `STAGE_HIDE_DELAY_S = 2.0`）。
const STAGE_HIDE_DELAY_SECS: f32 = 2.0;
/// 控制条淡入淡出时长（对齐 AnimatedOpacity 的常见 200ms）。
const STAGE_FADE_SECS: f32 = 0.2;

/// 自定义播放控件构造器：收到「当前是否显示」，在播放器作用域里 emit 任意控件树
/// （对齐 rui_extra::video::VideoControlsBuilder）。传 `None` = 用 [`VideoControlsOverlay`]。
pub type VideoControlsBuilder = Rc<dyn Fn(bool)>;

/// 内置控制条的**叠放**样式：半透明深底 + 暗色主题字形，压在任意画面上都读得清
/// （对齐 cui `VideoControls(overlay=true)`）。分工同 cui：本控件只管「长什么样 +
/// 点了做什么」，「贴底部 / 自动隐藏」是 [`VideoPlayerWidget`] 的摆放策略。
pub fn VideoControlsOverlay(controller: VideoPlayerController, modifier: Modifier) {
    let bar = modifier
        .background(Color::rgba(0.0, 0.0, 0.0, 0.55), 0.0)
        .padding_ltrb(8.0, 2.0, 8.0, 2.0);
    Box(bar, Alignment::CenterStart, move || {
        // 叠在画面上时不吃主题：固定走暗色调色板，浅色主题下字形也不会变黑糊进暗底。
        // 内层是 `Fn`（会被反复调用），故把控制器 move 进去后每轮再 clone 一份喂给 VideoControls。
        let ctl = controller.clone();
        MaterialTheme(ColorScheme::dark(), move || {
            VideoControls(ctl.clone(), Modifier::new().fill_max_width());
        });
    });
}

/// 画面 + **底部内置播放控件（自动隐藏）**的播放器，控制条可整体替换
/// （对齐 rui_extra::video::VideoPlayerWidget / cui `examples/video_demo`）。
///
/// - 舞台尺寸由调用方的 `modifier` 决定（例如 `fill_max_width().height(270.0)`），
///   画面铺满舞台，控制条贴在舞台**底边**（`Positioned(left:0,right:0,bottom:0)` 语义）；
///   舞台本身不改变节点数量 ⇒ 显隐只动透明度与命中，不会平移其它槽位；
/// - 点画面切换播放/暂停，并让控制条重新显形；
/// - 鼠标移入即显形；静止 [`STAGE_HIDE_DELAY_SECS`] 秒或移出画面自动隐藏
///   （隐藏 = `alpha(0)` + `no_hit()`，对齐 `IgnorePointer`，点击穿透到画面）；
/// - `auto_hide = false` ⇒ 控制条常显；未就绪（还没打开文件）时也常显，免得只剩黑屏；
/// - `controls` 传 `Some(builder)` 整体替换控制条，builder 收到「当前是否显示」。
pub fn VideoPlayerWidget(
    controller: VideoPlayerController,
    modifier: Modifier,
    auto_hide: bool,
    controls: Option<VideoControlsBuilder>,
) {
    let v = controller.value();
    let ready = v.is_initialized && v.error_description.is_none();

    // 折叠态是响应式的（决定 no_hit 与淡入淡出目标），故用 MutableState；
    // 「换源」判定与显形序号只是组合期记账，用 Cell 不触发额外重组。
    let collapsed = remember(|| mutableStateOf(false));
    let session = remember(|| Rc::new(Cell::new(controller.session_id())));
    if session.get() != controller.session_id() {
        session.set(controller.session_id());
        collapsed.set(false);
    }
    let collapsed_now = collapsed.get();
    let show = !ready || !auto_hide || !collapsed_now;

    // 进入显形态 ⇒ 序号 +1 ⇒ auto_hide_after 从此刻重新计时（Rust 参考里是重起 ticker）。
    let seq = remember(|| Rc::new(Cell::new(0u64)));
    let prev_show = remember(|| Rc::new(Cell::new(true)));
    if show != prev_show.get() {
        prev_show.set(show);
        if show {
            seq.set(seq.get().wrapping_add(1));
        }
    }
    if auto_hide {
        let hide = collapsed;
        super::feedback::auto_hide_after(
            seq.get(),
            STAGE_HIDE_DELAY_SECS,
            show,
            move || hide.set(true),
        );
    }

    let hover_seq = seq.clone();
    let hover_collapsed = collapsed;
    hoverable(
        modifier,
        move |entered| {
            if entered {
                hover_seq.set(hover_seq.get().wrapping_add(1));
                hover_collapsed.set(false);
            } else if auto_hide {
                // 移出画面立刻收起（Rust 参考的 stage_collapse），不等 2 秒。
                hover_collapsed.set(true);
            }
        },
        move || {
            // 底层：画面铺满舞台，点一下切换播放/暂停并重新显形（GestureDetector on_tap）。
            // 两个闭包（on_click / content）各持一份控制器句柄，不能共用同一个所有权。
            let tap = controller.clone();
            let surface_ctl = controller.clone();
            let tap_seq = seq.clone();
            let tap_collapsed = collapsed;
            Clickable(
                Modifier::new().fill_max_size(),
                move || {
                    tap.toggle_play();
                    tap_seq.set(tap_seq.get().wrapping_add(1));
                    tap_collapsed.set(false);
                },
                move || {
                    let c = surface_ctl.clone();
                    Video(c, Modifier::new().fill_max_size());
                },
            );

            // 上层：先铺满整块舞台再按 BottomStart 落位 ⇒ 控制条自然高、贴着底边，
            // 且淡出过程中不改变孩子布局（alpha/no_hit 只作用于这一层）。
            let t = animate_float_as_state(if show { 1.0 } else { 0.0 }, Tween::new(STAGE_FADE_SECS));
            let bar_mod = Modifier::new().fill_max_size().alpha(t);
            let bar_mod = if show { bar_mod } else { bar_mod.no_hit() };
            let builder = controls.clone();
            let bar_ctl = controller.clone();
            Box(bar_mod, Alignment::BottomStart, move || match &builder {
                Some(b) => b(show),
                None => {
                    let c = bar_ctl.clone();
                    VideoControlsOverlay(c, Modifier::new().fill_max_width());
                }
            });
        },
    );
}

/// 桌面文件选择 + 播放的便捷入口（回调在主线程执行）。
pub fn video_open_dialog(
    on_result: impl FnOnce(Option<VideoPlayerController>) + Send + 'static,
) {
    open_file_dialog(
        "打开视频",
        &[FileFilter::new("视频", &["mp4", "mkv", "avi", "mov", "webm", "gif", "png", "jpg"])],
        move |path| {
            let c = path.and_then(|p| {
                let s = p.to_string_lossy().to_string();
                VideoPlayerController::file(s)
            });
            on_result(c);
        },
    );
}

// ── Mock 后端（离线演示与回归测试） ─────────────────────────────────────────

pub struct MockVideoBackend {
    info: VideoInfo,
    frames: Vec<VideoFrame>,
    index: usize,
}
impl MockVideoBackend {
    pub fn new(width: u32, height: u32, duration_us: i64, frames: usize) -> Self {
        let n = frames.max(1);
        let frames = (0..frames)
            .map(|i| VideoFrame {
                rgba: vec![((i * 40) % 255) as u8; (width * height * 4) as usize],
                width,
                height,
                pts_us: i as i64 * duration_us / n as i64,
                seq: 0,
            })
            .collect();
        Self {
            info: VideoInfo {
                width,
                height,
                duration_us,
                kind: VideoMediaKind::VideoContainer,
                codec: "mock".into(),
                ..Default::default()
            },
            frames,
            index: 0,
        }
    }
}
impl VideoDecoderBackend for MockVideoBackend {
    fn kind(&self) -> VideoBackendKind {
        VideoBackendKind::System
    }
    fn info(&self) -> Option<VideoInfo> {
        Some(self.info.clone())
    }
    fn read_frame(&mut self) -> Option<VideoFrame> {
        let f = self.frames.get(self.index)?.clone();
        self.index += 1;
        Some(f)
    }
    fn seek_to(&mut self, pts_us: i64) {
        let step = self.info.duration_us.max(1) / self.frames.len().max(1) as i64;
        self.index = (pts_us / step.max(1)) as usize;
    }
    fn at_end(&self) -> bool {
        self.index >= self.frames.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_shape() {
        let c = video_backend_catalog();
        assert_eq!(c.catalog().iter().any(|e| e.kind == VideoBackendKind::Ffmpeg), cfg!(compose_ffmpeg));
        assert_eq!(ffmpeg_compiled(), cfg!(compose_ffmpeg));
    }

    #[test]
    fn playback_state_machine() {
        let mut p = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(16, 16, 1_000_000, 10)));
        assert!(p.initialize());
        assert_eq!(p.state(), PlaybackState::Initialized);
        p.play();
        assert_eq!(p.state(), PlaybackState::Playing);
        assert!(p.pump().unwrap().valid());
        p.pause();
        assert_eq!(p.state(), PlaybackState::Paused);
    }

    #[test]
    fn seek_clamp_and_replay() {
        let mut p = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(16, 16, 1_000_000, 10)));
        p.initialize();
        p.seek_to(5_000_000);
        assert_eq!(p.position_us(), 1_000_000);
        assert!(p.is_completed());
        p.play();
        assert_eq!(p.position_us(), 0);
        assert!(!p.is_completed());
        assert!(p.is_playing());
    }

    #[test]
    fn speed_and_volume_bounds() {
        let mut p = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(16, 16, 1_000_000, 10)));
        p.initialize();
        assert!(!p.set_playback_speed(0.0));
        assert!(p.set_playback_speed(2.0));
        p.set_volume(3.0);
        assert_eq!(p.volume(), 1.0);
        p.set_volume(-1.0);
        assert_eq!(p.volume(), 0.0);
    }

    #[test]
    fn player_value_defaults_and_aspect() {
        let v = VideoPlayerValue::default();
        assert!((v.aspect_ratio() - 1.0).abs() < 1e-9);
        let v = VideoPlayerValue {
            is_initialized: true,
            width: 160,
            height: 90,
            ..Default::default()
        };
        assert!((v.aspect_ratio() - 16.0 / 9.0).abs() < 1e-9);
    }

    #[test]
    fn controller_value_and_session() {
        let c = VideoPlayerController::with_backend(Box::new(MockVideoBackend::new(16, 16, 2_000_000, 20)));
        let c2 = c.clone();
        assert!(c.is_same_session(&c2));
        c.initialize();
        c.play();
        c.pause();
        assert!(!c.value().is_playing);
        c.set_volume(0.25);
        assert!((c.value().volume - 0.25).abs() < 1e-9);
        assert!(!c.set_playback_speed(0.0));
        c.seek_to(500_000);
        assert_eq!(c.value().position_us, 500_000);
    }
}
