//! 位图 `Image` 控件：内置字节来源 + 后台线程异步解码 + GPU 纹理上传（由渲染层完成）。
//!
//! 控件只负责：分配稳定 `image_id`、在后台线程把字节解码为 RGBA8 并提交到全局图像注册表；
//! 渲染层（`compose::render::image_cache`）负责把像素上传为 `valo::Image` 并绘制。解码完成
//! 通过「修订号 token」翻转触发重绘（见 `DrawCmd::Image`），从而让保留模式静态块缓存失效并重绘。
//!
//! 当 `source` 变化（例如用户用文件对话框改选图片）时，会清掉旧纹理并自增修订号，强制重新走
//! 「占位 → 真图」；新旧图片使用同一 `image_id`，但 GPU 纹理在注册表里被替换。

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use compose::layout::{add_node, DrawBuilder, Modifier, Policy};
use compose::render::image_cache;
use compose::runtime::{mutableStateOf, remember, with_rt_pub};

/// 图片字节来源。`Image` 控件据此在后台线程异步解码为位图。
#[derive(Clone, PartialEq)]
pub enum ImageSource {
    /// 内存字节（PNG / JPEG / GIF / BMP 等，由 `image` crate 自动识别格式）。
    Bytes(Vec<u8>),
    /// 本地文件（运行时读取后同上解码）。
    File(PathBuf),
}

/// `Image(modifier, source)` — 异步解码并绘制位图；解码完成前显示占位，完成后整图绘制。
///
/// 内部用稳定 `image_id` 关联 GPU 纹理（按 id 跨帧复用，不重复上传）。
pub fn Image(modifier: Modifier, source: ImageSource) {
    // 稳定 id：每次挂载分配一次。
    let id = remember(|| image_cache::next_image_id());
    // 已提交解码的 source 快照：用于检测 source 变化后重新解码。
    let submitted = remember(|| mutableStateOf(None::<ImageSource>));
    // 解码线程只启动一次（每个 source 版本一次）。
    let started = remember(|| Rc::new(Cell::new(false)));
    // 修订号：source 变化或解码完成都会自增 → DrawCmd 内容指纹变化 → 保留模式缓存失效。
    let rev = remember(|| mutableStateOf(0u64));
    // 记录「已为哪个修订号完成解码」，避免每帧重复自增修订号导致无限重绘。
    let decoded_for = remember(|| mutableStateOf(None::<u64>));

    // source 发生变化（或首次）：清掉旧纹理，重置解码状态，准备重新解码。
    if submitted.read_clone() != Some(source.clone()) {
        submitted.set(Some(source.clone()));
        image_cache::drop_image(id); // 清 GPU 纹理 + 待上传像素，使其重新走占位 → 真图
        rev.set(rev.get() + 1);
        decoded_for.set(None);
        started.set(false);
    }

    if !started.get() {
        started.set(true);
        let src = source;
        let img_id = id;
        // 后台线程解码，避免阻塞 UI 组合 / 布局。
        std::thread::spawn(move || {
            let bytes: Vec<u8> = match &src {
                ImageSource::Bytes(b) => b.clone(),
                ImageSource::File(p) => match std::fs::read(p) {
                    Ok(d) => d,
                    Err(_) => return,
                },
            };
            let rgba = match image::load_from_memory(&bytes).map(|img| img.to_rgba8()) {
                Ok(r) => r,
                Err(_) => return,
            };
            let (w, h) = (rgba.width(), rgba.height());
            image_cache::submit_decoded(img_id, rgba.into_raw(), w, h);
        });
    }

    // 解码完成（注册表已有该 id 的像素）且当前修订号尚未标记完成 → 推进修订号触发重绘。
    if image_cache::has(id) && decoded_for.get() != Some(rev.get()) {
        decoded_for.set(Some(rev.get() + 1));
        rev.set(rev.get() + 1);
    }

    let t = rev.get();
    let iid = id;
    let node = add_node(Policy::Leaf, modifier);
    with_rt_pub(|rt| {
        rt.nodes[node.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            // 始终 emit `DrawCmd::Image`：token(修订号) 进 hash，渲染层按 id 查纹理（未上传则占位）。
            b.image(iid, t, x, y, w, h, 1.0);
        }));
    });
}
