//! 平台无关帧内核 — winit 无关的组合/布局/绘制/事件派发公共部分。
//!
//! 抽取自 compose-desktop（内核定位 = 应用宿主的核心事件/渲染循环）：
//! 所有平台后端（desktop/web/android/ios/ohos）共享同一套
//! 「组合 → advance_frame → 布局 → draw_tree → DisplayList → GPU」管线，
//! 平台 crate 只负责各自的事件泵与 Surface 对接：
//!
//! | 平台 | 事件泵 | Surface 入口（GpuRenderer） |
//! |---|---|---|
//! | desktop/web/android | winit（`run_app` / `spawn_app` / `with_android_app`） | `create_surface_from_window` / `create_surface_from_native_window` |
//! | ios | 宿主驱动（UIKit CADisplayLink 调 `Kernel::frame`） | `create_surface_from_metal_layer` |
//! | ohos | 宿主驱动（ArkTS/NAPI 调 `Kernel::frame`） | `create_surface_from_ohos_window` |

use std::time::Instant;

use compose::foundation::Constraints;

use compose::layout::{
    create_root_node, dispatch_char, dispatch_hover_at, dispatch_hover_left, dispatch_key,
    dispatch_press_down, dispatch_press_up, dispatch_pull_down, dispatch_scroll,
    dispatch_scrollbar_down, dispatch_secondary_click_at, drag_2d_end_global,
    drag_2d_move_global, drag_end_global,
    drag_move_global, draw_tree_tagged, hit_test,
    perform_layout, pull_end_global, pull_move_global, scrollbar_end_global,
    scrollbar_move_global, set_last_pointer_global, set_text_measurer, set_viewport, Alignment,
    heuristic_text_at, TextMeasurer, TextStyle,
};
use compose::material::ColorScheme;
use compose::runtime::{advance_frame, Composition, Composer, NavKey, Recomposer};

use crate::render::display_list::{
    build_cmds_display_list, build_display_list, new_frame_builder, record_cmds, TextContext,
};
use crate::render::gpu::GpuRenderer;

/// 用户应用 trait — 对齐 Compose `setContent { content }`。
/// 从 compose-desktop 上移至此，供全部平台 crate 共享。
pub trait App {
    /// 组合内容（@Composable 入口）。
    fn content(&self);
    /// 主题（默认 Material 3 Light）。
    fn theme(&self) -> ColorScheme {
        ColorScheme::light()
    }
}

thread_local! {
    /// 段落缓存 + 字体集合（平台在启动时经 `init_text_context` 注入字体）。
    pub(crate) static TEXT_CTX: std::cell::RefCell<Option<TextContext>> =
        const { std::cell::RefCell::new(None) };
}

/// 保留模式 GPU 渲染开关（默认开启）。
///
/// 静态场景渲到离屏层缓存，每帧只把该层作为一次全屏 quad blit 进呈现目标，再叠加动态段，
/// 动画期低 CPU（story 动画页实测 5.81 → 2.58 ms/帧，34.8% → 15.5%）。
/// 设 `COMPOSE_RETAINED_GPU=0` 可退回全量重绘，用于对照测量。
fn retained_enabled() -> bool {
    use std::sync::OnceLock;
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("COMPOSE_RETAINED_GPU")
            .map(|v| !matches!(v.as_str(), "0" | "off" | "false" | "no"))
            .unwrap_or(true)
    })
}

/// 静态块是否交给 valo 的栅格缓存（`draw_display_list_cached`）：命中后整块变成一次贴图，
/// 规划期不再逐命令走它。默认关闭，`COMPOSE_RASTER_CACHE=1` 打开做 A/B。
fn raster_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("COMPOSE_RASTER_CACHE")
            .map(|v| !matches!(v.as_str(), "0" | "off" | "false" | "no"))
            .unwrap_or(false)
    })
}

/// 帧管线追踪开关（`COMPOSE_TRACE_FRAMES=1`）：每 60 帧向 stderr 打印帧率、命中的快路径
/// 与各阶段均耗时，用于动画/重绘性能定位；关闭时零输出（每帧仅多几次 `Instant::now()`）。
fn trace_on() -> bool {
    use std::sync::OnceLock;
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("COMPOSE_TRACE_FRAMES")
            .map(|v| matches!(v.as_str(), "1" | "on" | "true" | "yes"))
            .unwrap_or(false)
    })
}

/// 一帧各阶段的耗时与规模采样（`COMPOSE_TRACE_FRAMES=1` 时按 60 帧窗口汇报）。
struct FrameSample {
    /// 命中的快路径：0=未遍历（画面 settled），1=遍历但无改变，2=真实脏区重建。
    path: u8,
    adv_ms: f32,
    lay_ms: f32,
    draw_ms: f32,
    build_ms: f32,
    gpu_ms: f32,
    cmds: usize,
    /// 保留模式下动态层命令数（关闭保留模式时等于全量）。
    dyn_cmds: usize,
}

/// 累计一帧的分阶段耗时并按窗口汇报。`gpu` 含呈现阻塞（等 vsync），故 `work` 只是
/// 「非阻塞阶段」占空比，不等于进程 CPU —— 真实 CPU 以 /proc 计。
fn trace_frame(s: FrameSample) {
    if !trace_on() {
        return;
    }
    struct Acc {
        n: u32,
        p: [u32; 3],
        sum: [f32; 5],
        cmds: usize,
        dyn_cmds: usize,
        measured: u32,
        placed: u32,
        clock: Instant,
    }
    let new_acc = |clock: Instant| Acc {
        n: 0,
        p: [0; 3],
        sum: [0.0; 5],
        cmds: 0,
        dyn_cmds: 0,
        measured: 0,
        placed: 0,
        clock,
    };
    thread_local! {
        static ACC: std::cell::RefCell<Option<Acc>> = const { std::cell::RefCell::new(None) };
    }
    ACC.with(|cell| {
        let mut b = cell.borrow_mut();
        let acc = b.get_or_insert_with(|| new_acc(Instant::now()));
        acc.n += 1;
        acc.p[s.path as usize] += 1;
        acc.sum[0] += s.adv_ms;
        acc.sum[1] += s.lay_ms;
        acc.sum[2] += s.draw_ms;
        acc.sum[3] += s.build_ms;
        acc.sum[4] += s.gpu_ms;
        acc.cmds = acc.cmds.max(s.cmds);
        acc.dyn_cmds = acc.dyn_cmds.max(s.dyn_cmds);
        // 本轮布局实际测量/落位的节点数：接近 arena 规模即「增量布局失效」的信号。
        let (m, p) = compose::layout::layout_node_counts();
        acc.measured = acc.measured.max(m);
        acc.placed = acc.placed.max(p);
        if acc.n < (if trace_small() {3} else {60}) {
            return;
        }
        // arena 规模：布局尾部的全表扫描（滚动结算）成本随它线性增长。
        let arena = compose::runtime::with_rt_pub(|rt| rt.nodes.len());
        let anims = compose::animation::running_animations();
        let drivers = compose::runtime::frame_driver_count();
        let secs = acc.clock.elapsed().as_secs_f32().max(1e-6);
        let n = acc.n as f32;
        let frame_ms = acc.sum.iter().sum::<f32>() / n;
        eprintln!(
            "[trace] {:5.1} fps | path fast1={} fast2={} full={} | ms/frame adv={:.2} lay={:.2} draw={:.2} build={:.2} gpu={:.2} total={:.2} | cmds<={} dyn<={} | lay nodes meas<={} place<={}/{} | anim={} drv={} | work={:.1}%",
            n / secs,
            acc.p[0],
            acc.p[1],
            acc.p[2],
            acc.sum[0] / n,
            acc.sum[1] / n,
            acc.sum[2] / n,
            acc.sum[3] / n,
            acc.sum[4] / n,
            frame_ms,
            acc.cmds,
            acc.dyn_cmds,
            acc.measured,
            acc.placed,
            arena,
            anims,
            drivers,
            frame_ms * (n / secs) / 10.0,
        );
        *acc = new_acc(Instant::now());
    });
}

/// valo 文本度量器（布局阶段注入 core；真实字体度量，无字体时启发式兜底）。
pub struct ValoTextMeasurer;

impl TextMeasurer for ValoTextMeasurer {
    fn measure(&self, Text: &str, style: &TextStyle) -> (f32, f32) {
        self.measure_at(Text, style, f32::INFINITY)
    }

    fn measure_at(&self, Text: &str, style: &TextStyle, max_w: f32) -> (f32, f32) {
        TEXT_CTX.with(|cell| {
            let mut b = cell.borrow_mut();
            match b.as_mut() {
                Some(tc) => tc.measure_at(Text, style, max_w),
                None => heuristic_text_at(Text, style, max_w),
            }
            
        })
    }
}

/// 注入字体集合（平台启动时调用一次；`FontCollection::default()` = 启发式度量）。
pub fn init_text_context(fc: valo::FontCollection) {
    TEXT_CTX.with(|cell| {
        *cell.borrow_mut() = Some(TextContext::new(fc));
    });
}

/// 平台无关应用内核：持有组合根 + 主题，驱动帧管线与事件派发。
pub struct Kernel {
    pub root: compose::layout::NodeId,
    pub theme: ColorScheme,
    last_frame: Instant,
    /// 是否已成功渲染过至少一帧。首帧必渲染；此后由增量脏区判据（`scene_is_clean` /
    /// `scene_changed`）决定能否跳过绘制遍历与 GPU 上传，替代旧「整段 Vec<DrawCmd> 深比较」。
    has_rendered: bool,
    /// 保留模式的跨帧分区状态（上一帧段指纹 + 静态块签名 + 已缓存的块键）。
    retained: compose::layout::RetainedState,
    /// 本帧渲染计划（段句柄 + 现场命令）：跨帧复用缓冲容量，动画期逐帧重建时零分配。
    frame_draw: compose::layout::FrameDraw,
    /// 上一帧主题底色。底色不在命令流里，变化时必须强制重录静态块。
    last_bg: Option<compose::foundation::Color>,
    /// 静态块键 → 已录好的 DisplayList（与 `retained.cached` 同步，只保留本帧用到的块）。
    cached_runs: std::collections::HashMap<u64, std::sync::Arc<valo::DisplayList>>,
    /// 本帧用到的静态块键（逐帧复用：动画期每帧都要收集一遍，不值得重新分配）。
    used_chunks: std::collections::HashSet<u64>,
    last_w: f32,
    last_h: f32,
}

impl Kernel {
    /// 组合应用内容（等价 winit 后端 `resumed` 里的组合段）。
    /// 注意：不额外包 `MaterialTheme`（App::content 自行提供主题，主题切换才能重提供）。
    pub fn new(theme: ColorScheme, content: impl Fn() + 'static) -> Self {
        let root = create_root_node(Alignment::TopStart);
        // 文本度量器是线程局部、无状态的：注入一次即可（旧实现每帧 `Box::new` 一次
        // 分配，空闲帧也逃不掉）。
        set_text_measurer(Box::new(ValoTextMeasurer));
        let composition = Composition::new(root);
        composition.set_content(content);
        Self { root, theme, last_frame: Instant::now(), has_rendered: false, retained: Default::default(), frame_draw: Default::default(), last_bg: None, cached_runs: Default::default(), used_chunks: Default::default(), last_w: 0.0, last_h: 0.0 }
    }

    /// 渲染一帧：advance_frame → 重组 → 布局 → DisplayList → GPU。
    /// 返回 true 表示仍有动画/待重组（宿主应尽快再来一帧）。
    pub fn frame(&mut self, renderer: &mut GpuRenderer, width: f32, height: f32) -> bool {
        let delta = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();

        // 帧推进（重组 + 动画帧驱动 + 协程）
        // 视口先注入：浮层（Dialog / Snackbar）在组合期就要读它做居中定位
        set_viewport(width, height);
        let t0 = Instant::now();
        advance_frame(delta);
        Recomposer::recompose_pending();
        let t1 = Instant::now();

        // 3) 布局（物理像素坐标）；perform_layout 内含「已稳定则跳过整树」增量短路。
        let root = self.root;
        let _ = perform_layout(root, Constraints::new(0.0, width, 0.0, height));
        let t2 = Instant::now();

        let size_unchanged = self.last_w == width && self.last_h == height;
        let ms = |a: Instant, b: Instant| b.duration_since(a).as_secs_f32() * 1000.0;

        // 4) 增量脏区绘制。
        // 快路径①：画面已 settled（根无失效标记 = 本轮无重组 / 几何 / 滚动脏）且尺寸未变 →
        // 与上一帧逐命令相同 → 跳过整树绘制遍历、DisplayList 重建与 GPU 上传（空闲帧近乎零成本）。
        if self.has_rendered && size_unchanged && compose::layout::scene_is_clean(root) {
            let busy = compose::animation::animations_active() || Recomposer::has_pending();
            trace_frame(FrameSample {
                path: 0,
                adv_ms: ms(t0, t1),
                lay_ms: ms(t1, t2),
                draw_ms: 0.0,
                build_ms: 0.0,
                gpu_ms: 0.0,
                cmds: 0,
                dyn_cmds: 0,
            });
            return busy;
        }

        // 遍历绘制树。保留模式只物化内容变化的段（静态段留在节点图层缓存里，见 `FrameDraw`）；
        // 非保留模式把整条命令流按「本帧要画」物化，行为与旧实现一致。
        let bg = self.theme.background;
        // 缓存缺失（首帧）或底色变化 = 键覆盖不到的外部失效，强制重录静态块。
        let retained = retained_enabled();
        let force_static = !self.has_rendered || self.last_bg != Some(bg);
        self.last_bg = Some(bg);
        // 取出并复用上一帧的计划缓冲：动画期每帧都会重建计划，`Vec`/`HashSet` 的
        // 分配与重哈希不该逐帧发生（用完在两条返回路径上放回）。
        let mut frame_draw = std::mem::take(&mut self.frame_draw);
        if retained {
            compose::layout::draw_tree_retained_into(
                root,
                &mut self.retained,
                &mut frame_draw,
                force_static,
            );
        } else {
            let (cmds, _owners) = draw_tree_tagged(root);
            let n = cmds.len();
            frame_draw.clear();
            frame_draw.dynamic_cmds = cmds;
            frame_draw.cmd_count = n;
        }
        let t3 = Instant::now();
        // 快路径②：本轮遍历未发现任何节点输出改变（重组产出等价画面 / 光标处于同一相位）→
        // 跳过 DisplayList 重建与 GPU 上传。以 O(1) 标志取代旧 O(M) 整段 Vec 深比较。
        //
        // `static_changed` 一并判：段「由动态转静态」的那一帧各节点图层内容并未改变
        // （`scene_changed` 为假），但静态块**集合**变了。若在此跳过，`RetainedState::sig`
        // 已推进而缓存的块还停在旧计划上，下一帧就再也判不出需要重录。
        if self.has_rendered
            && size_unchanged
            && !compose::layout::scene_changed()
            && !frame_draw.static_changed
        {
            self.last_w = width;
            self.last_h = height;
            let busy = compose::animation::animations_active() || Recomposer::has_pending();
            trace_frame(FrameSample {
                path: 1,
                adv_ms: ms(t0, t1),
                lay_ms: ms(t1, t2),
                draw_ms: ms(t2, t3),
                build_ms: 0.0,
                gpu_ms: 0.0,
                cmds: frame_draw.cmd_count,
                dyn_cmds: 0,
            });
            self.frame_draw = frame_draw;
            return busy;
        }

        // —— 有真实脏区：重建并上传 ——
        let mut build_ms = 0.0f32;
        let mut gpu_ms = 0.0f32;
        let dyn_count = frame_draw.dynamic_cmds.len();
        TEXT_CTX.with(|cell| {
            let mut b = cell.borrow_mut();
            let Some(tc) = b.as_mut() else { return };
            let tb = Instant::now();
            let dl = if retained {
                self.build_chunked_frame(&frame_draw, bg, tc)
            } else {
                std::sync::Arc::new(build_display_list(&frame_draw.dynamic_cmds, bg, tc))
            };
            let tr = Instant::now();
            renderer.render_frame(dl, bg);
            let te = Instant::now();
            build_ms = ms(tb, tr);
            gpu_ms = ms(tr, te);
        });
        self.last_w = width;
        self.last_h = height;
        self.has_rendered = true;

        // 5) 帧调度门控：动画进行中/有待重组 → 连续帧；否则宿主可休眠
        let busy = compose::animation::animations_active() || Recomposer::has_pending();
        trace_frame(FrameSample {
            path: 2,
            adv_ms: ms(t0, t1),
            lay_ms: ms(t1, t2),
            draw_ms: ms(t2, t3),
            build_ms,
            gpu_ms,
            cmds: frame_draw.cmd_count,
            dyn_cmds: dyn_count,
        });
        self.frame_draw = frame_draw;
        busy
    }

    /// 保留模式拼帧：静态块按内容键复用已录好的 DisplayList（嵌进本帧只是一条 op），
    /// 现场块直接录进同一条流。块序 = 绘制树序 ⇒ z 序与平铺路径逐命令一致。
    ///
    /// 旧写法把「全部静态」渲进离屏层再整屏贴图、动态段画在其上，省下的是 CPU，
    /// 代价是树中部动画把后续静态内容（表格文字等）盖掉——本函数把分层换回精确。
    fn build_chunked_frame(
        &mut self,
        f: &compose::layout::FrameDraw,
        bg: compose::foundation::Color,
        tc: &mut TextContext,
    ) -> std::sync::Arc<valo::DisplayList> {
        use compose::layout::Chunk;
        let mut b = new_frame_builder(Some(bg));
        let mut used = std::mem::take(&mut self.used_chunks);
        used.clear();
        for chunk in &f.plan {
            match *chunk {
                Chunk::Static { key, from, to } => {
                    let dl = match self.cached_runs.get(&key).cloned() {
                        Some(dl) => dl,
                        None => {
                            let cmds = f.static_chunk_cmds(from, to);
                            let dl = std::sync::Arc::new(build_cmds_display_list(&cmds, tc));
                            self.cached_runs.insert(key, dl.clone());
                            dl
                        }
                    };
                    used.insert(key);
                    if raster_cache_enabled() {
                        b.draw_display_list_cached(&dl);
                    } else {
                        b.draw_display_list(&dl);
                    }
                }
                Chunk::Live { start, len } => {
                    record_cmds(&mut b, &f.dynamic_cmds[start..start + len], tc)
                }
            }
        }
        // 块缓存只留本帧用到的键：换页 / 改尺寸后的旧块不会一直占着显存与堆。
        self.cached_runs.retain(|k, _| used.contains(k));
        self.used_chunks = used;
        std::sync::Arc::new(b.build())
    }

    /// 指针按下（宿主触摸/鼠标 down）。
    pub fn pointer_down(&self, x: f32, y: f32) -> bool {
        set_last_pointer_global(x, y);
        let Some((target, _, _)) = hit_test(self.root, x, y) else { return false };
        dispatch_hover_at(target);
        // 滚动条拖拽优先（桌面，命中右侧/底部滚动条区域则接管，不再透传点击）
        if dispatch_scrollbar_down(target, x, y) {
            return true;
        }
        // 下拉刷新：命中链上有已滚到顶的下拉容器时开启手势会话
        dispatch_pull_down(target, y);
        // 按下只做「按压指示 + on_press 手势」；点击在抬起时结算（对齐 Flutter onTap）。
        dispatch_press_down(target, x, y)
    }

    /// 次要按钮（鼠标右键）按下：在命中链上派发 `on_secondary_click`（上下文菜单）。
    ///
    /// 只处理**按下**、不处理抬起：右键弹出菜单后全屏遮罩会盖住指针，若抬起也派发
    /// 会立刻把刚弹出的菜单关掉。右键也不进入 `dispatch_press_down` 的按压/波纹链路，
    /// 避免产生左键式 ripple 与被 `dispatch_press_up` 结算成普通点击。
    pub fn pointer_secondary_click(&self, x: f32, y: f32) -> bool {
        set_last_pointer_global(x, y);
        let Some((target, lx, ly)) = hit_test(self.root, x, y) else {
            return false;
        };
        dispatch_secondary_click_at(target, lx, ly)
    }

    /// 指针移动（宿主 touch move / mouse move；优先驱动拖拽，其次下拉手势，否则悬停态）。
    pub fn pointer_move(&self, x: f32, y: f32) -> bool {
        set_last_pointer_global(x, y);
        if drag_move_global(x, y) {
            return true;
        }
        if drag_2d_move_global(x, y) {
            return true;
        }
        if pull_move_global(y) {
            return true;
        }
        if scrollbar_move_global(x, y) {
            return true;
        }
        match hit_test(self.root, x, y) {
            Some((target, _, _)) => dispatch_hover_at(target),
            None => dispatch_hover_left(),
        }
    }

    /// 指针抬起（触摸 up / 鼠标释放；结束拖拽/下拉手势并结算按压点击）。
    pub fn pointer_up(&self, x: f32, y: f32) -> bool {
        set_last_pointer_global(x, y);
        // 一次只可能有一个手势会话，三个 `*_end` 与旧实现同样首个真即结束。
        let gesture = drag_end_global() || drag_2d_end_global() || pull_end_global()
            || scrollbar_end_global();
        // 抬起结算点击：位移在 tap slop 内且抬起链上的可点击节点与按下时同一个才触发。
        // 拖拽/下拉期间指针必然越出 slop ⇒ 这里自然取消，不会误触。
        let hit = hit_test(self.root, x, y);
        let clicked = dispatch_press_up(hit, x, y);
        if gesture || clicked {
            return true;
        }
        match hit {
            Some((target, _, _)) => dispatch_hover_at(target),
            None => dispatch_hover_left(),
        }
    }

    /// 滚轮/滚动手势（dy > 0 = 向下）。
    pub fn scroll_at(&self, x: f32, y: f32, dy: f32) -> bool {
        let target = hit_test(self.root, x, y).map(|(t, _, _)| t);
        dispatch_scroll(target, dy)
    }

    /// 键盘导航键（方向键/PageUp/PageDown 由平台翻译为 NavKey）。
    pub fn key(&self, key: NavKey) -> bool {
        if dispatch_key(key) {
            return true;
        }
        crate::input_router::key(key);
        true
    }

    /// 字符输入（路由：TextField 焦点优先 → 焦点作用域 type-ahead）。
    pub fn char(&self, ch: char) -> bool {
        if !crate::input_router::focused() && dispatch_char(ch) {
            return true;
        }
        crate::input_router::char(ch);
        true
    }

    /// 退格（路由到聚焦 TextField）。
    pub fn backspace(&self) {
        crate::input_router::backspace();
    }
}

/// 临时诊断：把 60 帧汇报窗口压到 3 帧（`COMPOSE_TRACE_SMALL=1`）。
fn trace_small() -> bool {
    std::env::var("COMPOSE_TRACE_SMALL").is_ok()
}
