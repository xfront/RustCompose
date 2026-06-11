//! 位图渲染的全局图像注册表与上传缓存。
//!
//! 解码在后台线程（由 `Image` 控件驱动）完成，RGBA8 像素先投递进线程安全的
//! `INBOX`；主线程每帧在 `Kernel::frame` 开头 `consume_decoded()` 把像素取回本线程
//! 注册表并触发一次重组（让 `Image` 控件的 `token` 翻转 → `DrawCmd` hash 变 →
//! 保留模式静态块缓存失效 → 重绘），随后 `upload_phase()` 把 CPU 像素上传为
//! `valo::Image`（GPU 纹理）并按 `image_id` 缓存；`record_cmds` 通过 `get_uploaded`
//! 取得句柄调用 `DisplayListBuilder::draw_image`。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use valo::Image;

use crate::render::gpu::GpuRenderer;

/// 一份解码后的位图（RGBA8，行优先，每行 `4 * w` 字节）。
struct Decoded {
    pixels: Vec<u8>,
    w: u32,
    h: u32,
}

/// 解码线程 → 主线程的投递箱（线程安全）。
static INBOX: Mutex<Vec<(u64, Decoded)>> = Mutex::new(Vec::new());
/// 是否有新像素待取回（避免每帧锁箱）。
static INBOX_READY: AtomicBool = AtomicBool::new(false);

/// 主线程 / 渲染线程本地状态：可持有 GPU 资源（`valo::Image` 含 `wgpu::Texture`）。
struct Registry {
    /// 待上传的 CPU 像素（按 `image_id`）。
    pending: HashMap<u64, Decoded>,
    /// 已上传、可复用的 GPU 纹理（按 `image_id` 寻址）。
    uploaded: HashMap<u64, Image>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry {
        pending: HashMap::new(),
        uploaded: HashMap::new(),
    });
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// 分配进程内唯一且稳定的图像 id（控件挂载时调用一次）。
pub fn next_image_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// 解码线程调用（线程安全）：把解码后的 RGBA8 像素交给主线程。
pub fn submit_decoded(id: u64, pixels: Vec<u8>, w: u32, h: u32) {
    if let Ok(mut boxed) = INBOX.lock() {
        boxed.push((id, Decoded { pixels, w, h }));
        INBOX_READY.store(true, Ordering::SeqCst);
    }
}

/// 主线程每帧开头调用：把 `INBOX` 像素转入 `pending`，返回是否有新像素到达。
pub fn consume_decoded() -> bool {
    if !INBOX_READY.swap(false, Ordering::SeqCst) {
        return false;
    }
    let items = match INBOX.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(_) => return false,
    };
    if items.is_empty() {
        return false;
    }
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        for (id, d) in items {
            reg.pending.insert(id, d);
        }
    });
    true
}

/// 渲染线程调用：把 `pending` 中尚未上传的像素上传为 `valo::Image`，缓存到 `uploaded`。
pub fn upload_phase(renderer: &mut GpuRenderer) {
    let ids: Vec<u64> = REG.with(|r| r.borrow().pending.keys().cloned().collect());
    for id in ids {
        // 取出 owned decoded，避免上传期间长期借住 REG。
        let decoded = REG.with(|r| r.borrow_mut().pending.remove(&id));
        let Some(d) = decoded else { continue };
        let already = REG.with(|r| r.borrow().uploaded.contains_key(&id));
        if already {
            continue;
        }
        let img = renderer.upload_image(id, &d.pixels, d.w, d.h);
        REG.with(|r| {
            r.borrow_mut().uploaded.insert(id, img);
        });
    }
}

/// 取已上传的纹理句柄（供 `record_cmds` 绘制）。`valo::Image` 是 cheap clone 句柄。
pub fn get_uploaded(id: u64) -> Option<Image> {
    REG.with(|r| r.borrow().uploaded.get(&id).cloned())
}

/// 是否已上传（供控件 draw 闭包决定是否 emit `DrawCmd::Image`）。
pub fn is_uploaded(id: u64) -> bool {
    REG.with(|r| r.borrow().uploaded.contains_key(&id))
}

/// 控件销毁时释放 GPU 纹理与待上传像素。
pub fn drop_image(id: u64) {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        reg.pending.remove(&id);
        reg.uploaded.remove(&id);
    });
}

/// 是否还有待上传的像素（供 `Kernel::frame` 判定本帧必须重绘，不可走快路径跳过）。
pub fn pending_nonempty() -> bool {
    REG.with(|r| !r.borrow().pending.is_empty())
}

/// 某 `image_id` 是否已有解码像素（pending 或已上传任一），供控件组合期推进 token。
pub fn has(id: u64) -> bool {
    REG.with(|r| {
        let g = r.borrow();
        g.pending.contains_key(&id) || g.uploaded.contains_key(&id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_then_consume_moves_to_pending() {
        let id = next_image_id();
        assert!(!has(id));
        // 模拟解码线程投递 RGBA8 像素（1×1）。
        submit_decoded(id, vec![10, 20, 30, 255], 1, 1);
        // 主线程帧开头取回，应进入 pending 且 has 为真。
        assert!(consume_decoded());
        assert!(has(id));
        assert!(pending_nonempty());
        // 再次取回应无新数据。
        assert!(!consume_decoded());
    }
}
