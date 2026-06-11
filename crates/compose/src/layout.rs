//! Compose UI — 节点 arena、Modifier 值链、测量/布局/绘制三阶段、命中测试。
//!
//! 对齐 `androidx.compose.ui`：
//! - `Constraints` 单趟测量（每个孩子只测一次，weight 孩子最后测）
//! - `MeasurePolicy`：父定约束、子测尺寸、父落位
//! - 绘制产出平台无关 `DrawCmd` 序列（valo 转换层在 compose-desktop）

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

pub(crate) use crate::foundation::{Color, Constraints, Size};
use crate::runtime::{MutableState, RuntimeData as Rt, ScopeId};

// ══════════════════════════════ Node ══════════════════════════════

/// 节点句柄（arena 索引）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

pub type NodeRef = Node;

/// 节点数据（存于中央 arena，禁止跨线程）。
pub struct Node {
    /// 槽位是否存活（`add_node` 复用槽位时重置；`free_subtree` 置否，兜底防重复释放）。
    pub alive: bool,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub policy: Policy,
    pub modifier: Modifier,
    /// 自定义绘制内容（text / Canvas / TextField 显示层）。
    pub draw: Option<Box<dyn Fn(&mut DrawBuilder, f32, f32, f32, f32)>>,
    pub owner_scope: Option<ScopeId>,
    /// 点击处理（take-put 调用防重入）。
    pub on_click: Option<Box<dyn FnMut()>>,
    /// 次要按钮（鼠标右键）点击处理（take-put 调用防重入）。
    /// 沿祖先链找最近的处理器，**不**回落到 `on_click`：桌面右键不应触发左键行为。
    pub on_secondary_click: Option<Box<dyn FnMut()>>,
    /// 按下处理（take-put 调用防重入）：**指针按下即触发**，用于需要在抬起前就开始的手势
    /// （分栏拖拽、滑杆按住拖动）。Flutter 的 `onTap` 在抬起时触发，故这类控件必须走这里。
    pub on_press: Option<Box<dyn FnMut()>>,
    /// 键盘导航处理（焦点作用域收到方向键时调用；take-put 防重入）。
    pub key_handler: Option<Box<dyn FnMut(crate::runtime::NavKey)>>,
    /// 悬停处理（鼠标进入/离开时调用；take-put 防重入）。
    pub hover_handler: Option<Box<dyn FnMut(bool)>>,
    /// 逐字搜索处理（焦点作用域收到字符时调用；take-put 防重入）。
    pub char_handler: Option<Box<dyn FnMut(char)>>,
    /// TextField 绑定状态（键盘输入路由）。
    pub text_value: Option<MutableState<String>>,
    /// 滚动信息（滚动容器由 modifier 标记）。
    pub scroll: Option<ScrollInfo>,
    // ── 测量/布局产物 ──
    pub width: f32,
    pub height: f32,
    /// 在父 Row/Col 中占用的**单元格**尺寸（`(w, h)`）。
    ///
    /// `weight(w, fill=false)` 时孩子自身保持固有尺寸，但主轴上仍按分配份额占位
    /// （对齐 Compose：后续兄弟从单元格末尾开始），否则兄弟节点会叠在其上。
    pub cell: (f32, f32),
    /// 在父内容区的偏移。
    pub x: f32,
    pub y: f32,
    // ── 增量渲染（只重绘变化的子树）──
    /// 需要重新布局/重绘。首次为 true（首帧全量），其后仅在几何或内容变化时被置位。
    pub dirty: bool,
    /// 上一帧几何（用于检测几何是否变化 → 标记子/祖先脏）。
    pub last_geo: Option<(f32, f32, f32, f32)>,
    /// 上一帧滚动偏移（检测偏移变化 → 标记子树脏，使增量渲染不跳过滚动内容）。
    pub last_scroll: f32,
    // ── 增量绘制（对齐 Compose `LayerState`：一节点一段命令，父节点不嵌入子树命令）──
    /// 本节点自身绘制片段缓存（绝对坐标）。位置/尺寸/滚动键一致且未标脏时直接回放。
    pub draw_cache: Option<DrawLayer>,
    /// 需重绘（内容/几何/时间相关变化）。布局 `dirty` 会在稳定后清脏，但绘制需保留该标记，
    /// 以驱动「回放失效」，避免缓存冻结时间相关节点（光标闪烁/Pulse）。
    pub draw_invalid: bool,
}

/// 单个节点的绘制片段：前缀（背景/材质/裁剪/自身绘制）与后缀（滚动条/边框/ClipPop），
/// 孩子的命令夹在两者之间，由父层按序拼接，不复制进本节点。
///
/// 两段各带一个内容哈希：保留模式据此在不复制命令的前提下判定「这段与上一帧同内容」。
#[derive(PartialEq)]
pub struct DrawLayer {
    /// 回放判据：`(绝对 x, 绝对 y, 宽, 高, 滚动平移 x, 滚动平移 y)`。
    /// 祖先位移会改变后代的绝对坐标 ⇒ 后代键自然失配，无需向下广播失效。
    pub(crate) key: [f32; 6],
    pub(crate) prefix: Vec<DrawCmd>,
    pub(crate) prefix_hash: u64,
    pub(crate) suffix: Vec<DrawCmd>,
    pub(crate) suffix_hash: u64,
}

impl Node {
    fn new(policy: Policy, modifier: Modifier) -> Self {
        Self {
            alive: true,
            parent: None,
            children: Vec::new(),
            policy,
            modifier,
            draw: None,
            owner_scope: None,
            on_click: None,
            on_secondary_click: None,
            on_press: None,
            key_handler: None,
            hover_handler: None,
            char_handler: None,
            text_value: None,
            scroll: None,
            width: 0.0,
            height: 0.0,
            cell: (0.0, 0.0),
            x: 0.0,
            y: 0.0,
            dirty: true,
            last_geo: None,
            last_scroll: 0.0,
            draw_cache: None,
            draw_invalid: false,
        }
    }
}

/// 滚动容器信息。
#[derive(Clone)]
pub struct ScrollInfo {
    pub vertical: bool,
    /// 当前滚动偏移（原始存储，不走状态系统 — 避免重组放大）。
    pub offset: f32,
    /// 内容尺寸（主轴）。
    pub content_size: f32,
    /// 需保持可见的子节点（布局时校正偏移使其进入视口；**应用一次后清除**，
    /// 以免与用户手动滚动冲突）。
    pub sticky: Option<NodeId>,
}

/// 标记节点本身 + 全部祖先为脏（作用域重组 / 几何变化时，保证祖先不被「跳过」而漏掉本节点）。
thread_local! {
    /// 是否有节点自上次绘制遍历以来被标记失效（对齐 Compose「layer 失效汇总」）。
    /// 逐节点图层不再将子树命令嵌入祖先，故不需要「沿祖先链置标记」来传递汇总信号。
    static RENDER_DIRTY: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    /// 是否已完成过至少一次绘制遍历（首帧必须画）。
    static SCENE_WALKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[inline]
fn mark_render_dirty() {
    RENDER_DIRTY.with(|c| c.set(true));
}

pub(crate) fn mark_dirty_path(rt: &mut Rt, mut id: NodeId) {
    loop {
        rt.nodes[id.index()].dirty = true;
        rt.nodes[id.index()].draw_invalid = true;
        match rt.nodes[id.index()].parent {
            Some(p) => id = p,
            None => break,
        }
    }
    mark_render_dirty();
}

/// 标记节点本身 + 全部后代为脏（父节点几何变化 / 滚动偏移变化时，后代绝对坐标随之变化）。
pub(crate) fn mark_dirty_subtree(rt: &mut Rt, id: NodeId) {
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        rt.nodes[n.index()].dirty = true;
        rt.nodes[n.index()].draw_invalid = true;
        for i in 0..rt.nodes[n.index()].children.len() {
            stack.push(rt.nodes[n.index()].children[i]);
        }
    }
    mark_render_dirty();
}

/// 标记节点全部祖先为绘制失效（不标自身）：仍供「祖先自身片段也依赖后代状态」的控件使用。
pub(crate) fn mark_draw_invalid_path(rt: &mut Rt, mut id: NodeId) {
    while let Some(p) = rt.nodes[id.index()].parent {
        rt.nodes[p.index()].draw_invalid = true;
        id = p;
    }
    mark_render_dirty();
}

/// 标记节点及其**全部后代**为绘制失效（不动 `dirty`，不强制重布局）：
/// 用于「子节点绘制内容依赖共享状态」的情形（如 OTP 焦点格存在 `Rc<Cell>` 里，边框画在各格子节点上）。
pub(crate) fn mark_draw_invalid_subtree(rt: &mut Rt, id: NodeId) {
    // 叶子（光标 / Spinner / 进度条这类 draw-phase 节点）是最热路径：免去 Vec 分配。
    if rt.nodes[id.index()].children.is_empty() {
        rt.nodes[id.index()].draw_invalid = true;
        mark_render_dirty();
        return;
    }
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        rt.nodes[n.index()].draw_invalid = true;
        for i in 0..rt.nodes[n.index()].children.len() {
            stack.push(rt.nodes[n.index()].children[i]);
        }
    }
    mark_render_dirty();
}

/// 绘制失效一个子树（自身 + 后代 + 祖先链）：时间相关绘制不走重组时（光标闪烁、draw-phase 进度动画、
/// 非响应式 Cell 驱动的高亮）用此入口，而不是直接写 `node.draw_invalid = true`。
///
/// 两个方向都必须走穿：
/// - 向下：孩子自己的片段嵌在孩子的图层缓存里，只标自身仍会回放旧孩子命令（光标/高亮冻结）；
/// - 向上：祖先链一并失效，保证任何以「祖先是否脏」为判据的回放路径都不会跳过本子树。
pub fn invalidate_draw_node(id: NodeId) {
    crate::runtime::with_rt_pub(|rt| {
        mark_draw_invalid_subtree(rt, id);
        mark_draw_invalid_path(rt, id);
    });
}

// ══════════════════════════════ Modifier ══════════════════════════════

/// 液态玻璃规格 — 毛玻璃面板的材质参数（`Modifier::glass` 与 `DrawCmd::GlassPanel` 共用）。
///
/// 渲染层用 valo 的 `save_layer_backdrop` + 高斯背景模糊实现真正的 frosted glass：
/// 面板底色半透明、边缘高光（`highlight`）、细描边（`outline`）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassSpec {
    /// 是否启用玻璃材质；关闭时控件应退回不透明底色。
    pub enabled: bool,
    /// 背景高斯模糊 sigma（px）。
    pub blur: f32,
    /// 玻璃底色（含 alpha，叠在模糊结果之上）。
    pub tint: Color,
    /// 边缘高光色（液态玻璃的镜面反光）。
    pub highlight: Color,
    /// 描边色。
    pub outline: Color,
    /// 描边/高光线宽（px）。
    pub line: f32,
    /// 圆角半径（px）。
    pub radius: f32,
}

impl GlassSpec {
    /// 覆盖圆角半径（各控件尺寸不同，复用同一材质规格）。
    pub fn with_radius(self, radius: f32) -> Self {
        Self { radius, ..self }
    }
    /// 覆盖模糊强度。
    pub fn with_blur(self, blur: f32) -> Self {
        Self { blur, ..self }
    }
    /// 关闭态（控件用普通不透明底色）。
    pub fn off() -> Self {
        Self {
            enabled: false,
            blur: 0.0,
            tint: Color::TRANSPARENT,
            highlight: Color::TRANSPARENT,
            outline: Color::TRANSPARENT,
            line: 1.0,
            radius: 12.0,
        }
    }
}

impl Default for GlassSpec {
    fn default() -> Self {
        Self::off()
    }
}

/// 修饰符值链 — Compose `Modifier`（不可变值，`clone` 廉价）。
#[derive(Clone, Default)]
pub struct Modifier {
    pub(crate) elems: Vec<ModElem>,
}

#[derive(Clone)]
pub(crate) enum ModElem {
    Padding {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    /// 定宽/定高（px）。
    Size {
        width: Option<f32>,
        height: Option<f32>,
    },
    /// 最小宽/高（px）— Compose `Modifier.defaultMinSize` / `sizeIn`。
    MinSize {
        width: Option<f32>,
        height: Option<f32>,
    },
    /// 最大高度（px）— Compose `Modifier.heightIn(max = …)`。
    MaxHeight(f32),
    /// 最大宽度（px）— Compose `Modifier.widthIn(max = …)`。
    MaxWidth(f32),
    /// 展开进度（0..1）— Compose `AnimatedVisibility` + `expandVertically` 的测量语义。
    Reveal(f32),
    FillMaxWidth(f32),
    FillMaxHeight(f32),
    Background {
        color: Color,
        radius: f32,
    },
    /// 线性渐变背景（默认上→下；`vertical=false` 为左→右）。
    BackgroundGradient {
        from: Color,
        to: Color,
        radius: f32,
        vertical: bool,
    },
    /// Material 3 高度（elevation）→ 柔和投影，画在本节点背景**之前**（不参与测量）。
    Shadow {
        elevation: f32,
        radius: f32,
        color: Color,
    },
    /// 液态玻璃背景（`Modifier::glass`）。
    Glass(GlassSpec),
    Border {
        width: f32,
        color: Color,
        radius: f32,
    },
    Offset {
        x: f32,
        y: f32,
    },
    /// 锚定到 `anchor` 节点正下方：绝对位置 = 锚点位置 + (dx, 锚点高度 + gap)。
    ///
    /// 用于浮层（菜单/气泡）锚定触发节点：组合期锚点可能刚重建、坐标尚未布局，
    /// 故延迟到布局阶段再读（锚点子树先于浮层落位，位置已就绪）。
    PinTo {
        anchor: NodeId,
        dx: f32,
        gap: f32,
        match_width: bool,
    },
    /// 锚定到**窗口全局坐标**：落位期把 `(x, y)` 减去父节点的绝对位置得到父内偏移。
    ///
    /// 浮层坐标常来自 `last_pointer_global()` 等窗口坐标，而宿主根节点未必位于
    /// `(0, 0)`（IDE 外壳的菜单栏/状态栏之下、带 padding 的内容区等）。此时若用
    /// `Offset`，全局坐标会被当成局部位移，浮层整体偏移到布局中部或窗外。
    PinAt {
        x: f32,
        y: f32,
    },
    Align(Alignment),
    Weight(f32, bool),
    Clip(f32),
    Scroll {
        vertical: bool,
    },
    /// 命中测试穿透：本节点（含子树）不参与 `hit_test`（纯装饰浮层用）。
    NoHit,
    /// 关闭按压波纹：本节点被点击时不画波纹。
    ///
    /// 波纹默认对所有带 `on_click` 的节点开启（对齐 Flutter `InkWell`），因此需要例外的是
    /// 「整屏遮罩」这类点击即关闭、且波纹会糊满全屏的节点 —— scrim、模态浮层。
    ///
    /// 只影响绘制，不参与测量/落位/命中，因此不需要在其它三处接线。
    NoIndication,
    /// 隐藏滚动条：滚动容器仍可滚动（滚轮/拖拽），但不绘制滚动条、
    /// 也不在交叉轴预留 gutter（TabRow 等靠自动滚入视口而非滚动条导航的场景）。
    /// 只影响绘制与预留，不改滚动能力。
    NoScrollbar,
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn padding(self, all: f32) -> Modifier {
        self.padding_hv(all, all)
    }
    pub fn padding_hv(self, horizontal: f32, vertical: f32) -> Modifier {
        self.then(ModElem::Padding {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        })
    }
    pub fn padding_ltrb(self, left: f32, top: f32, right: f32, bottom: f32) -> Modifier {
        self.then(ModElem::Padding {
            left,
            top,
            right,
            bottom,
        })
    }
    pub fn width(self, w: f32) -> Modifier {
        self.then(ModElem::Size {
            width: Some(w),
            height: None,
        })
    }
    pub fn height(self, h: f32) -> Modifier {
        self.then(ModElem::Size {
            width: None,
            height: Some(h),
        })
    }
    pub fn size(self, w: f32, h: f32) -> Modifier {
        self.then(ModElem::Size {
            width: Some(w),
            height: Some(h),
        })
    }
    /// 最小宽度（Compose `defaultMinSize(minWidth = …)`）。
    pub fn min_width(self, w: f32) -> Modifier {
        self.then(ModElem::MinSize {
            width: Some(w),
            height: None,
        })
    }
    /// 最小高度（Compose `defaultMinSize(minHeight = …)`）。
    pub fn min_height(self, h: f32) -> Modifier {
        self.then(ModElem::MinSize {
            width: None,
            height: Some(h),
        })
    }
    /// 最小宽高（Compose `defaultMinSize`）。
    pub fn min_size(self, w: f32, h: f32) -> Modifier {
        self.then(ModElem::MinSize {
            width: Some(w),
            height: Some(h),
        })
    }
    /// 最大高度（Compose `heightIn(max = …)`）。只收紧上界，内容更矮时仍按自然高度输出。
    ///
    /// 竖向展开动画的基础：把 `max_height` 从 0 逐帧动画到内容高度，父容器每帧重排即可
    /// （配 `.clip(0)` 裁掉溢出部分）。
    pub fn max_height(self, h: f32) -> Modifier {
        self.then(ModElem::MaxHeight(h))
    }
    /// 最大宽度（Compose `widthIn(max = …)`）。只收紧上界，内容更窄时仍按自然宽度输出。
    ///
    /// 文本气泡 / 卡片正文的换行宽度上界：内容超宽时折行，不足时保持紧凑。
    pub fn max_width(self, w: f32) -> Modifier {
        self.then(ModElem::MaxWidth(w))
    }
    /// 竖向展开：内容按自然高度测量，本节点高度取 `frac ×` 自然高度。
    ///
    /// 配 `.clip(0)` 裁掉溢出，即 Compose `expandVertically` 的效果。与
    /// `max_height` 的区别：后者会把约束传给内容（内容被压扁），本修饰符不会。
    pub fn reveal_height(self, frac: f32) -> Modifier {
        self.then(ModElem::Reveal(frac.clamp(0.0, 1.0)))
    }
    pub fn fill_max_width(self) -> Modifier {
        self.fill_max_width_frac(1.0)
    }
    pub fn fill_max_width_frac(self, f: f32) -> Modifier {
        self.then(ModElem::FillMaxWidth(f))
    }
    pub fn fill_max_height(self) -> Modifier {
        self.fill_max_height_frac(1.0)
    }
    pub fn fill_max_height_frac(self, f: f32) -> Modifier {
        self.then(ModElem::FillMaxHeight(f))
    }
    pub fn fill_max_size(self) -> Modifier {
        self.fill_max_width().fill_max_height()
    }
    pub fn background(self, color: Color, radius: f32) -> Modifier {
        self.then(ModElem::Background { color, radius })
    }

    /// 上→下线性渐变背景（页头 / 英雄卡）。
    pub fn background_gradient(self, from: Color, to: Color, radius: f32) -> Modifier {
        self.then(ModElem::BackgroundGradient {
            from,
            to,
            radius,
            vertical: true,
        })
    }

    /// 左→右线性渐变背景。
    pub fn background_gradient_h(self, from: Color, to: Color, radius: f32) -> Modifier {
        self.then(ModElem::BackgroundGradient {
            from,
            to,
            radius,
            vertical: false,
        })
    }
    /// 液态玻璃背景：`enabled` 为真时走背景模糊材质，否则控件自行退回不透明底色。
    pub fn glass(self, spec: GlassSpec) -> Modifier {
        self.then(ModElem::Glass(spec))
    }
    pub fn border(self, width: f32, color: Color, radius: f32) -> Modifier {
        self.then(ModElem::Border {
            width,
            color,
            radius,
        })
    }
    /// Material 3 高度：给本节点铺一层柔和投影（不影响测量尺寸）。
    ///
    /// `elevation` 用 M3 的 0/1/3/6/8/12 档位；投影用同心圆角矩形叠加近似模糊，
    /// 因此只适合静态表面（卡片 / FAB / 对话框），不适合每帧移动的元件。
    pub fn shadow(self, elevation: f32, radius: f32, color: Color) -> Modifier {
        self.then(ModElem::Shadow {
            elevation,
            radius,
            color,
        })
    }
    pub fn offset(self, x: f32, y: f32) -> Modifier {
        self.then(ModElem::Offset { x, y })
    }
    /// 锚定到 `anchor` 节点正下方 + gap（布局期解析，见 [`ModElem::PinTo`]）。
    pub fn pin_below(self, anchor: NodeId, gap: f32) -> Modifier {
        self.then(ModElem::PinTo {
            anchor,
            dx: 0.0,
            gap,
            match_width: false,
        })
    }
    /// 同 [`Modifier::pin_below`]，且宽度不小于锚点宽度（下拉面板对齐触发框）。
    pub fn pin_below_width(self, anchor: NodeId, gap: f32) -> Modifier {
        self.then(ModElem::PinTo {
            anchor,
            dx: 0.0,
            gap,
            match_width: true,
        })
    }
    /// 把浮层左上角锚定到**窗口全局坐标** `(x, y)`（布局期换算，见 [`ModElem::PinAt`]）。
    ///
    /// 与 `offset` 的区别：`offset` 是父容器内的相对位移；这里的坐标来自指针事件，
    /// 宿主根节点自身可能带偏移。另会自动按视口边界夹取，使浮层完整落在窗口内。
    pub fn pin_at_global(self, x: f32, y: f32) -> Modifier {
        self.then(ModElem::PinAt { x, y })
    }
    /// Box 内对齐。
    pub fn align(self, a: Alignment) -> Modifier {
        self.then(ModElem::Align(a))
    }
    /// Row/Column 内权重（对齐 `RowScope.weight`）。
    pub fn weight(self, w: f32, fill: bool) -> Modifier {
        self.then(ModElem::Weight(w, fill))
    }
    pub fn clip(self, radius: f32) -> Modifier {
        self.then(ModElem::Clip(radius))
    }
    /// 标记为滚动容器（本节点主轴可滚）。
    pub fn VerticalScroll(self) -> Modifier {
        self.then(ModElem::Scroll { vertical: true })
    }
    pub fn HorizontalScroll(self) -> Modifier {
        self.then(ModElem::Scroll { vertical: false })
    }
    /// 隐藏本滚动容器的滚动条（仍可用滚轮/拖拽滚动，但不绘制、不预留 gutter）。
    pub fn hide_scrollbar(self) -> Modifier {
        self.then(ModElem::NoScrollbar)
    }
    /// 命中测试穿透：本节点（含子树）不接受点击/悬停命中，事件落到其下方的兄弟或父级。
    ///
    /// 用于覆盖层（聚焦外框、装饰性描边等）——它们绘制在内容之上但不该吃掉事件。
    pub fn no_hit(self) -> Modifier {
        self.then(ModElem::NoHit)
    }

    /// 关闭按压波纹：本节点点击时不画波纹（默认对所有 `on_click` 节点开启）。
    ///
    /// 用于「整屏遮罩」——点一下就关，波纹糊满全屏只会难看（Flutter 的 modal barrier 同样无波）。
    pub fn no_indication(self) -> Modifier {
        self.then(ModElem::NoIndication)
    }

    fn then(self, e: ModElem) -> Modifier {
        let mut m = self;
        m.elems.push(e);
        m
    }

    pub(crate) fn padding_of(&self) -> (f32, f32, f32, f32) {
        let mut p = (0.0, 0.0, 0.0, 0.0);
        for e in &self.elems {
            if let ModElem::Padding {
                left,
                top,
                right,
                bottom,
            } = e
            {
                p.0 += *left;
                p.1 += *top;
                p.2 += *right;
                p.3 += *bottom;
            }
        }
        p
    }

    /// `Modifier.offset` 的累计位移 — 只在**落位**阶段生效（不参与测量，对齐 Compose）。
    pub(crate) fn offset_of(&self) -> (f32, f32) {
        let mut o = (0.0, 0.0);
        for e in &self.elems {
            if let ModElem::Offset { x, y } = e {
                o.0 += *x;
                o.1 += *y;
            }
        }
        o
    }

    /// [`ModElem::PinTo`] 的锚定信息（若有）。
    pub(crate) fn pin_of(&self) -> Option<(NodeId, f32, f32, bool)> {
        self.elems.iter().find_map(|e| match e {
            ModElem::PinTo {
                anchor,
                dx,
                gap,
                match_width,
            } => Some((*anchor, *dx, *gap, *match_width)),
            _ => None,
        })
    }

    /// [`ModElem::PinAt`] 的窗口全局锚点（若有）。
    pub(crate) fn pin_at_of(&self) -> Option<(f32, f32)> {
        self.elems.iter().find_map(|e| match e {
            ModElem::PinAt { x, y } => Some((*x, *y)),
            _ => None,
        })
    }

    pub(crate) fn is_no_hit(&self) -> bool {
        self.elems.iter().any(|e| matches!(e, ModElem::NoHit))
    }

    /// 本节点是否关闭了按压波纹。
    pub fn ripples_off(&self) -> bool {
        self.elems.iter().any(|e| matches!(e, ModElem::NoIndication))
    }

    /// 本滚动容器是否隐藏了滚动条（`Modifier::hide_scrollbar`）。
    pub fn scrollbar_hidden(&self) -> bool {
        self.elems.iter().any(|e| matches!(e, ModElem::NoScrollbar))
    }
}

// ══════════════════════════════ 对齐 / 排列 ══════════════════════════════

/// Box 内对齐 — Compose `Alignment`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Alignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl Alignment {
    pub(crate) fn align(&self, cw: f32, ch: f32, w: f32, h: f32) -> (f32, f32) {
        let x = match self {
            Self::TopStart | Self::CenterStart | Self::BottomStart => 0.0,
            Self::TopCenter | Self::Center | Self::BottomCenter => (cw - w) * 0.5,
            Self::TopEnd | Self::CenterEnd | Self::BottomEnd => cw - w,
        };
        let y = match self {
            Self::TopStart | Self::TopCenter | Self::TopEnd => 0.0,
            Self::CenterStart | Self::Center | Self::CenterEnd => (ch - h) * 0.5,
            Self::BottomStart | Self::BottomCenter | Self::BottomEnd => ch - h,
        };
        (x, y)
    }
}

/// 主轴排列 — Compose `Arrangement`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Arrangement {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// 交叉轴对齐 — Compose `Alignment.Vertical/Horizontal`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CrossAxisAlignment {
    Start,
    Center,
    End,
    Stretch,
}

// ══════════════════════════════ 布局策略 ══════════════════════════════

/// 节点布局策略。
pub enum Policy {
    Box(Alignment),
    Row {
        arrangement: Arrangement,
        cross: CrossAxisAlignment,
    },
    Column {
        arrangement: Arrangement,
        cross: CrossAxisAlignment,
    },
    /// 无子节点，自身绘制内容（text / Spacer / Canvas）。
    Leaf,
    /// 自定义测量策略 — Compose `Layout(measurePolicy) { content }`。
    Custom(Box<dyn MeasurePolicy>),
}

/// 自定义测量策略 — 对齐 Compose `MeasurePolicy`。
pub trait MeasurePolicy {
    /// 父定约束、子测尺寸、返回自身尺寸与孩子落位。
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx,
        measurables: &[NodeId],
        constraints: Constraints,
    ) -> MeasureResult;
}

pub struct MeasureResult {
    pub width: f32,
    pub height: f32,
    /// (child, x, y)
    pub placements: Vec<(NodeId, f32, f32)>,
}

/// 测量上下文（借道 &mut RuntimeData，避免重入）。
pub struct MeasureCtx<'a> {
    pub(crate) rt: &'a mut Rt,
}

impl<'a> MeasureCtx<'a> {
    /// 测量子节点（每个孩子一趟 — Compose 单趟测量契约）。
    pub fn measure(&mut self, id: NodeId, constraints: Constraints) -> Placeable {
        let (w, h) = measure_node(self.rt, id, constraints);
        Placeable {
            id,
            width: w,
            height: h,
        }
    }

    /// 干测量：不落布局结果，仅询问期望尺寸。
    pub fn dry_measure(&mut self, id: NodeId, constraints: Constraints) -> Size {
        let (w, h) = measure_node(self.rt, id, constraints);
        Size::new(w, h)
    }

    pub fn modifier_of(&self, id: NodeId) -> Modifier {
        self.rt.nodes[id.index()].modifier.clone()
    }
}

/// 可放置对象 — Compose `Placeable`（place 由 placements 表达）。
pub struct Placeable {
    pub id: NodeId,
    pub width: f32,
    pub height: f32,
}

impl Placeable {
    pub fn place(&self, placements: &mut Vec<(NodeId, f32, f32)>, x: f32, y: f32) {
        placements.push((self.id, x, y));
    }
}

// ══════════════════════════════ 节点创建 ══════════════════════════════

/// 创建节点并挂到当前父节点。
pub fn add_node(policy: Policy, modifier: Modifier) -> NodeId {
    crate::runtime::with_rt_pub(|rt| {
        let parent = rt.parent_stack.last().copied();
        let mut node = Node::new(policy, modifier);
        node.parent = parent;
        // 槽位复用：重组会整棵子树重建，若只增不减 arena 会随帧数线性膨胀
        // （实测动画页每帧 ~357 个孤儿节点，长时间运行到数百万）。
        let id = match rt.node_free.pop() {
            Some(i) => {
                rt.nodes[i as usize] = node;
                NodeId(i)
            }
            None => {
                let id = NodeId(rt.nodes.len() as u32);
                rt.nodes.push(node);
                id
            }
        };
        if let Some(p) = parent {
            rt.nodes[p.index()].children.push(id);
        }
        id
    })
}

/// 节点 arena 可达性 GC：标记「还有人要」的节点，其余归还自由表。
///
/// 起点三类：
/// - 组合根（`root_nodes`）与无父节点的独立根；
/// - **每个存活作用域的锚节点** —— 作用域树与节点树会错位：某个活作用域的节点
///   可能已不在任何父节点的 children 里，但它仍是该作用域内容/祖先链的落点；
/// - 向上遍历父链：`CompositionLocal` 沿节点父链找提供者，链断一格就 panic，
///   所以存活节点的整条祖先链必须保活。
///
/// 只做「摘除即释放」会误伤第二类（实测页面内容作用域的锚节点在祖先重建时被摘掉），
/// 只做向下可达又会漏掉父链，二者叠加才是安全判据。
pub(crate) fn sweep_nodes() {
    crate::runtime::with_rt_pub(|rt| {
        let n = rt.nodes.len();
        if n == 0 {
            return;
        }
        let mut seen = vec![false; n];
        let mut stack: Vec<NodeId> = rt.root_nodes.clone();
        for i in 0..n {
            if rt.nodes[i].alive && rt.nodes[i].parent.is_none() {
                stack.push(NodeId(i as u32));
            }
        }
        for s in rt.scopes.iter().flatten() {
            if !s.disposed {
                if let Some(nd) = s.node {
                    stack.push(nd);
                }
            }
        }
        while let Some(id) = stack.pop() {
            let i = id.index();
            if i >= n || !rt.nodes[i].alive || seen[i] {
                continue;
            }
            seen[i] = true;
            for k in &rt.nodes[i].children {
                stack.push(*k);
            }
            if let Some(p) = rt.nodes[i].parent {
                stack.push(p);
            }
        }
        for i in 0..n {
            if !rt.nodes[i].alive || seen[i] {
                continue;
            }
            if let Some(p) = rt.nodes[i].parent {
                rt.nodes[p.index()].children.retain(|c| *c != NodeId(i as u32));
            }
            if std::env::var_os("COMPOSE_DEBUG_GC").is_some() {
                eprintln!("[gc] free node={} owner={:?}", i, rt.nodes[i].owner_scope.map(|o| o.0));
            }
            let mut dead = Node::new(Policy::Box(Alignment::TopStart), Modifier::new());
            dead.alive = false;
            rt.nodes[i] = dead;
            rt.node_free.push(i as u32);
            if rt.hovered_node == Some(i) {
                rt.hovered_node = None;
            }
        }
    });
}

/// 以节点为父运行内容作用域（组合容器 widget 的实现核心）。
pub fn with_content_scope(id: NodeId, content: impl Fn() + 'static) -> crate::runtime::ScopeId {
    crate::runtime::emit_scope_into(id, content)
}

// ══════════════════════════════ 测量 ══════════════════════════════

/// 应用 modifier 的约束变换（padding/size/fill*），返回 (内容约束, 尺寸修正闭包数据)。
struct ModMeasure {
    content: Constraints,
    pad: (f32, f32, f32, f32),
    fill_w: Option<f32>,
    fill_h: Option<f32>,
    /// `.reveal_height(frac)` 的进度：孩子按**自然高度**测量，本节点高度取 `frac ×` 它。
    reveal: Option<f32>,
}

fn apply_measure_mods(mods: &Modifier, incoming: Constraints) -> ModMeasure {
    let (pl, pt, pr, pb) = mods.padding_of();
    let mut c = Constraints::new(
        incoming.min_width,
        (incoming.max_width - pl - pr).max(0.0),
        incoming.min_height,
        (incoming.max_height - pt - pb).max(0.0),
    );
    c.min_width = (c.min_width - pl - pr).max(0.0);
    c.min_height = (c.min_height - pt - pb).max(0.0);
    let mut mm = ModMeasure {
        content: c,
        pad: (pl, pt, pr, pb),
        fill_w: None,
        fill_h: None,
        reveal: None,
    };
    for e in &mods.elems {
        match e {
            ModElem::Size {
                width: Some(w),
                height: Some(h),
            } => {
                mm.content = Constraints::tight(*w, *h).constrain_by(mm.content);
            }
            ModElem::Size {
                width: Some(w),
                height: None,
            } => {
                mm.content = Constraints::new(*w, *w, mm.content.min_height, mm.content.max_height);
            }
            ModElem::Size {
                width: None,
                height: Some(h),
            } => {
                mm.content = Constraints::new(mm.content.min_width, mm.content.max_width, *h, *h);
            }
            ModElem::MinSize { width, height } => {
                mm.content = mm
                    .content
                    .with_min(width.unwrap_or(0.0), height.unwrap_or(0.0));
            }
            ModElem::MaxHeight(h) => {
                // 只收紧上界，且不得跌破已有下界（保持 min ≤ max 不变式）。
                mm.content.max_height = mm.content.max_height.min(*h).max(mm.content.min_height);
            }
            ModElem::MaxWidth(w) => {
                mm.content.max_width = mm.content.max_width.min(*w).max(mm.content.min_width);
            }
            ModElem::Reveal(f) => {
                // 内容按无界高度测量（拿到自然高度），节点高度稍后按 `f` 折算。
                mm.reveal = Some(*f);
                mm.content.max_height = f32::INFINITY;
            }
            ModElem::FillMaxWidth(f) => {
                if incoming.has_bounded_width() {
                    mm.fill_w = Some(incoming.max_width * f);
                }
            }
            ModElem::FillMaxHeight(f) => {
                if incoming.has_bounded_height() {
                    mm.fill_h = Some(incoming.max_height * f);
                }
            }
            _ => {}
        }
    }
    // fill 修正：内容约束收紧到 fill 目标。注意 fill 目标是**节点总宽/高**（含 padding），
    // 内容约束须先扣除 padding，否则内容区比实际宽 2×padding，导致右/下溢出。
    if let Some(fw) = mm.fill_w {
        let target = (fw - pl - pr).max(0.0);
        mm.content.min_width = mm.content.min_width.max(target);
        mm.content.max_width = mm.content.max_width.min(target).max(mm.content.min_width);
    }
    if let Some(fh) = mm.fill_h {
        let target = (fh - pt - pb).max(0.0);
        mm.content.min_height = mm.content.min_height.max(target);
        mm.content.max_height = mm.content.max_height.min(target).max(mm.content.min_height);
    }
    mm
}

trait ConstraintsExt {
    fn constrain_by(&self, outer: Constraints) -> Constraints;
}
impl ConstraintsExt for Constraints {
    /// 以 self 为目标紧约束，被 outer 限幅。
    fn constrain_by(&self, outer: Constraints) -> Constraints {
        Constraints::new(
            self.min_width.clamp(outer.min_width, outer.max_width),
            self.max_width.clamp(outer.min_width, outer.max_width),
            self.min_height.clamp(outer.min_height, outer.max_height),
            self.max_height.clamp(outer.min_height, outer.max_height),
        )
    }
}

/// 本轮 `perform_layout` 真正测量 / 落位的节点数（缓存命中的子树不计）。
/// 诊断增量布局退化用：动画页若每帧接近全树规模，说明脏区扩散到了整棵树。
pub fn layout_node_counts() -> (u32, u32) {
    (MEASURED.with(|c| c.get()), PLACED.with(|c| c.get()))
}

thread_local! {
    static MEASURED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static PLACED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 测量节点（返回受 modifier 修正后的最终尺寸）。
pub(crate) fn measure_node(rt: &mut Rt, id: NodeId, incoming: Constraints) -> (f32, f32) {
    // 增量：几何未变（脏标记清除）直接复用缓存尺寸，跳过整棵子树测量。
    if !rt.nodes[id.index()].dirty {
        let n = &rt.nodes[id.index()];
        return (n.width, n.height);
    }
    MEASURED.with(|c| c.set(c.get() + 1));
    let modifier = rt.nodes[id.index()].modifier.clone();
    let mm = apply_measure_mods(&modifier, incoming);
    // `PinTo.match_width`：宽度固定为锚点宽度（不增不减，避免浮层被 `fill_max_width`
    // 子项在宿主的宽松约束下撑到窗口宽度；锚点子树在本轮测量中先于本节点）。
    let mm = if let Some(aw) = modifier.elems.iter().find_map(|e| match e {
        ModElem::PinTo {
            anchor,
            match_width: true,
            ..
        } => Some(rt.nodes[anchor.index()].width),
        _ => None,
    }) {
        let need = (aw - mm.pad.0 - mm.pad.2).max(0.0);
        let mut c = mm.content;
        c.min_width = need;
        c.max_width = need;
        ModMeasure { content: c, ..mm }
    } else {
        mm
    };
    let (w, h) = measure_policy(rt, id, mm.content);
    // padding / fill 加回
    let mut fw = w + mm.pad.0 + mm.pad.2;
    let mut fh = h + mm.pad.1 + mm.pad.3;
    // `.reveal_height(frac)`：内容已按自然高度测出，节点只露出 frac 比例。
    if let Some(frac) = mm.reveal {
        fh *= frac;
    }
    if let Some(fw_target) = mm.fill_w {
        if incoming.has_bounded_width() {
            fw = incoming.max_width * fw_target / fw_target.max(1e-6);
            fw = incoming.max_width;
        }
    }
    if let Some(fh_target) = mm.fill_h {
        if incoming.has_bounded_height() {
            fh = incoming.max_height * fh_target / fh_target.max(1e-6);
            fh = incoming.max_height;
        }
    }
    fw = incoming.constrain_width(fw);
    fh = incoming.constrain_height(fh);
    let node = &mut rt.nodes[id.index()];
    node.width = fw;
    node.height = fh;
    (fw, fh)
}

/// 按 policy 测量内容尺寸（不含 modifier 修正）。
fn measure_policy(rt: &mut Rt, id: NodeId, constraints: Constraints) -> (f32, f32) {
    // 取走 policy（take-put：内置策略直接算，Custom 需要 ctx 桥）
    let mut policy = std::mem::replace(&mut rt.nodes[id.index()].policy, Policy::Leaf);
    let result = match &mut policy {
        Policy::Box(_) => {
            let children = rt.nodes[id.index()].children.clone();
            let mut mw = 0.0f32;
            let mut mh = 0.0f32;
            for ch in &children {
                let child_mods = rt.nodes[ch.index()].modifier.clone();
                let align_override = child_mods.elems.iter().find_map(|e| match e {
                    ModElem::Align(a) => Some(*a),
                    _ => None,
                });
                let (cw, chh) = measure_node(rt, *ch, constraints.loose_constraints());
                mw = mw.max(cw);
                mh = mh.max(chh);
                let _ = align_override; // 落位时再读
            }
            (
                mw.max(constraints.min_width),
                mh.max(constraints.min_height),
            )
        }
        Policy::Row {
            arrangement: _,
            cross,
        } => {
            let (w, h) = measure_row_col(rt, id, constraints, true, *cross);
            (w, h)
        }
        Policy::Column {
            arrangement: _,
            cross,
        } => {
            let (w, h) = measure_row_col(rt, id, constraints, false, *cross);
            (w, h)
        }
        Policy::Leaf => {
            // Leaf 尺寸 = 绘制内容期望尺寸（text 由文本度量器给出；其余 0）
            let (w, h) = leaf_intrinsic(rt, id, constraints.max_width);
            (
                constraints.constrain_width(w),
                constraints.constrain_height(h),
            )
        }
        Policy::Custom(_) => {
            // ctx 桥：Custom 策略通过 MeasureCtx 访问同一 rt（无重入）
            let children = rt.nodes[id.index()].children.clone();
            let mut ctx = MeasureCtx { rt };
            if let Policy::Custom(p) = &mut policy {
                let r = p.measure(&mut ctx, &children, constraints);
                // 落位记入节点（place 阶段读取）
                let placements = r.placements;
                let (w, h) = (r.width, r.height);
                let node = &mut ctx.rt.nodes[id.index()];
                node.width = w;
                node.height = h;
                // 存 placements 到临时 — 直接写孩子 x/y
                for (cid, x, y) in placements {
                    let cn = &mut ctx.rt.nodes[cid.index()];
                    cn.x = x;
                    cn.y = y;
                }
                (w, h)
            } else {
                unreachable!()
            }
        }
    };
    rt.nodes[id.index()].policy = policy;
    result
}

trait LooseExt {
    fn loose_constraints(&self) -> Constraints;
}
impl LooseExt for Constraints {
    /// Box 语义：孩子以松弛约束测量（Compose `Box` 测量规则）。
    fn loose_constraints(&self) -> Constraints {
        Constraints::loose(self.max_width, self.max_height)
    }
}

/// Row / Column 测量（weight 孩子最后测 — Compose 契约）。
fn measure_row_col(
    rt: &mut Rt,
    id: NodeId,
    constraints: Constraints,
    is_row: bool,
    cross: CrossAxisAlignment,
) -> (f32, f32) {
    let children = rt.nodes[id.index()].children.clone();
    let stretch_cross = cross == CrossAxisAlignment::Stretch;
    let (main_max, cross_max) = if is_row {
        (constraints.max_width, constraints.max_height)
    } else {
        (constraints.max_height, constraints.max_width)
    };
    // 滚动容器：主轴对子节点**无界**（内容可超出视口，由滚动偏移裁剪显示）。
    // 否则「剩余空间」约束会把视口外的子节点压成 0 高 —— 滚动内容塌陷、无法滚动。
    let scroll_main = match rt.nodes[id.index()].scroll.as_ref() {
        Some(s) => s.vertical == !is_row,
        None => false,
    };
    let plain_main_max = if scroll_main { f32::INFINITY } else { main_max };
    // 滚动容器沿主轴滚动 → 在交叉轴末端预留滚动条 gutter（桌面端）。
    // 孩子用收紧后的交叉约束测量，内容永不被滚动条覆盖；容器自身尺寸仍吃满传入约束（min）。
    let cross_reserve = scrollbar_gutter_for(rt, id, is_row);
    let cross_max_c = (cross_max - cross_reserve).max(0.0);

    struct Child {
        id: NodeId,
        weight: Option<(f32, bool)>,
        size: (f32, f32),
    }
    let mut weighted = Vec::new();
    let mut plain = Vec::new();
    for ch in &children {
        let mods = rt.nodes[ch.index()].modifier.clone();
        let weight = mods.elems.iter().find_map(|e| match e {
            ModElem::Weight(w, f) => Some((*w, *f)),
            _ => None,
        });
        if weight.is_some() {
            weighted.push(Child {
                id: *ch,
                weight,
                size: (0.0, 0.0),
            });
        } else {
            plain.push(Child {
                id: *ch,
                weight: None,
                size: (0.0, 0.0),
            });
        }
    }

    let total_weight: f32 = weighted.iter().map(|c| c.weight.unwrap().0).sum();

    // 1) 无权重孩子：主轴松弛（剩余空间）
    let mut used = 0.0f32;
    let mut cross_max_seen = 0.0f32;
    {
        let remaining = (main_max - used).max(0.0);
        let _ = remaining;
    }
    // 先干测无权重孩子以决定 weight 分配前的已用空间
    for c in plain.iter_mut() {
        // Compose：主轴 max = 剩余；交叉轴松弛（或 Stretch 收紧到 cross_max）
        // 滚动容器主轴无界（见上方 scroll_main）
        // Stretch 仅在交叉轴**有界**时收紧到 cross_max；无界（如滚动容器内）时
        // 无法拉伸，退回松弛让子节点保持自然尺寸 —— 否则会把 cross 尺寸撑成
        // INFINITY，导致背景/描边以无限尺寸绘制而不可见（Compose Stretch 同此语义）。
        let stretchable = if is_row {
            stretch_cross && constraints.has_bounded_height()
        } else {
            stretch_cross && constraints.has_bounded_width()
        };
        let mc = if is_row {
            if stretchable {
                Constraints::new(0.0, (plain_main_max - used).max(0.0), cross_max_c, cross_max_c)
            } else {
                Constraints::new(0.0, (plain_main_max - used).max(0.0), 0.0, cross_max_c)
            }
        } else if stretchable {
            Constraints::new(cross_max_c, cross_max_c, 0.0, (plain_main_max - used).max(0.0))
        } else {
            Constraints::new(0.0, cross_max_c, 0.0, (plain_main_max - used).max(0.0))
        };
        let (w, h) = measure_node(rt, c.id, mc);
        c.size = (w, h);
        rt.nodes[c.id.index()].cell = (w, h);
        used += if is_row { w } else { h };
        cross_max_seen = cross_max_seen.max(if is_row { h } else { w });
    }

    // 2) 权重孩子：按剩余主轴均分
    if total_weight > 0.0 {
        let remaining = (main_max - used).max(0.0);
        for c in weighted.iter_mut() {
            let (wt, fill) = c.weight.unwrap();
            let share = remaining * wt / total_weight;
            let mc = if is_row {
                if fill {
                    Constraints::tight(share, cross_max_c).with_loose_height()
                } else {
                    Constraints::new(0.0, share, 0.0, cross_max_c)
                }
            } else if fill {
                Constraints::new(0.0, cross_max_c, share, share)
            } else {
                Constraints::new(0.0, cross_max_c, 0.0, share)
            };
            let (w, h) = measure_node(rt, c.id, mc);
            c.size = (w, h);
            // 单元格主轴 = 分配份额：`fill=false` 只表示孩子不被拉伸，
            // 不改变它占用的空间（否则后续兄弟会叠上来）。
            let cell = if is_row { (share, h) } else { (w, share) };
            rt.nodes[c.id.index()].cell = cell;
            used += if is_row { cell.0 } else { cell.1 };
            cross_max_seen = cross_max_seen.max(if is_row { cell.1 } else { cell.0 });
        }
    }

    // 汇总
    let (mw, mh) = if is_row {
        (used, cross_max_seen)
    } else {
        (cross_max_seen, used)
    };
    // 受约束修正（incoming min/max）
    let fw = constraints.constrain_width(mw);
    let fh = constraints.constrain_height(mh);
    let node = &mut rt.nodes[id.index()];
    node.width = fw;
    node.height = fh;
    (fw, fh)
}

trait WithLooseHeight {
    fn with_loose_height(&self) -> Constraints;
}
impl WithLooseHeight for Constraints {
    fn with_loose_height(&self) -> Constraints {
        Constraints::new(self.min_width, self.max_width, 0.0, self.max_height)
    }
}

/// Leaf 期望尺寸。`avail_w` 用于让文本按父级给的宽度折行后上报真实高度。
fn leaf_intrinsic(rt: &mut Rt, id: NodeId, avail_w: f32) -> (f32, f32) {
    let node = &rt.nodes[id.index()];
    if let Some(draw) = &node.draw {
        // 预探测：Leaf 由闭包通过 DrawBuilder 上报期望尺寸
        let mut probe = DrawBuilder::probe();
        probe.probe_max_w = avail_w;
        draw(&mut probe, 0.0, 0.0, 0.0, 0.0);
        (probe.probe_w, probe.probe_h)
    } else {
        (0.0, 0.0)
    }
}

// ══════════════════════════════ 落位 ══════════════════════════════

/// 整树标记为脏（窗口尺寸变化等需整体重布局时使用）。
pub(crate) fn invalidate_all(rt: &mut Rt) {
    for n in rt.nodes.iter_mut() {
        n.dirty = true;
        n.last_geo = None;
    }
}

/// 布局入口：测量根 + 落位到 (0,0)。
pub fn perform_layout(root: NodeId, constraints: Constraints) -> Size {
    MEASURED.with(|c| c.set(0));
    PLACED.with(|c| c.set(0));
    crate::runtime::with_rt_pub(|rt| {
        // 约束变化（窗口尺寸变化）→ 整树失效重布局，避免跳过逻辑使用陈旧几何。
        let changed = rt.last_constraints != Some(constraints);
        if changed {
            invalidate_all(rt);
            rt.last_constraints = Some(constraints);
        } else if !rt.nodes[root.index()].dirty {
            // 增量布局：无失效标记且约束未变 → 整树几何已稳定，
            // 跳过 measure/place 递归与 O(N) 滚动结算循环（空闲帧近乎零成本）。
            // 任何脏（重组 / 几何 / 滚动）都会经 mark_dirty_path 传导至根，故根不脏即全树不脏。
            let n = &rt.nodes[root.index()];
            return Size::new(n.width, n.height);
        }
        let (w, h) = measure_node(rt, root, constraints);
        place_node(rt, root, 0.0, 0.0);
        Size::new(w, h)
    })
}

/// 节点绝对位置（沿父链累加 x/y；要求父链已落位）。
fn node_abs_pos(rt: &Rt, id: NodeId) -> (f32, f32) {
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut cur = Some(id);
    while let Some(n) = cur {
        x += rt.nodes[n.index()].x;
        y += rt.nodes[n.index()].y;
        cur = rt.nodes[n.index()].parent;
    }
    (x, y)
}

/// 递归落位：容器按 policy 摆孩子；叶子无需处理。
fn place_node(rt: &mut Rt, id: NodeId, x: f32, y: f32) {
    // 增量：节点未脏且本帧分配到的位置与缓存一致 → 整棵子树复用缓存几何，直接返回。
    // 仅依赖「脏标记」会遗漏「父重布局导致兄弟位移」的情形，故同时校验位置。
    let (dx, dy, dirty) = {
        let n = &rt.nodes[id.index()];
        (n.x, n.y, n.dirty)
    };
    if !dirty && (x - dx).abs() < 0.01 && (y - dy).abs() < 0.01 {
        return;
    }
    PLACED.with(|c| c.set(c.get() + 1));
    // `Modifier.offset`：落位时整体平移（含子树），不影响测量 — Compose 语义。
    let (ofx, ofy) = rt.nodes[id.index()].modifier.offset_of();
    // `Modifier.pin_below` / `pin_at_global`：落位期解析绝对位置（组合期几何尚未就绪，
    // 此时锚点子树与父节点已完成落位）。
    let pin = rt.nodes[id.index()].modifier.pin_of();
    let pin_at = rt.nodes[id.index()].modifier.pin_at_of();
    let is_pin_at = pin.is_none() && pin_at.is_some();
    let pin_pos = pin
        .map(|(anchor, pdx, gap, _mw)| {
            let ah = rt.nodes[anchor.index()].height;
            let (ax, ay) = node_abs_pos(rt, anchor);
            (ax + pdx, ay + ah + gap)
        })
        .or(pin_at);
    let parent = rt.nodes[id.index()].parent;
    let (ppx, ppy) = if pin_pos.is_some() {
        match parent {
            Some(p) => node_abs_pos(rt, p),
            None => (0.0, 0.0),
        }
    } else {
        (0.0, 0.0)
    };
    {
        let n = &mut rt.nodes[id.index()];
        n.x = x + ofx;
        n.y = y + ofy;
        if let Some((tx, ty)) = pin_pos {
            // 锚定后再叠加 `offset`：Compose 的 `offset` 是「落位结果的位移」，
            // 与位移来源（父链排布 / 锚点绝对位置 / 全局坐标）无关，漏加即成静默 bug。
            let mut abs_x = tx + ofx;
            let mut abs_y = ty + ofy;
            // 浮层智能重定位：面板若伸出窗口左/右边界（如靠右的 TabRow 溢出「⋮」菜单
            // 按内容自然宽展开），则整体平移使其完整落在视口内（留出 4px 边距），
            // 不被 OS 窗口裁切。面板比视口还宽 / 视口未知时保持原位。
            let (vw, vh) = viewport_size();
            let pw = n.width;
            let ph = n.height;
            if vw > 0.0 && pw > 0.0 && pw < vw {
                abs_x = abs_x.min(vw - 4.0 - pw).max(4.0);
            }
            // `pin_at_global` 的坐标来自指针事件，可能贴近视口底边（底部标签条的「▼」），
            // 纵向同样夹取；`pin_below` 的纵向溢出仍交由滚动封顶处理。
            if is_pin_at && vh > 0.0 && ph > 0.0 && ph < vh {
                abs_y = abs_y.min(vh - 4.0 - ph).max(4.0);
            }
            n.x = abs_x - ppx;
            n.y = abs_y - ppy;
        }
    }
    let (cw, ch) = (rt.nodes[id.index()].width, rt.nodes[id.index()].height);
    let policy_kind = policy_kind_of(rt, id);
    match policy_kind {
        PolicyKind::Box => {
            let default_align = match &rt.nodes[id.index()].policy {
                Policy::Box(a) => *a,
                _ => Alignment::TopStart,
            };
            // 对齐在 padding 之后的内容区内进行（节点尺寸含 padding，先扣除）
            let (pl, pt, pr, pb) = rt.nodes[id.index()].modifier.padding_of();
            let children = rt.nodes[id.index()].children.clone();
            for chid in children {
                let mods = rt.nodes[chid.index()].modifier.clone();
                let align = mods
                    .elems
                    .iter()
                    .find_map(|e| match e {
                        ModElem::Align(a) => Some(*a),
                        _ => None,
                    })
                    .unwrap_or(default_align);
                let (w, h) = (rt.nodes[chid.index()].width, rt.nodes[chid.index()].height);
                let (ox, oy) = align.align(cw - pl - pr, ch - pt - pb, w, h);
                place_node(rt, chid, ox + pl, oy + pt);
            }
        }
        PolicyKind::Row | PolicyKind::Col => {
            let is_row = policy_kind == PolicyKind::Row;
            let (arrangement, cross) = match &rt.nodes[id.index()].policy {
                Policy::Row { arrangement, cross } => (*arrangement, *cross),
                Policy::Column { arrangement, cross } => (*arrangement, *cross),
                _ => (Arrangement::Start, CrossAxisAlignment::Start),
            };
            let children = rt.nodes[id.index()].children.clone();
            // 主轴 / 交叉轴居中均在 padding 之后的内容区内进行（节点尺寸含 padding，先扣除）
            let (cpl, cpt, cpr, cpb) = rt.nodes[id.index()].modifier.padding_of();
            let main_size = (if is_row { cw } else { ch }) - cpl - if is_row { cpr } else { cpb };
            // 交叉可用尺寸扣除滚动条 gutter，使 Center/End 对齐的内容不落入滚动条区域。
            let reserve = scrollbar_gutter_for(rt, id, is_row);
            let cross_size =
                (if is_row { ch } else { cw }) - if is_row { cpt + cpb } else { cpl + cpr } - reserve;
            // 收集主轴尺寸（用单元格尺寸：`weight(w, false)` 的孩子自身窄于份额）
            let sizes: Vec<f32> = children
                .iter()
                .map(|cid| {
                    let n = &rt.nodes[cid.index()];
                    let cell = if n.cell == (0.0, 0.0) {
                        (n.width, n.height)
                    } else {
                        n.cell
                    };
                    if is_row { cell.0 } else { cell.1 }
                })
                .collect();
            let used: f32 = sizes.iter().sum();
            let free = (main_size - used).max(0.0);
            let n = children.len() as f32;
            let (start_off, gap) = match arrangement {
                Arrangement::Start => (0.0, 0.0),
                Arrangement::Center => (free * 0.5, 0.0),
                Arrangement::End => (free, 0.0),
                Arrangement::SpaceBetween => {
                    if n > 1.0 {
                        (0.0, free / (n - 1.0))
                    } else {
                        (free * 0.5, 0.0)
                    }
                }
                Arrangement::SpaceAround => (free / (2.0 * n), free / n),
                Arrangement::SpaceEvenly => (free / (n + 1.0), free / (n + 1.0)),
            };
            let mut main = start_off;
            for (i, cid) in children.iter().enumerate() {
                let cell = {
                    let cn = &rt.nodes[cid.index()];
                    if cn.cell == (0.0, 0.0) {
                        (cn.width, cn.height)
                    } else {
                        cn.cell
                    }
                };
                let (child_main, child_cross) = if is_row {
                    (cell.0, cell.1)
                } else {
                    (cell.1, cell.0)
                };
                let cross_off = match cross {
                    CrossAxisAlignment::Start => 0.0,
                    CrossAxisAlignment::Center => (cross_size - child_cross) * 0.5,
                    CrossAxisAlignment::End => cross_size - child_cross,
                    CrossAxisAlignment::Stretch => 0.0, // stretch 已在测量收紧
                };
                // 孩子落入本节点 padding 之后的内容区
                let (cpl, cpt, _, _) = rt.nodes[id.index()].modifier.padding_of();
                let (px, py) = if is_row {
                    (main + cpl, cross_off + cpt)
                } else {
                    (cross_off + cpl, main + cpt)
                };
                place_node(rt, *cid, px, py);
                main += child_main + gap;
                let _ = i;
            }
        }
        PolicyKind::Leaf => {}
        PolicyKind::Custom => {
            // Custom 的落位已写入孩子 x/y，这里递归即可
            let children = rt.nodes[id.index()].children.clone();
            for chid in children {
                let (cx, cy) = {
                    let cn = &rt.nodes[chid.index()];
                    (cn.x, cn.y)
                };
                place_node(rt, chid, cx, cy);
            }
        }
    }
    let _ = ch;
    // 几何变化检测：本节点落位后若 x/y/w/h 与上一帧不同（亚像素容差，规避浮点噪声导致永久脏），
    // 则其后代绝对坐标随之变化，必须重绘（含后代）；同时标记自身保持脏（自身重绘），
    // 祖先标记保证下一帧遍历能抵达。
    let (nx, ny, nw, nh) = {
        let n = &rt.nodes[id.index()];
        (n.x, n.y, n.width, n.height)
    };
    let changed = match rt.nodes[id.index()].last_geo {
        Some((ox, oy, ow, oh)) => {
            (nx - ox).abs() > 0.01
                || (ny - oy).abs() > 0.01
                || (nw - ow).abs() > 0.01
                || (nh - oh).abs() > 0.01
        }
        None => true,
    };
    rt.nodes[id.index()].last_geo = Some((nx, ny, nw, nh));
    if changed {
        // 几何变化：后代绝对坐标随之变化，标记自身+子树+祖先脏（下一帧遍历能抵达、后代不陈旧）。
        rt.nodes[id.index()].dirty = true;
        mark_dirty_subtree(rt, id);
        mark_dirty_path(rt, id);
    } else {
        // 几何稳定：清除脏标记，下一帧 measure/place 可直接跳过本子树（绘制闭包仍每帧执行）。
        // 仅因重组/滚动而脏、但几何未变的节点在此被清脏，避免永久全量布局。
        rt.nodes[id.index()].dirty = false;
    }
    // 滚动容器内容尺寸：落位后结算（测量期算不准 —— 孩子位置此刻才定）。
    // 挂在落位递归末尾 ⇒ 成本随「本帧真正走到的子树」而非 arena 规模；
    // 早返回的干净子树孩子几何未变，内容尺寸必然不变，跳过是安全的。
    update_scroll_content(rt, id);
}

#[derive(PartialEq)]
enum PolicyKind {
    Box,
    Row,
    Col,
    Leaf,
    Custom,
}

fn policy_kind_of(rt: &Rt, id: NodeId) -> PolicyKind {
    match &rt.nodes[id.index()].policy {
        Policy::Box(_) => PolicyKind::Box,
        Policy::Row { .. } => PolicyKind::Row,
        Policy::Column { .. } => PolicyKind::Col,
        Policy::Leaf => PolicyKind::Leaf,
        Policy::Custom(_) => PolicyKind::Custom,
    }
}

// ══════════════════════════════ 绘制 ══════════════════════════════

/// 绘制命令 — 平台无关（valo 转换层在 compose-desktop）。
///
/// `Hash`（按位，见 [`crate::foundation::hash_f32`]）供渲染层做内容寻址的静态/动态分区
/// （见 `crate::render::kernel`）。
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    FillRRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    },
    /// 线性渐变填充矩形（`vertical` = 从上到下，否则从左到右）。
    FillRectGradient {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        from: Color,
        to: Color,
        vertical: bool,
    },
    /// 线性渐变填充圆角矩形（页头 / 英雄卡 / 图表面积）。
    FillRRectGradient {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        from: Color,
        to: Color,
        vertical: bool,
    },
    StrokeRRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
        line: f32,
    },
    FillCircle {
        cx: f32,
        cy: f32,
        r: f32,
        color: Color,
    },
    StrokeCircle {
        cx: f32,
        cy: f32,
        r: f32,
        color: Color,
        line: f32,
    },
    Line {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: Color,
        line: f32,
    },
    /// 闭合多边形填充（饼图 / 雷达图 / 面积图 / 三角箭头）。
    FillPath {
        points: Vec<(f32, f32)>,
        color: Color,
    },
    /// 折线路径描边（曲线图 / 多边形轮廓）。
    StrokePath {
        points: Vec<(f32, f32)>,
        color: Color,
        line: f32,
    },
    /// 液态玻璃面板：背景高斯模糊 + 半透明底色 + 边缘高光 + 描边。
    GlassPanel {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        tint: Color,
        blur: f32,
        highlight: Color,
        outline: Color,
        line: f32,
    },
    /// 文本：由渲染层经 Paragraph 排版（baseline 顶对齐绘制）。
    Text {
        x: f32,
        y: f32,
        text: String,
        style: TextStyle,
        max_w: f32,
    },
    ClipPush {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
    },
    ClipPop,
    /// 位图：按 `image_id` 从图像注册表取得 `valo::Image` 绘制；`token` 随解码就绪翻转，
    /// 使保留模式静态块缓存失效并被重绘（避免解码完成前缓存的占位被永久复用）。
    /// `opacity` 进内容指纹（同图不同透明度视为不同墨迹）。
    Image {
        image_id: u64,
        token: u64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        opacity: f32,
    },
}

/// 文本样式。
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub family: String,
    /// 行高（绝对像素）。`None` = 用字体自带行距（各字体不一致 → 混排时纵向节奏会乱）。
    /// M3 排版阶梯每个角色都带明确行高，见 `compose_material::tokens`。
    pub line_height: Option<f32>,
    /// 字距（逻辑像素，加在每个字簇之后）。
    pub letter_spacing: f32,
}

use crate::foundation::hash_f32;

impl std::hash::Hash for TextStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        hash_f32(state, self.size);
        self.weight.hash(state);
        self.italic.hash(state);
        self.family.hash(state);
        // 行高/字距改变即换墨迹，必须进内容键（否则保留层会回放旧行高的段落）
        match self.line_height {
            Some(px) => {
                state.write_u8(1);
                hash_f32(state, px);
            }
            None => state.write_u8(0),
        }
        hash_f32(state, self.letter_spacing);
    }
}

impl std::hash::Hash for DrawCmd {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        use DrawCmd as C;
        match self {
            C::FillRect { x, y, w, h, color } => {
                state.write_u8(0);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                color.hash(state);
            }
            C::FillRRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                state.write_u8(1);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                hash_f32(state, *radius);
                color.hash(state);
            }
            C::FillRectGradient {
                x,
                y,
                w,
                h,
                from,
                to,
                vertical,
            } => {
                state.write_u8(20);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                from.hash(state);
                to.hash(state);
                state.write_u8(*vertical as u8);
            }
            C::FillRRectGradient {
                x,
                y,
                w,
                h,
                radius,
                from,
                to,
                vertical,
            } => {
                state.write_u8(21);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                hash_f32(state, *radius);
                from.hash(state);
                to.hash(state);
                state.write_u8(*vertical as u8);
            }
            C::StrokeRRect {
                x,
                y,
                w,
                h,
                radius,
                color,
                line,
            } => {
                state.write_u8(2);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                hash_f32(state, *radius);
                color.hash(state);
                hash_f32(state, *line);
            }
            C::FillCircle { cx, cy, r, color } => {
                state.write_u8(3);
                hash_f32(state, *cx);
                hash_f32(state, *cy);
                hash_f32(state, *r);
                color.hash(state);
            }
            C::StrokeCircle {
                cx,
                cy,
                r,
                color,
                line,
            } => {
                state.write_u8(4);
                hash_f32(state, *cx);
                hash_f32(state, *cy);
                hash_f32(state, *r);
                color.hash(state);
                hash_f32(state, *line);
            }
            C::Line {
                x0,
                y0,
                x1,
                y1,
                color,
                line,
            } => {
                state.write_u8(5);
                hash_f32(state, *x0);
                hash_f32(state, *y0);
                hash_f32(state, *x1);
                hash_f32(state, *y1);
                color.hash(state);
                hash_f32(state, *line);
            }
            C::FillPath { points, color } => {
                state.write_u8(6);
                hash_path(state, points);
                color.hash(state);
            }
            C::StrokePath {
                points,
                color,
                line,
            } => {
                state.write_u8(7);
                hash_path(state, points);
                color.hash(state);
                hash_f32(state, *line);
            }
            C::GlassPanel {
                x,
                y,
                w,
                h,
                radius,
                tint,
                blur,
                highlight,
                outline,
                line,
            } => {
                state.write_u8(8);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                hash_f32(state, *radius);
                tint.hash(state);
                hash_f32(state, *blur);
                highlight.hash(state);
                outline.hash(state);
                hash_f32(state, *line);
            }
            C::Text {
                x,
                y,
                text,
                style,
                max_w,
            } => {
                state.write_u8(9);
                hash_f32(state, *x);
                hash_f32(state, *y);
                text.hash(state);
                style.hash(state);
                hash_f32(state, *max_w);
            }
            C::ClipPush { x, y, w, h, radius } => {
                state.write_u8(10);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                hash_f32(state, *radius);
            }
            C::ClipPop => state.write_u8(11),
            C::Image {
                image_id,
                token,
                x,
                y,
                w,
                h,
                opacity,
            } => {
                state.write_u8(12);
                // u64 字段走精确写入（f32 精度不足以容纳 id / token）。
                state.write_u64(*image_id);
                state.write_u64(*token);
                hash_f32(state, *x);
                hash_f32(state, *y);
                hash_f32(state, *w);
                hash_f32(state, *h);
                hash_f32(state, *opacity);
            }
        }
    }
}

#[inline]
fn hash_path<H: std::hash::Hasher>(state: &mut H, points: &[(f32, f32)]) {
    std::hash::Hash::hash(&points.len(), state);
    for (x, y) in points {
        hash_f32(state, *x);
        hash_f32(state, *y);
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            size: 14.0,
            weight: 400,
            italic: false,
            family: String::new(),
            line_height: None,
            letter_spacing: 0.0,
        }
    }
}

impl TextStyle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = w;
        self
    }
    pub fn italic(mut self, on: bool) -> Self {
        self.italic = on;
        self
    }
    pub fn family(mut self, f: &str) -> Self {
        self.family = f.to_string();
        self
    }
    /// 行高（绝对像素）。
    pub fn line_height(mut self, px: f32) -> Self {
        self.line_height = Some(px);
        self
    }
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.letter_spacing = px;
        self
    }
}

/// 绘制命令构建器（Canvas 内容闭包的 DrawScope）。
pub struct DrawBuilder {
    pub(crate) cmds: Vec<DrawCmd>,
    /// probe 模式：仅统计期望尺寸（Leaf intrinsic 测量）。
    pub(crate) probe: bool,
    pub(crate) probe_w: f32,
    pub(crate) probe_h: f32,
    /// probe 时父级给的最大宽度：文本按它折行，测出的才是**多行**高度
    /// （否则 `Text` 永远按单行上报，卡片/列表里的正文会横着溢出）。
    pub(crate) probe_max_w: f32,
}

impl DrawBuilder {
    /// 直接写入既有命令缓冲（绘制遍历用：免掉「先建小 Vec、再逐条转入图层」的分配与复制）。
    pub(crate) fn with_buffer(cmds: Vec<DrawCmd>) -> Self {
        Self {
            cmds,
            probe: false,
            probe_w: 0.0,
            probe_h: 0.0,
            probe_max_w: f32::INFINITY,
        }
    }
    pub(crate) fn probe() -> Self {
        Self {
            cmds: Vec::new(),
            probe: true,
            probe_w: 0.0,
            probe_h: 0.0,
            probe_max_w: f32::INFINITY,
        }
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if !self.probe {
            self.cmds.push(DrawCmd::FillRect { x, y, w, h, color });
        }
        self.grow(x + w, y + h);
    }
    /// 位图：emit `DrawCmd::Image`（按 `image_id` 在渲染层解析纹理）。`token` 进内容指纹，
    /// 解码就绪翻转后使保留模式静态块失效并被重绘。
    pub fn image(&mut self, image_id: u64, token: u64, x: f32, y: f32, w: f32, h: f32, opacity: f32) {
        if !self.probe {
            self.cmds.push(DrawCmd::Image {
                image_id,
                token,
                x,
                y,
                w,
                h,
                opacity,
            });
        }
        self.grow(x + w, y + h);
    }
    pub fn fill_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        if !self.probe {
            self.cmds.push(DrawCmd::FillRRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            });
        }
        self.grow(x + w, y + h);
    }
    /// 线性渐变矩形（`vertical` = 上→下）。
    pub fn fill_rect_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        from: Color,
        to: Color,
        vertical: bool,
    ) {
        if !self.probe {
            self.cmds.push(DrawCmd::FillRectGradient {
                x,
                y,
                w,
                h,
                from,
                to,
                vertical,
            });
        }
        self.grow(x + w, y + h);
    }
    /// 线性渐变圆角矩形。
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rrect_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        from: Color,
        to: Color,
        vertical: bool,
    ) {
        if !self.probe {
            self.cmds.push(DrawCmd::FillRRectGradient {
                x,
                y,
                w,
                h,
                radius,
                from,
                to,
                vertical,
            });
        }
        self.grow(x + w, y + h);
    }
    pub fn stroke_rrect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
        line: f32,
    ) {
        if !self.probe {
            self.cmds.push(DrawCmd::StrokeRRect {
                x,
                y,
                w,
                h,
                radius,
                color,
                line,
            });
        }
        self.grow(x + w, y + h);
    }
    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        if !self.probe {
            self.cmds.push(DrawCmd::FillCircle { cx, cy, r, color });
        }
        self.grow(cx + r, cy + r);
    }
    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, color: Color, line: f32) {
        if !self.probe {
            self.cmds.push(DrawCmd::StrokeCircle {
                cx,
                cy,
                r,
                color,
                line,
            });
        }
        self.grow(cx + r, cy + r);
    }
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color, line: f32) {
        if !self.probe {
            self.cmds.push(DrawCmd::Line {
                x0,
                y0,
                x1,
                y1,
                color,
                line,
            });
        }
        self.grow(x1, y1);
    }

    /// 闭合多边形填充（自动闭合首尾）。
    pub fn fill_path(&mut self, points: &[(f32, f32)], color: Color) {
        if points.len() < 2 {
            return;
        }
        if !self.probe {
            self.cmds.push(DrawCmd::FillPath {
                points: points.to_vec(),
                color,
            });
        }
        for (px, py) in points {
            self.grow(*px, *py);
        }
    }

    /// 折线路径描边。
    pub fn stroke_path(&mut self, points: &[(f32, f32)], color: Color, line: f32) {
        if points.len() < 2 {
            return;
        }
        if !self.probe {
            self.cmds.push(DrawCmd::StrokePath {
                points: points.to_vec(),
                color,
                line,
            });
        }
        for (px, py) in points {
            self.grow(*px, *py);
        }
    }
    pub fn draw_text(&mut self, x: f32, y: f32, text: &str, style: &TextStyle, max_w: f32) {
        if !self.probe {
            self.cmds.push(DrawCmd::Text {
                x,
                y,
                text: text.to_string(),
                style: style.clone(),
                max_w,
            });
        } else {
            // probe 阶段闭包拿不到真实宽度（传 0），按父级给的可用宽度折行测量。
            let mw = if max_w > 0.0 { max_w.min(self.probe_max_w) } else { self.probe_max_w };
            let (w, h) = measure_text_at(text, style, mw);
            self.grow(x + w, y + h);
        }
    }

    fn grow(&mut self, w: f32, h: f32) {
        self.probe_w = self.probe_w.max(w);
        self.probe_h = self.probe_h.max(h);
    }
}

/// 文本探测度量：按可用宽度折行，返回 (折行后宽, 多行高)。
pub(crate) fn measure_text_at(text: &str, style: &TextStyle, max_w: f32) -> (f32, f32) {
    TEXT_MEASURER.with(|cell| {
        let b = cell.borrow();
        match b.as_ref() {
            Some(m) => m.measure_at(text, style, max_w),
            None => heuristic_text_at(text, style, max_w),
        }
    })
}

/// 无字体/无度量器时的近似度量：宽度按字符数估，超出 `max_w` 则按行叠高。
pub fn heuristic_text_at(text: &str, style: &TextStyle, max_w: f32) -> (f32, f32) {
    let w = text.chars().count() as f32 * (style.size * 0.55 + style.letter_spacing);
    let h = style.line_height.unwrap_or(style.size * 1.4);
    if max_w.is_finite() && max_w > 0.0 && w > max_w {
        (max_w, h * (w / max_w).ceil())
    } else {
        (w, h)
    }
}

thread_local! {
    static TEXT_MEASURER: RefCell<Option<Box<dyn TextMeasurer>>> = const { RefCell::new(None) };
}

/// 文本度量器 trait — 平台后端注入（valo Paragraph 真实度量）。
pub trait TextMeasurer {
    fn measure(&self, text: &str, style: &TextStyle) -> (f32, f32);

    /// 按 `max_w` 折行测量。默认退回单行度量，后端有段落排版时覆写。
    fn measure_at(&self, text: &str, style: &TextStyle, max_w: f32) -> (f32, f32) {
        let _ = max_w;
        self.measure(text, style)
    }
}

/// 注入全局文本度量器（平台后端在每帧布局前调用）。
pub fn set_text_measurer(m: Box<dyn TextMeasurer>) {
    TEXT_MEASURER.with(|cell| *cell.borrow_mut() = Some(m));
}

/// 带度量器测量（文本布局的真实路径）。
pub fn measure_text_with_measurer(text: &str, style: &TextStyle) -> (f32, f32) {
    measure_text_at(text, style, f32::INFINITY)
}

/// 释放节点子树：销毁其名下作用域（disposables / 依赖解除）。
pub(crate) fn dispose_subtree(rt: &mut Rt, id: NodeId) {
    let children = rt.nodes[id.index()].children.clone();
    for ch in children {
        dispose_subtree(rt, ch);
    }
    if let Some(sid) = rt.nodes[id.index()].owner_scope {
        crate::runtime::dispose_scope_rt(rt, sid);
    }
}

/// 一段命令的内容指纹（含段长度，避免相邻段拼接后的歧义）。
fn segment_hash(cmds: &[DrawCmd]) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&cmds.len(), &mut h);
    for c in cmds {
        c.hash(&mut h);
    }
    h.finish()
}

/// 保留模式的跨帧状态，由平台内核持有（`Arc`/`Box` 皆可，逐帧可变借用传入遍历）。
#[derive(Default)]
pub struct RetainedState {
    /// 上一帧出现过的图层段指纹：本帧同指纹的段即「内容未变」→ 可进静态块。
    /// （私有：外部只通过遍历入口使用，避免把内部容器类型暴露成 API。）
    last_runs: FingerprintSet,
    /// 上一帧静态块序列的签名（按序混合各块键 + 块数）。
    sig: u64,
    /// 段静态判定 / 切块的临时缓冲：动画期每帧都要重算，逐帧复用容量（否则每帧
    /// 一次 `vec![false; n]` + 一次 `Vec<Blk>` 分配，节点越多代价越大）。
    is_static: Vec<bool>,
    blks: Vec<Blk>,
}

/// 内容指纹集合：键本身就是内容哈希 / 自增 id，分布已均匀，默认 SipHash13 的
/// 抗碰撞强度是多余的。保留模式每帧按段一次 `contains` + 一次 `insert`
/// （story 动画页 ~900 段），换成 splitmix64 收尾后这条热路径只剩纯算术。
type FingerprintSet = std::collections::HashSet<u64, FingerprintHashBuilder>;

#[derive(Default)]
struct FingerprintHasher(u64);

impl std::hash::Hasher for FingerprintHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut z = self.0 ^ 0x9E37_79B9_7F4A_7C15;
        for &b in bytes {
            z = (z ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            z ^= z >> 32;
        }
        self.0 = z;
    }
    fn write_u64(&mut self, n: u64) {
        let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        self.0 = z ^ (z >> 31);
    }
    fn write_u32(&mut self, n: u32) {
        self.write_u64(n as u64);
    }
    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }
}

#[derive(Default)]
struct FingerprintHashBuilder;

impl std::hash::BuildHasher for FingerprintHashBuilder {
    type Hasher = FingerprintHasher;
    fn build_hasher(&self) -> FingerprintHasher {
        FingerprintHasher(0)
    }
}

/// 静态/现场切块计划的一个临时块（仅遍历期使用，见 [`RetainedState::blks`]）。
struct Blk {
    from: usize,
    to: usize,
    key: u64,
    is_static: bool,
}

/// 图层段属于节点的哪一半：前缀在孩子之前、后缀在孩子之后。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SegPart {
    Prefix,
    Suffix,
}

/// 一帧里的一个**原子段**句柄：命令仍留在节点的图层缓存里，未变的部分不被复制。
///
/// 一个节点段（前缀 / 后缀）会按「裁剪指令」再切成若干原子段：`ClipPush`/`ClipPop`
/// 各自成一段（[`LayerSeg::clip`]），其余墨迹连续成段。裁剪段永远进不了静态块，
/// 静态块因此永远是自平衡的（不含任何裁剪进出），可以在裁剪区内任意切。
pub struct LayerSeg {
    pub node: u32,
    pub part: SegPart,
    /// 该段在图层 `prefix`/`suffix` 里的命令区间。
    pub start: u32,
    pub len: u32,
    pub hash: u64,
    /// 本原子段是裁剪指令（不可缓存，见 [`Chunk::Live`]）。
    pub clip: bool,
}

/// 帧构建计划里的一块（chunk）。块序即 z 序，平台按序生成一条 DisplayList。
#[derive(Clone, Copy, Debug)]
pub enum Chunk {
    /// `[from..to)` 号原子段，内容逐命令等于上一帧见过的那段墨。`key` 相同 ⇒ 同内容，
    /// 平台可复用为同键录好的 DisplayList；命令本体仍躺在各节点的图层缓存里。
    ///
    /// 块内不含任何裁剪指令，故录出来的 DisplayList 自身平衡，嵌回原位置时继承外层裁剪。
    Static { key: u64, from: usize, to: usize },
    /// `len` 条命令（接在 `dynamic_cmds` 的 `start` 处），每帧现场录制。
    ///
    /// 裁剪指令（`ClipPush`/`ClipPop`）永远落在这里：它们改的是外层构建器的裁剪栈，
    /// 藏进可复用块里就等于让块后的兄弟脱离这层裁剪。
    Live { start: usize, len: usize },
}

/// 保留模式一轮遍历的产物。
///
/// 对齐 Compose 的 `LayerState`：内容未变的层不重新生成显示操作。旧实现把整棵子树的
/// 命令逐帧展平成一条 `Vec<DrawCmd>`（动画页 893 条，含文本/路径的堆分配）再分区，
/// 现在只有现场块会被物化（实测 28 条），静态段始终以句柄记录、按需才取回本体。
#[derive(Default)]
pub struct FrameDraw {
    /// 现场块的命令，按流序物化。
    pub dynamic_cmds: Vec<DrawCmd>,
    /// 静态块集合相对上一帧是否改变。
    pub static_changed: bool,
    /// 本帧构建计划：静态块与现场块交替，**顺序即真实 z 序**。
    pub plan: Vec<Chunk>,
    /// 本帧命令总数（静态 + 动态），供诊断用，不代表已物化。
    pub cmd_count: usize,
    /// 本帧各段的内容指纹与图层句柄（分区与取本体都只读它）。
    segments: Vec<LayerSeg>,
    next_runs: FingerprintSet,
}

impl FrameDraw {
    /// 清空本帧产物但**保留缓冲容量**：内核跨帧持有同一个 `FrameDraw`，
    /// 动画期逐帧重建计划时不再重新分配 `segments` / `plan` / `dynamic_cmds`。
    pub fn clear(&mut self) {
        self.dynamic_cmds.clear();
        self.plan.clear();
        self.segments.clear();
        self.cmd_count = 0;
        self.static_changed = false;
    }

    /// 静态块的命令本体：按段句柄从节点图层缓存取回。
    ///
    /// 只有平台侧该键的 DisplayList 缓存缺失时才会走到这里（动画期一般命中缓存，
    /// 于是整帧的命令复制只剩现场块那几十条）。
    pub fn static_chunk_cmds(&self, from: usize, to: usize) -> Vec<DrawCmd> {
        let mut out = Vec::new();
        crate::runtime::with_rt_pub(|rt| {
            for seg in &self.segments[from..to] {
                if let Some(cmds) = layer_part(rt, seg) {
                    out.extend_from_slice(cmds);
                }
            }
        });
        out
    }
}

impl FrameDraw {
    /// 非保留模式的等价物：整条命令流都按「本帧要画」处理，没有静态层。
    pub fn flat(cmds: Vec<DrawCmd>) -> Self {
        let n = cmds.len();
        FrameDraw {
            dynamic_cmds: cmds,
            cmd_count: n,
            ..FrameDraw::default()
        }
    }

    /// 记录本节点的一段：按裁剪指令切成原子段，只留句柄、不复制命令。
    ///
    /// 不含裁剪指令的段（绝大多数）整段就是一个原子，直接复用调用方算好的 `hash`，
    /// 零额外哈希；只有裁剪节点的前缀 / 后缀才会被切开并逐墨迹 run 重新哈希。
    fn record(&mut self, node: u32, part: SegPart, cmds: &[DrawCmd], hash: u64) {
        if cmds.is_empty() {
            // 空段（多数容器既无背景也无边框）不进记账：否则每个空段都携同一个哈希，
            // 静态键会被无关的节点增删扰动。
            return;
        }
        self.cmd_count += cmds.len();
        if !cmds.iter().any(is_clip) {
            self.push_seg(node, part, 0, cmds.len(), hash, false);
            return;
        }
        let mut i = 0usize;
        while i < cmds.len() {
            let clip = is_clip(&cmds[i]);
            let end = i + cmds[i..].iter().position(|c| is_clip(c) != clip).unwrap_or(cmds.len() - i);
            let run = &cmds[i..end];
            self.push_seg(node, part, i, run.len(), if clip { 0 } else { segment_hash(run) }, clip);
            i = end;
        }
    }

    fn push_seg(&mut self, node: u32, part: SegPart, start: usize, len: usize, hash: u64, clip: bool) {
        if !clip {
            self.next_runs.insert(hash);
        }
        self.segments.push(LayerSeg {
            node,
            part,
            start: start as u32,
            len: len as u32,
            hash,
            clip,
        });
    }
}

/// 绘制输出的落点：命令要么全部物化成一条流（平铺），要么按段记账（保留）。
pub enum Out<'a> {
    Flat {
        cmds: &'a mut Vec<DrawCmd>,
        owners: &'a mut Vec<u32>,
    },
    Retained { f: &'a mut FrameDraw },
}

impl Out<'_> {
    /// 把本节点的一段命令接到输出流上。
    fn emit(&mut self, node: u32, part: SegPart, cmds: &[DrawCmd], hash: u64) {
        match self {
            Out::Flat { cmds: o, owners } => {
                o.extend_from_slice(cmds);
                owners.resize(o.len(), node);
            }
            Out::Retained { f } => f.record(node, part, cmds, hash),
        }
    }
}

/// 绘制整棵树 → 平台无关 DrawCmd 序列（绝对坐标）。供测试与非保留模式使用。
///
/// 不动增量脏区的线程局部标志：它只是一次纯读取式遍历。
pub fn draw_tree(root: NodeId) -> Vec<DrawCmd> {
    let mut cmds = Vec::new();
    let mut owners = Vec::new();
    crate::runtime::with_rt_pub(|rt| {
        let mut out = Out::Flat {
            cmds: &mut cmds,
            owners: &mut owners,
        };
        draw_node(rt, root, 0.0, 0.0, &mut out);
    });
    cmds
}

/// 绘制整棵树，并返回每条 DrawCmd 的归属节点索引（诊断与非保留模式的等价物化路径）。
pub fn draw_tree_tagged(root: NodeId) -> (Vec<DrawCmd>, Vec<u32>) {
    SCENE_CHANGED.with(|s| s.set(false));
    let mut cmds = Vec::new();
    let mut owners = Vec::new();
    crate::runtime::with_rt_pub(|rt| {
        let mut out = Out::Flat {
            cmds: &mut cmds,
            owners: &mut owners,
        };
        draw_node(rt, root, 0.0, 0.0, &mut out);
    });
    walk_consumed();
    (cmds, owners)
}

/// 段句柄 → 图层里的命令本体（遍历结束时图层缓存已是本帧内容，故按句柄取即可）。
fn layer_part<'r>(rt: &'r Rt, seg: &LayerSeg) -> Option<&'r [DrawCmd]> {
    rt.nodes
        .get(seg.node as usize)
        .and_then(|n| n.draw_cache.as_ref())
        .and_then(|l| {
            let all: &[DrawCmd] = match seg.part {
                SegPart::Prefix => l.prefix.as_slice(),
                SegPart::Suffix => l.suffix.as_slice(),
            };
            let from = seg.start as usize;
            all.get(from..from + seg.len as usize)
        })
}

/// 该命令是否改变裁剪栈（决定它能否进可复用的静态块）。
fn is_clip(c: &DrawCmd) -> bool {
    matches!(c, DrawCmd::ClipPush { .. } | DrawCmd::ClipPop)
}

/// 保留模式遍历：产出「块计划 + 静态块命令（按需）+ 现场命令」。
///
/// 原子段的内容指纹缓存在节点图层里，故本帧不重新哈希未变的墨迹；静态块的键只混合
/// 静态段，动态段增删不会牵动它（story 动画页实测：静态块可连续多帧复用同一份 DisplayList）。
///
/// 一条硬约束：**裁剪指令不进静态块**（`ClipPush`/`ClipPop` 永远现场录制）。
/// 静态块录成 DisplayList 后是被 `draw_display_list` **嵌回原位置**的，valo 在嵌入点
/// 继承外层裁剪栈，所以块自身只要平衡就等价——而块内不含裁剪进出即天然平衡。
/// 反过来，一旦把某父节点的 `ClipPush` 关进缓存块，外层构建器的栈就少这一层，
/// 块后的兄弟（下一帧仍是静态或现场）全部脱离裁剪。把裁剪指令单独成段还换来一件事：
/// 裁剪区里的静态墨迹可以各自成块，不必像旧的 `depth==0` 规则那样整棵裁剪子树并成
/// 一个巨型现场块（动画页实测：现场命令 890 → 数十条）。
pub fn draw_tree_retained(
    root: NodeId,
    state: &mut RetainedState,
    force_static: bool,
) -> FrameDraw {
    let mut f = FrameDraw::default();
    draw_tree_retained_into(root, state, &mut f, force_static);
    f
}

/// 保留模式遍历的复用入口：把本帧产物写进调用方持有的 `FrameDraw`（清空但保留容量）。
///
/// 平台内核逐帧复用同一个 `FrameDraw` —— 动画期这是每帧一次的热路径，
/// `with_capacity` 与全新 `HashSet` 的扩容/重哈希都不该每帧发生。
pub fn draw_tree_retained_into(
    root: NodeId,
    state: &mut RetainedState,
    f: &mut FrameDraw,
    force_static: bool,
) {
    SCENE_CHANGED.with(|s| s.set(false));
    f.clear();
    let mut changed = false;
    crate::runtime::with_rt_pub(|rt| {
        {
            let mut out = Out::Retained { f: &mut *f };
            draw_node(rt, root, 0.0, 0.0, &mut out);
        }
        // ① 逐原子段判定：内容指纹在上一帧出现过（且本体还取得回来）才算静态。
        //    段可以只覆盖节点前缀的一部分，故不再要求「整节点同生共死」。
        let nseg = f.segments.len();
        let mut is_static = std::mem::take(&mut state.is_static);
        is_static.clear();
        is_static.resize(nseg, false);
        for (i, seg) in f.segments.iter().enumerate() {
            is_static[i] =
                !seg.clip && state.last_runs.contains(&seg.hash) && layer_part(rt, seg).is_some();
        }
        // ② 切块：静态性一变即断块，无其它限制。
        let mut blks = std::mem::take(&mut state.blks);
        blks.clear();
        let mut start = 0usize;
        let mut cur_static = is_static.first().copied().unwrap_or(false);
        let mut key = 0u64;
        for i in 0..nseg {
            if i > start && cur_static != is_static[i] {
                blks.push(Blk { from: start, to: i, key, is_static: cur_static });
                (start, cur_static, key) = (i, is_static[i], 0);
            }
            if cur_static {
                key = key.wrapping_mul(31).wrapping_add(f.segments[i].hash);
            }
        }
        if nseg > start {
            blks.push(Blk { from: start, to: nseg, key, is_static: cur_static });
        }
        // ③ 现场块物化；静态块只留段区间句柄，平台侧块缓存缺失时才按 `static_chunk_cmds` 取本体。
        for blk in blks.drain(..) {
            if blk.is_static {
                f.plan.push(Chunk::Static { key: blk.key, from: blk.from, to: blk.to });
            } else {
                let at = f.dynamic_cmds.len();
                for seg in &f.segments[blk.from..blk.to] {
                    if let Some(cmds) = layer_part(rt, seg) {
                        f.dynamic_cmds.extend_from_slice(cmds);
                    }
                }
                f.plan.push(Chunk::Live { start: at, len: f.dynamic_cmds.len() - at });
            }
        }
        // ④ 静态块序列签名（顺序混合各块键 + 块数）；现场块的增删不进签名。
        let mut sig_acc = 0u64;
        let mut static_blks = 0usize;
        for chunk in &f.plan {
            if let Chunk::Static { key, .. } = *chunk {
                sig_acc = sig_acc.wrapping_mul(31).wrapping_add(key);
                static_blks += 1;
            }
        }
        let sig = sig_acc ^ (static_blks as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        changed = force_static || sig != state.sig;
        state.sig = sig;
        // 段指纹集合双缓冲交换：本帧的进 `last_runs`，上一帧的清空后留给下一帧复用
        // （`clear` 保留哈希桶容量，动画期不再每帧重新扩容 + 重哈希几百条段）。
        std::mem::swap(&mut state.last_runs, &mut f.next_runs);
        f.next_runs.clear();
        state.is_static = is_static;
        state.blks = blks;
    });
    f.static_changed = changed;
    walk_consumed();
}

thread_local! {
    /// 本轮 `draw_tree_tagged` 是否有节点的绘制输出真正改变（增量脏区判据）。
    /// 为空表示画面逐命令与上一帧一致，平台可据此跳过 DisplayList 重建与 GPU 上传。
    static SCENE_CHANGED: Cell<bool> = const { Cell::new(false) };
}

/// 本轮绘制遍历是否产生了内容变化（须在 `draw_tree_tagged` 之后立即读取）。
pub fn scene_changed() -> bool {
    SCENE_CHANGED.with(|s| s.get())
}

/// 画面是否已 settled：自上次遍历以来无任何节点绘制失效 → 本帧与上一帧逐命令相同，
/// 平台可跳过整树绘制遍历。
///
/// 汇总信号用全局标记而非「根节点的 `draw_invalid`」：逐节点图层不把子树命令嵌进祖先，
/// 失效不必再沿祖先链传播（对齐 Compose：layer 失效由 render pass 统一收集）。
pub fn scene_is_clean(_root: NodeId) -> bool {
    SCENE_WALKED.with(|w| w.get()) && !RENDER_DIRTY.with(|d| d.get())
}

/// 遍历即消费：失效汇总标记清零、标记「本轮已遍历」，下一帧若无新失效即可走快路径①。
fn walk_consumed() {
    SCENE_WALKED.with(|s| s.set(true));
    RENDER_DIRTY.with(|s| s.set(false));
}

/// M3 elevation → 柔和投影：三层同心圆角矩形叠加，近似模糊（无离屏 pass）。
///
/// 只用于静态表面（卡片 / FAB / 对话框 / 菜单）。逐帧移动的元件别用——
/// 每层都是真实绘制命令，动画期会把整片背景重录一遍。
fn push_shadow(
    cmds: &mut Vec<DrawCmd>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    elevation: f32,
    color: Color,
) {
    if elevation <= 0.0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    // 由外到内：外圈扩散大、透明度低；向下轻微偏移模拟顶光。
    // 扩散与偏移刻意收紧（相对上一版约减半）：低海拔菜单贴着面板形成薄光晕，
    // 而不是底部一大块硬边厚带；高海拔卡片按 elevation 比例仍保留立体投影。
    const SPREAD: [f32; 3] = [1.1, 0.6, 0.22];
    for (i, s) in SPREAD.iter().enumerate() {
        let grow = elevation * s;
        let alpha = (color.a * elevation * 0.04 / (i as f32 * 0.9 + 0.7)).min(0.24);
        cmds.push(DrawCmd::FillRRect {
            x: x - grow,
            y: y - grow + elevation * 0.32,
            w: w + grow * 2.0,
            h: h + grow * 2.0,
            radius: radius + grow,
            color: Color::rgba(color.r, color.g, color.b, alpha),
        });
    }
}

fn draw_node(rt: &mut Rt, id: NodeId, abs_x: f32, abs_y: f32, out: &mut Out<'_>) {
    let idx = id.index() as u32;
    let (x, y, w, h, sdx, sdy, is_scroll) = {
        let n = &rt.nodes[id.index()];
        let (sdx, sdy) = match n.scroll.as_ref() {
            Some(s) if s.vertical => (0.0, -s.offset),
            Some(s) => (-s.offset, 0.0),
            None => (0.0, 0.0),
        };
        (
            abs_x + n.x,
            abs_y + n.y,
            n.width,
            n.height,
            sdx,
            sdy,
            n.scroll.is_some(),
        )
    };
    let key = [x, y, w, h, sdx, sdy];
    let (cx, cy) = (x + sdx, y + sdy);
    // 图层缓存 / 孩子表 take-put：孩子递归与绘制闭包都要再借 rt，取出的数据是本地所有权、
    // 不占借用。孩子表不 clone：每帧每个节点克隆一次 `Vec<NodeId>` 在动画期是纯浪费
    // （上百节点的页面就是每帧上百次堆分配），而遍历本身只需要读一遍。
    let cache = rt.nodes[id.index()].draw_cache.take();
    let children = std::mem::take(&mut rt.nodes[id.index()].children);

    // ── 增量绘制：未标脏且键（绝对位置/尺寸/滚动平移）一致 → 回放本节点片段，不跑绘制闭包 ──
    // 时间相关节点（光标闪烁 / Pulse）由帧驱动每帧置 draw_invalid，故不会被冻结在缓存里。
    if !rt.nodes[id.index()].draw_invalid && cache.as_ref().is_some_and(|c| c.key == key) {
        let c = cache.unwrap();
        out.emit(idx, SegPart::Prefix, &c.prefix, c.prefix_hash);
        for &chid in &children {
            draw_node(rt, chid, cx, cy, out);
        }
        out.emit(idx, SegPart::Suffix, &c.suffix, c.suffix_hash);
        let node = &mut rt.nodes[id.index()];
        node.draw_cache = Some(c);
        node.children = children;
        return;
    }

    let node = &rt.nodes[id.index()];
    let mods = node.modifier.clone();

    // 本节点片段缓冲（前缀 / 后缀各一段）：命令写入 out 的同时留档进图层缓存。
    let mut cmds: Vec<DrawCmd> = Vec::new();

    // 投影在背景之前（M3 elevation：先铺影再铺表面）
    for e in &mods.elems {
        if let ModElem::Shadow { elevation, radius, color } = e {
            push_shadow(&mut cmds, x, y, w, h, *radius, *elevation, *color);
        }
    }
    // 背景在 padding 外（Compose：background 覆盖整个节点含 padding）
    for e in &mods.elems {
        if let ModElem::Background { color, radius } = e {
            if *radius > 0.0 {
                cmds.push(DrawCmd::FillRRect {
                    x,
                    y,
                    w,
                    h,
                    radius: *radius,
                    color: *color,
                });
            } else {
                cmds.push(DrawCmd::FillRect {
                    x,
                    y,
                    w,
                    h,
                    color: *color,
                });
            }
        }
    }
    // 渐变背景（与实底同层：覆盖整个节点含 padding）
    for e in &mods.elems {
        if let ModElem::BackgroundGradient {
            from,
            to,
            radius,
            vertical,
        } = e
        {
            if *radius > 0.0 {
                cmds.push(DrawCmd::FillRRectGradient {
                    x,
                    y,
                    w,
                    h,
                    radius: *radius,
                    from: *from,
                    to: *to,
                    vertical: *vertical,
                });
            } else {
                cmds.push(DrawCmd::FillRectGradient {
                    x,
                    y,
                    w,
                    h,
                    from: *from,
                    to: *to,
                    vertical: *vertical,
                });
            }
        }
    }
    // 液态玻璃：背景模糊材质（在孩子之前铺底）
    for e in &mods.elems {
        if let ModElem::Glass(g) = e {
            cmds.push(DrawCmd::GlassPanel {
                x,
                y,
                w,
                h,
                radius: g.radius,
                tint: g.tint,
                blur: g.blur,
                highlight: g.highlight,
                outline: g.outline,
                line: g.line,
            });
        }
    }
    // clip
    let clip_radius = mods.elems.iter().find_map(|e| match e {
        ModElem::Clip(r) => Some(*r),
        _ => None,
    });
    if let Some(r) = clip_radius {
        cmds.push(DrawCmd::ClipPush {
            x,
            y,
            w,
            h,
            radius: r,
        });
    }

    // 滚动容器：裁剪内容（平移在 prologue 算好，键里也带着它 ⇒ 滚动必然失配本节点缓存）
    if is_scroll {
        cmds.push(DrawCmd::ClipPush {
            x,
            y,
            w,
            h,
            radius: clip_radius.unwrap_or(0.0),
        });
    }

    // 自身绘制内容（take-put：闭包可能间接访问 runtime）。
    // 注意：绘制闭包允许依赖时间（光标闪烁、Pulse 等），故不能无条件缓存；
    // 失效判据 = `draw_invalid`（时间/内容驱动）+ 位置尺寸滚动键（几何驱动）。
    let draw_fn = rt.nodes[id.index()].draw.take();
    if let Some(draw_fn) = draw_fn {
        // 绘制闭包直接写进前缀缓冲：省掉一条临时 Vec 的分配与逐命令复制（动画期每帧发生）。
        let mut b = DrawBuilder::with_buffer(cmds);
        draw_fn(&mut b, cx, cy, w, h);
        cmds = b.cmds;
        rt.nodes[id.index()].draw = Some(draw_fn);
    }

    // 按压波纹：在本节点自身内容之上、孩子之下（对齐 Flutter `Material` 的 ink 层顺序）。
    push_ripples(rt, idx, &mut cmds, x, y, w, h);

    // 前缀收档：命令留档进图层缓存，再按输出模式接进输出流（孩子的命令紧随其后）。
    let mut suffix: Vec<DrawCmd> = Vec::new();
    let prefix = std::mem::take(&mut cmds);
    let prefix_hash = segment_hash(&prefix);
    out.emit(idx, SegPart::Prefix, &prefix, prefix_hash);
    let mut layer = DrawLayer {
        key,
        prefix,
        prefix_hash,
        suffix: Vec::new(),
        suffix_hash: 0,
    };

    // 孩子
    for &chid in &children {
        draw_node(rt, chid, cx, cy, out);
    }

    if is_scroll {
        suffix.push(DrawCmd::ClipPop);
    }

    // 滚动条（非移动平台、内容溢出时显示；绘制在内容之上、边框之下）
    // `Modifier::hide_scrollbar` 的容器不画滚动条（滚动能力保留）。
    if is_scroll && !is_mobile() && !mods.scrollbar_hidden() {
        if let Some(geo) = scrollbar_geometry(rt, id, x, y) {
            let theme = crate::material::current_colors_global();
            let track = theme.on_surface.with_alpha(0.16);
            let thumb = theme.on_surface.with_alpha(0.42);
            suffix.push(DrawCmd::FillRRect {
                x: geo.track_x,
                y: geo.track_y,
                w: geo.track_w,
                h: geo.track_h,
                radius: geo.track_w * 0.5,
                color: track,
            });
            suffix.push(DrawCmd::FillRRect {
                x: geo.thumb_x,
                y: geo.thumb_y,
                w: geo.thumb_w,
                h: geo.thumb_h,
                radius: geo.thumb_w * 0.5,
                color: thumb,
            });
        }
    }

    // 边框最后画（盖住孩子溢出）
    for e in &mods.elems {
        if let ModElem::Border {
            width,
            color,
            radius,
        } = e
        {
            suffix.push(DrawCmd::StrokeRRect {
                x,
                y,
                w,
                h,
                radius: *radius,
                color: *color,
                line: *width,
            });
        }
    }

    if clip_radius.is_some() {
        suffix.push(DrawCmd::ClipPop);
    }

    // ── 图层缓存写回：只有本节点自己的片段变化才算脏区 ──
    // 祖先不嵌入子树命令，因此无需向上失效传播（旧实现每层各自整段深比较 + 两次克隆）。
    let suffix_hash = segment_hash(&suffix);
    out.emit(idx, SegPart::Suffix, &suffix, suffix_hash);
    layer.suffix = suffix;
    layer.suffix_hash = suffix_hash;
    let changed = cache.as_ref().is_none_or(|old| *old != layer);
    let node = &mut rt.nodes[id.index()];
    node.draw_invalid = false;
    node.draw_cache = Some(layer);
    node.children = children;
    if changed {
        // 本节点输出与上一帧不同 → 记入全局脏区标志，平台据此决定重建/上传。
        SCENE_CHANGED.with(|s| s.set(true));
    }
}

// ══════════════════════════════ 命中测试与输入 ══════════════════════════════

/// 命中测试：返回 (目标节点, 节点内局部坐标)。孩子优先（后画在上）。
pub fn hit_test(root: NodeId, px: f32, py: f32) -> Option<(NodeId, f32, f32)> {
    crate::runtime::with_rt_pub(|rt| hit_node(rt, root, px, py, 0.0, 0.0))
}

fn hit_node(rt: &Rt, id: NodeId, px: f32, py: f32, ax: f32, ay: f32) -> Option<(NodeId, f32, f32)> {
    let n = &rt.nodes[id.index()];
    // 装饰性覆盖层（`Modifier::no_hit`）不参与命中：返回 None 让兄弟继续参与。
    if n.modifier.is_no_hit() {
        return None;
    }
    let x = ax + n.x;
    let y = ay + n.y;
    if px < x || py < y || px > x + n.width || py > y + n.height {
        return None;
    }
    // 滚动偏移修正（纵向减 y、横向减 x）
    let (sx, sy) = match n.scroll.as_ref() {
        Some(s) if s.vertical => (0.0, s.offset),
        Some(s) => (s.offset, 0.0),
        None => (0.0, 0.0),
    };
    // 孩子优先（后画在上）
    let children = n.children.clone();
    for chid in children.iter().rev() {
        if let Some(hit) = hit_node(rt, *chid, px, py, x - sx, y - sy) {
            return Some(hit);
        }
    }
    Some((id, px - x, py - y))
}

// 点击坐标（事件分发时记录，Slider 等读取局部坐标）。
thread_local! {
    static LAST_CLICK: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}

pub(crate) fn set_last_click(x: f32, y: f32) {
    LAST_CLICK.with(|c| c.set((x, y)));
}

/// 最近一次点击的节点内局部坐标。
pub fn last_click_local() -> (f32, f32) {
    LAST_CLICK.with(|c| c.get())
}

// 最近一次指针的**全局**坐标（按下/移动/抬起均记录，供拖拽起点计算）。
thread_local! {
    static LAST_POINTER: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}

pub fn set_last_pointer_global(x: f32, y: f32) {
    LAST_POINTER.with(|p| p.set((x, y)));
}

pub fn last_pointer_global() -> (f32, f32) {
    LAST_POINTER.with(|p| p.get())
}

// 拖拽会话：`(container_x, container_y, container_w, container_h, 比例状态, 是否水平)`。
//
// 在按下分隔条时记录容器几何快照（容器几何在拖拽期间恒定），并持有可复制的比例
// `MutableState`；后续 `pointer_move` 直接更新比例，**不依赖随重组变化的节点 id**。
thread_local! {
    static DRAG_SESSION: RefCell<Option<DragSession>> = RefCell::new(None);
}

/// 一次比例拖拽（分隔条 / 滑块）：全局指针 → 归属矩形内的 0..1，写回 `ratio`。
#[derive(Clone)]
struct DragSession {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ratio: MutableState<f32>,
    horizontal: bool,
    /// 比例取值范围（分隔条留 0.05 边距防止某一侧完全消失；滑块用 0..1）。
    lo: f32,
    hi: f32,
}

/// 开始一次比例拖拽（在按下时调用 —— 指针移动阶段才会被 `drag_move_global` 驱动）。
pub fn begin_drag_session(
    cx: f32,
    cy: f32,
    cw: f32,
    ch: f32,
    ratio: MutableState<f32>,
    horizontal: bool,
    lo: f32,
    hi: f32,
) {
    DRAG_SESSION.with(|d| {
        *d.borrow_mut() = Some(DragSession {
            x: cx,
            y: cy,
            w: cw,
            h: ch,
            ratio,
            horizontal,
            lo,
            hi,
        })
    });
}

/// 节点绝对矩形（窗口坐标）——手势把「全局指针」换算成「本节点内比例」时用。
pub fn node_abs_rect(id: NodeId) -> (f32, f32, f32, f32) {
    crate::runtime::with_rt_pub(|rt| abs_rect(rt, id))
}

/// 指针移动时驱动拖拽；返回是否有活动拖拽被处理。
pub fn drag_move_global(x: f32, y: f32) -> bool {
    let sess = DRAG_SESSION.with(|d| d.borrow().clone());
    let Some(s) = sess else { return false };
    let nr = if s.horizontal {
        if s.w > 0.0 {
            ((x - s.x) / s.w).clamp(s.lo, s.hi)
        } else {
            s.ratio.get()
        }
    } else if s.h > 0.0 {
        ((y - s.y) / s.h).clamp(s.lo, s.hi)
    } else {
        s.ratio.get()
    };
    s.ratio.set(nr);
    true
}

/// 指针抬起时结束拖拽；返回之前是否有活动拖拽。
pub fn drag_end_global() -> bool {
    let had = DRAG_SESSION.with(|d| d.borrow().is_some());
    DRAG_SESSION.with(|d| *d.borrow_mut() = None);
    had
}

// ── 通用二维拖拽（取色器 SV 方块 / 停靠面板拖放等需同时读 x/y 的手势）──

/// 一次二维拖拽会话：`on_move` 逐次收到全局指针，`on_end` 在抬起时收到**最终**全局指针。
///
/// `on_end` 是 Docking 之类「拖到哪就放到哪」手势的提交点：没有它，控件只能知道
/// 拖拽经过的位置，无法在释放那一刻结算放置目标。
struct Drag2D {
    on_move: Box<dyn FnMut(f32, f32)>,
    on_end: Option<Box<dyn FnOnce(f32, f32)>>,
}

thread_local! {
    /// 活动二维拖拽会话（全局唯一）。
    static DRAG_2D: RefCell<Option<Drag2D>> = const { RefCell::new(None) };
}

/// 开始一次二维拖拽（按下时调用）：`on_move` 在指针移动阶段收到全局 `(x, y)`。
/// 与一维比例会话互斥使用（同一时刻只跑一个手势）。
pub fn begin_drag_2d(on_move: impl FnMut(f32, f32) + 'static) {
    DRAG_2D.with(|d| {
        *d.borrow_mut() = Some(Drag2D {
            on_move: Box::new(on_move),
            on_end: None,
        })
    });
}

/// 带释放回调的二维拖拽：`on_end` 在指针抬起时收到最终全局 `(x, y)`。
///
/// 回调同样在 runtime 借用之外执行（take-put），其内部可以安全地写 `MutableState`。
pub fn begin_drag_2d_with_end(
    on_move: impl FnMut(f32, f32) + 'static,
    on_end: impl FnOnce(f32, f32) + 'static,
) {
    DRAG_2D.with(|d| {
        *d.borrow_mut() = Some(Drag2D {
            on_move: Box::new(on_move),
            on_end: Some(Box::new(on_end)),
        })
    });
}

/// 指针移动时驱动二维拖拽；返回是否消费了本次移动。
pub fn drag_2d_move_global(x: f32, y: f32) -> bool {
    // take-put：回调可能间接触及 runtime / 再次发起拖拽，不能持有 RefCell 借用。
    let mut sess = DRAG_2D.with(|d| d.borrow_mut().take());
    let Some(s) = sess.as_mut() else {
        return false;
    };
    (s.on_move)(x, y);
    DRAG_2D.with(|d| *d.borrow_mut() = sess);
    true
}

/// 指针抬起结束二维拖拽；返回之前是否有活动会话。
pub fn drag_2d_end_global() -> bool {
    let (gx, gy) = last_pointer_global();
    let mut sess = DRAG_2D.with(|d| d.borrow_mut().take());
    let Some(s) = sess.as_mut() else {
        return false;
    };
    if let Some(f) = s.on_end.take() {
        f(gx, gy);
    }
    true
}

// ══════════════════════ 下拉刷新手势（pull-to-refresh） ══════════════════════

/// 下拉手势阶段。
#[derive(Clone, Copy, Debug)]
pub enum PullPhase {
    /// 拖动中（原始下拉距离，px，向下为正）。
    Move(f32),
    /// 手势结束（原始下拉距离，px；未越过阈值则应取消）。
    Release(f32),
}

/// 下拉目标（滚动容器 + 手势处理器）。
struct PullTarget {
    /// 当前有效的节点 id（**每次组合都会变**，故随组合更新）。
    node: NodeId,
    /// 手势处理器。
    handler: Box<dyn FnMut(PullPhase)>,
}

/// 下拉会话（指针按下 → 移动 → 抬起）。
struct PullSession {
    /// 目标 key（稳定，跨重组不变）。
    key: u64,
    /// 按下时的全局 y。
    start_y: f32,
    /// 当前下拉距离（px）。
    dist: f32,
}

thread_local! {
    /// 下拉目标注册表：稳定 key → 目标（key 跨重组不变，节点 id 随组合更新）。
    static PULL_TARGETS: RefCell<HashMap<u64, PullTarget>> = RefCell::new(HashMap::new());
    /// 当前下拉会话（全局唯一）。
    static PULL_SESSION: RefCell<Option<PullSession>> = RefCell::new(None);
    /// 下拉目标 key 种子。
    static PULL_KEY_SEED: Cell<u64> = const { Cell::new(0) };
}

/// 生成稳定的下拉目标 key（配合 `remember` 使用 → 跨重组不变）。
pub fn next_pull_key() -> u64 {
    PULL_KEY_SEED.with(|k| {
        let v = k.get() + 1;
        k.set(v);
        v
    })
}

/// 注册下拉刷新目标（组合期调用；同 `key` 覆盖旧登记）。
///
/// `node` 为滚动容器节点；节点 id 随重组变化，故每次组合都要重新登记，
/// 由**稳定 key** 关联，从而手势不依赖节点 id。
pub fn register_pull_target(key: u64, node: NodeId, handler: Box<dyn FnMut(PullPhase)>) {
    PULL_TARGETS.with(|m| {
        m.borrow_mut().insert(key, PullTarget { node, handler });
    });
}

/// 注销下拉刷新目标（控件销毁时调用，避免注册表残留）。
pub fn unregister_pull_target(key: u64) {
    PULL_TARGETS.with(|m| {
        m.borrow_mut().remove(&key);
    });
}

/// 指针按下：沿命中链向上找**已注册且已滚到顶**的下拉容器，开启下拉会话。
///
/// 返回是否开启了会话（开启后 `pointer_move` / `pointer_up` 由
/// [`pull_move_global`] / [`pull_end_global`] 接管）。
pub fn dispatch_pull_down(target: NodeId, global_y: f32) -> bool {
    // 1) 命中链（自内向上的祖先序列）
    let chain = crate::runtime::with_rt_pub(|rt| {
        let mut out: Vec<NodeId> = Vec::new();
        let mut cur = Some(target);
        while let Some(id) = cur {
            out.push(id);
            cur = rt.nodes[id.index()].parent;
        }
        out
    });
    // 2) 链上第一个已注册的下拉目标
    let found = PULL_TARGETS.with(|m| {
        let mm = m.borrow();
        chain.iter().find_map(|id| {
            mm.iter()
                .find(|(_, t)| t.node == *id)
                .map(|(k, _)| (*k, *id))
        })
    });
    let Some((key, node)) = found else {
        return false;
    };
    // 3) 需滚到顶才允许下拉（否则手势属于滚动，不属于下拉）
    let at_top = crate::runtime::with_rt_pub(|rt| {
        rt.nodes[node.index()]
            .scroll
            .as_ref()
            .map(|s| s.offset <= 0.0)
            .unwrap_or(true)
    });
    if !at_top {
        return false;
    }
    PULL_SESSION.with(|s| {
        *s.borrow_mut() = Some(PullSession {
            key,
            start_y: global_y,
            dist: 0.0,
        });
    });
    true
}

/// 指针移动：推进下拉会话（返回是否消费了本次移动）。
pub fn pull_move_global(global_y: f32) -> bool {
    let sess = PULL_SESSION.with(|s| {
        let mut g = s.borrow_mut();
        match g.as_mut() {
            Some(x) => {
                x.dist = (global_y - x.start_y).max(0.0);
                Some((x.key, x.dist))
            }
            None => None,
        }
    });
    let Some((key, dist)) = sess else {
        return false;
    };
    notify_pull(key, PullPhase::Move(dist));
    true
}

/// 指针抬起：结束下拉会话并通知（返回之前是否有活动会话）。
pub fn pull_end_global() -> bool {
    let sess = PULL_SESSION.with(|s| s.borrow_mut().take());
    let Some(sess) = sess else { return false };
    notify_pull(sess.key, PullPhase::Release(sess.dist));
    true
}

/// 向下拉目标派发手势阶段（就地调用；用 `try_borrow_mut` 避免回调内重入导致 panic）。
fn notify_pull(key: u64, phase: PullPhase) {
    let _ = PULL_TARGETS.try_with(|m| {
        if let Ok(mut mm) = m.try_borrow_mut() {
            if let Some(t) = mm.get_mut(&key) {
                (t.handler)(phase);
            }
        }
    });
}

// ══════════════════════ 滚动条（非移动平台） ══════════════════════

/// 滚动条轨道 / 滑块几何（绝对坐标）。
struct ScrollbarGeo {
    track_x: f32,
    track_y: f32,
    track_w: f32,
    track_h: f32,
    thumb_x: f32,
    thumb_y: f32,
    thumb_w: f32,
    thumb_h: f32,
}

/// 是否移动平台（触屏为主 → 默认隐藏滚动条；桌面平台 → 显示滚动条）。
///
/// 以**编译目标**为代理：android / ios / ohos 视为移动端。
#[inline]
pub fn is_mobile() -> bool {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        true
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        false
    }
}

/// 滚动条厚度（轨道宽度）。
pub const SCROLLBAR_THICKNESS: f32 = 8.0;
/// 滚动条距节点交叉轴末端的外边距。
pub const SCROLLBAR_MARGIN: f32 = 2.0;
/// 桌面端滚动容器在**交叉轴末端**为滚动条预留的空间（= 厚度 + 两侧边距）。
/// 布局阶段就扣掉这段 gutter，使内容永不被滚动条覆盖（对齐浏览器
/// `scrollbar-gutter: stable` 的预留语义）；命中区亦为 12px，与滚动条区域一致。
pub const SCROLLBAR_GUTTER: f32 = SCROLLBAR_THICKNESS + SCROLLBAR_MARGIN * 2.0;

/// 该节点作为滚动容器时需在交叉轴末端预留的滚动条 gutter（移动端 0，非滚动 / 交叉轴 0）。
/// `is_row` 为容器的政策主轴方向：沿主轴滚动（Row 横向 / Column 纵向）才在交叉轴留位。
fn scrollbar_gutter_for(rt: &Rt, id: NodeId, is_row: bool) -> f32 {
    if is_mobile() {
        return 0.0;
    }
    // 隐藏滚动条的容器不预留 gutter（无滚动条可避让）。
    if rt.nodes[id.index()].modifier.scrollbar_hidden() {
        return 0.0;
    }
    match rt.nodes[id.index()].scroll.as_ref() {
        Some(s) if s.vertical == !is_row => SCROLLBAR_GUTTER,
        _ => 0.0,
    }
}

/// 节点绝对矩形（含父链偏移；命中测试以 (0,0) 起算，与此一致）。
fn abs_rect(rt: &Rt, id: NodeId) -> (f32, f32, f32, f32) {
    let mut path: Vec<NodeId> = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        path.push(c);
        cur = rt.nodes[c.index()].parent;
    }
    path.reverse();
    let (mut ax, mut ay) = (0.0, 0.0);
    for p in path {
        ax += rt.nodes[p.index()].x;
        ay += rt.nodes[p.index()].y;
    }
    let n = &rt.nodes[id.index()];
    (ax, ay, n.width, n.height)
}

/// 计算滚动条几何；内容未溢出则返回 `None`。
fn scrollbar_geometry(rt: &Rt, id: NodeId, ax: f32, ay: f32) -> Option<ScrollbarGeo> {
    let n = &rt.nodes[id.index()];
    let s = n.scroll.as_ref()?;
    let vertical = s.vertical;
    let viewport = if vertical { n.height } else { n.width };
    let overflow = s.content_size - viewport;
    if overflow <= 0.0 {
        return None;
    }
    let thickness = SCROLLBAR_THICKNESS;
    let margin = SCROLLBAR_MARGIN;
    if vertical {
        let track_w = thickness;
        let track_h = n.height - margin * 2.0;
        let track_x = ax + n.width - thickness - margin;
        let track_y = ay + margin;
        let thumb_len = ((viewport / s.content_size) * track_h).clamp(18.0, track_h);
        let thumb_y = track_y + (s.offset / overflow) * (track_h - thumb_len);
        Some(ScrollbarGeo {
            track_x,
            track_y,
            track_w,
            track_h,
            thumb_x: track_x,
            thumb_y,
            thumb_w: track_w,
            thumb_h: thumb_len,
        })
    } else {
        let track_h = thickness;
        let track_w = n.width - margin * 2.0;
        let track_x = ax + margin;
        let track_y = ay + n.height - thickness - margin;
        let thumb_len = ((viewport / s.content_size) * track_w).clamp(18.0, track_w);
        let thumb_x = track_x + (s.offset / overflow) * (track_w - thumb_len);
        Some(ScrollbarGeo {
            track_x,
            track_y,
            track_w,
            track_h,
            thumb_x,
            thumb_y: track_y,
            thumb_w: thumb_len,
            thumb_h: track_h,
        })
    }
}

/// 自命中目标上溯，找命中滚动条区域（右侧 12px 竖条 / 底部 12px 横条）的滚动容器。
fn scrollbar_hit(target: NodeId, x: f32, y: f32) -> Option<NodeId> {
    crate::runtime::with_rt_pub(|rt| {
        let mut cur = Some(target);
        while let Some(id) = cur {
            // 隐藏滚动条的容器无可拖拽的条区，不参与滚动条命中。
            if rt.nodes[id.index()].modifier.scrollbar_hidden() {
                cur = rt.nodes[id.index()].parent;
                continue;
            }
            let s = rt.nodes[id.index()].scroll.clone();
            if let Some(s) = s {
                let (ax, ay, w, h) = abs_rect(rt, id);
                let viewport = if s.vertical { h } else { w };
                let overflow = s.content_size - viewport;
                if overflow > 0.0 {
                    let in_strip = if s.vertical {
                        x >= ax + w - 12.0 && x <= ax + w && y >= ay && y <= ay + h
                    } else {
                        y >= ay + h - 12.0 && y <= ay + h && x >= ax && x <= ax + w
                    };
                    if in_strip {
                        return Some(id);
                    }
                }
            }
            cur = rt.nodes[id.index()].parent;
        }
        None
    })
}

/// 滚动条拖拽会话。
#[derive(Clone)]
struct ScrollbarSession {
    node: NodeId,
    vertical: bool,
    start_pos: f32,
    start_offset: f32,
    ratio: f32,
}

thread_local! {
    /// 活动滚动条拖拽会话（全局唯一）。
    static SCROLLBAR_SESSION: RefCell<Option<ScrollbarSession>> = const { RefCell::new(None) };
}

/// 指针按下：若落在滚动条区域则开启拖拽会话并消费事件（不再透传内容点击）。
pub fn dispatch_scrollbar_down(target: NodeId, x: f32, y: f32) -> bool {
    let Some(node) = scrollbar_hit(target, x, y) else {
        return false;
    };
    let session = crate::runtime::with_rt_pub(|rt| {
        let n = &rt.nodes[node.index()];
        let s = n.scroll.as_ref().unwrap();
        let vertical = s.vertical;
        let viewport = if vertical { n.height } else { n.width };
        let ratio = s.content_size / viewport.max(1.0);
        ScrollbarSession {
            node,
            vertical,
            start_pos: if vertical { y } else { x },
            start_offset: s.offset,
            ratio,
        }
    });
    SCROLLBAR_SESSION.with(|s| *s.borrow_mut() = Some(session));
    true
}

/// 指针移动：推进滚动条拖拽（返回是否消费）。
pub fn scrollbar_move_global(x: f32, y: f32) -> bool {
    let sess = SCROLLBAR_SESSION.with(|s| s.borrow().clone());
    let Some(sess) = sess else {
        return false;
    };
    let delta = if sess.vertical {
        y - sess.start_pos
    } else {
        x - sess.start_pos
    };
    let new_off = (sess.start_offset + delta * sess.ratio).max(0.0);
    crate::runtime::with_rt_pub(|rt| {
        let n = &mut rt.nodes[sess.node.index()];
        if let Some(s) = n.scroll.as_mut() {
            let viewport = if sess.vertical { n.height } else { n.width };
            let max_off = (s.content_size - viewport).max(0.0);
            s.offset = new_off.min(max_off);
        }
        // 增量绘制：偏移变化 → 子树（内容绝对坐标随之移动）与祖先缓存均失效
        mark_dirty_subtree(rt, sess.node);
        mark_draw_invalid_path(rt, sess.node);
    });
    true
}

/// 指针抬起：结束滚动条拖拽（返回之前是否有活动会话）。
pub fn scrollbar_end_global() -> bool {
    SCROLLBAR_SESSION.with(|s| s.borrow_mut().take().is_some())
}

/// 处理器在 runtime 借用外执行（take-put，处理器内部可再入 runtime）。
pub fn dispatch_click_at(target: NodeId, local_x: f32, local_y: f32) -> bool {
    set_last_click(local_x, local_y);
    let handler = crate::runtime::with_rt_pub(|rt| {
        // 1) 焦点：沿**整条**祖先链找最近的键盘容器。
        //    必须与点击查找分开走完，否则点击被某个内层 on_click 消费后，
        //    上层的 focusable（侧栏等）就永远轮不到 → 点条目反而丢失焦点。
        let mut focus_sid = None;
        let mut cur = Some(target);
        while let Some(id) = cur {
            if rt.nodes[id.index()].key_handler.is_some() {
                focus_sid = rt.nodes[id.index()].owner_scope;
                break;
            }
            cur = rt.nodes[id.index()].parent;
        }
        // 2) 点击：沿祖先链找最近的 on_click 消费。
        let mut cur = Some(target);
        while let Some(id) = cur {
            if let Some(f) = rt.nodes[id.index()].on_click.take() {
                return Some((id, f, focus_sid));
            }
            cur = rt.nodes[id.index()].parent;
        }
        None
    });
    match handler {
        Some((id, mut f, focus_sid)) => {
            // 焦点变化同时失效新旧作用域（聚焦外框等焦点读者得以重组）。
            match focus_sid {
                Some(sid) => crate::runtime::set_focus(sid),
                None => crate::runtime::clear_focus(),
            }
            f();
            crate::runtime::with_rt_pub(|rt| rt.nodes[id.index()].on_click = Some(f));
            true
        }
        None => false,
    }
}

/// 兼容入口：不带坐标的点击分发。
pub fn dispatch_click(target: NodeId) -> bool {
    dispatch_click_at(target, 0.0, 0.0)
}

/// 次要按钮（鼠标右键）点击分发：沿祖先链找最近的 `on_secondary_click` 消费。
///
/// **不**回落到 `on_click`（右键不应触发左键行为）；焦点逻辑与
/// [`dispatch_click_at`] 一致，右键点击同样把焦点交给所在的键盘容器。
/// 处理器在 runtime 借用外执行（take-put，处理器内部可再入 runtime）。
pub fn dispatch_secondary_click_at(target: NodeId, local_x: f32, local_y: f32) -> bool {
    set_last_click(local_x, local_y);
    let handler = crate::runtime::with_rt_pub(|rt| {
        let mut focus_sid = None;
        let mut cur = Some(target);
        while let Some(id) = cur {
            if rt.nodes[id.index()].key_handler.is_some() {
                focus_sid = rt.nodes[id.index()].owner_scope;
                break;
            }
            cur = rt.nodes[id.index()].parent;
        }
        let mut cur = Some(target);
        while let Some(id) = cur {
            if let Some(f) = rt.nodes[id.index()].on_secondary_click.take() {
                return Some((id, f, focus_sid));
            }
            cur = rt.nodes[id.index()].parent;
        }
        None
    });
    match handler {
        Some((id, mut f, focus_sid)) => {
            match focus_sid {
                Some(sid) => crate::runtime::set_focus(sid),
                None => crate::runtime::clear_focus(),
            }
            f();
            crate::runtime::with_rt_pub(|rt| rt.nodes[id.index()].on_secondary_click = Some(f));
            true
        }
        None => false,
    }
}

/// 兼容入口：不带坐标的右键分发。
pub fn dispatch_secondary_click(target: NodeId) -> bool {
    dispatch_secondary_click_at(target, 0.0, 0.0)
}

// ══════════════ 按压指示（Flutter `InkWell` 语义：按下起波纹、抬起结算点击） ══════════════

/// 波纹线速度（px/秒）：M3 波纹按固定线速度从按压点向外生长，不按容器尺寸归一。
const RIPPLE_SPEED: f32 = 900.0;
/// 抬手后波纹淡出时长（秒）。按住不放时波纹停在满半径（Flutter 的 splash 行为）。
const RIPPLE_FADE_SECS: f32 = 0.35;
/// 同上，纳秒（按压计时用 `f64` 墙钟纳秒，避免每次逐处转换）。
const RIPPLE_FADE_NS: f64 = RIPPLE_FADE_SECS as f64 * 1_000_000_000.0;
/// 波纹不透明度（略低于 `STATE_PRESSED`：叠加在控件本色上，太实会盖掉内容色）。
const RIPPLE_ALPHA: f32 = 0.10;
/// 抬起点偏离按压点超过此值即取消点击（Flutter `kTouchSlop` = 8 逻辑像素）。
pub const TAP_SLOP: f32 = 8.0;
/// 同时存活的最大波纹数（快速连点时旧的先让位，避免无上限增长）。
const RIPPLE_MAX: usize = 6;

thread_local! {
    /// 按压计时基准。必须是**实时**单调钟而不是帧钟：空闲休眠时帧钟不前进，
    /// 按下时刻会停在很久以前，波纹一上来就已是「长大完毕」的状态。
    static PRESS_EPOCH: std::time::Instant = std::time::Instant::now();
}

fn press_now_ns() -> f64 {
    PRESS_EPOCH.with(|e| e.elapsed().as_nanos() as f64)
}

/// 一次「按下」的记账：抬起时该结算给谁、该在哪个节点画波纹。
pub(crate) struct Press {
    /// 最近的 `on_click` 祖先（抬起时位移在 slop 内且仍是同一节点才触发点击）。
    click_node: usize,
    /// 波纹归属节点（= `click_node` 且带 `Modifier::indication` 时才是 `Some`）。
    ripple_node: Option<usize>,
    /// 按下点（全局坐标）。
    gx: f32,
    gy: f32,
    /// 按下时归属节点的绝对矩形。重组会回收旧节点、把下标放进 `node_free` 复用，
    /// 但同一控件的几何不变；抬起时据此识别「换槽后的同一个控件」，避免
    /// hover 等状态变化引发的重组把这一次的点击吞掉。
    click_rect: (f32, f32, f32, f32),
}

/// 一枚波纹：按下期间生长，抬手后淡出。
pub(crate) struct Ripple {
    node: usize,
    gx: f32,
    gy: f32,
    down_ns: f64,
    /// `f64::MAX` = 仍按住。
    up_ns: f64,
    /// 满半径（按压点到归属节点四角的最大距离）——生长到此即停，动画结束可让宿主休眠。
    max_r: f32,
}

/// 沿祖先链找最近的带 `on_click` 的节点下标。
fn nearest_click_idx(rt: &Rt, target: NodeId) -> Option<usize> {
    let mut cur = Some(target);
    while let Some(id) = cur {
        let n = &rt.nodes[id.index()];
        if n.on_click.is_some() {
            return Some(id.index());
        }
        cur = n.parent;
    }
    None
}

/// 自身 + 全部祖先的绘制片段失效（波纹画在本节点前缀里，父层需要重排该段）。
fn invalidate_self_and_ancestors(rt: &mut Rt, idx: usize) {
    rt.nodes[idx].draw_invalid = true;
    mark_draw_invalid_path(rt, NodeId(idx as u32));
}

/// 指针按下：登记按压 + 起波纹 + 派发 `on_press`（需要在抬起前开始的手势，如分栏拖拽）。
///
/// **不**触发 `on_click` —— 与 Flutter/Compose 一致，点击在抬起时结算。
/// 返回是否命中了可点击链。
pub fn dispatch_press_down(target: NodeId, gx: f32, gy: f32) -> bool {
    let now = press_now_ns();
    let (hit, handler) = crate::runtime::with_rt_pub(|rt| {
        // 上一次按压的抬起事件可能整个丢了（在窗外释放 / 平台没投递）：
        // 新按下时把仍「按住中」的波纹就地转成淡出，否则会有一枚 splash 永久挂在那里。
        for r in rt.ripples.iter_mut() {
            if r.up_ns == f64::MAX {
                r.up_ns = now;
            }
        }
        let Some(idx) = nearest_click_idx(rt, target) else {
            return (false, None);
        };
        let click_rect = abs_rect(rt, NodeId(idx as u32));
        let ripple_node = if rt.nodes[idx].modifier.ripples_off() {
            None
        } else {
            Some(idx)
        };
        if let Some(r) = ripple_node {
            let (nx, ny, nw, nh) = click_rect;
            rt.ripples.push(Ripple {
                node: r,
                gx,
                gy,
                down_ns: now,
                up_ns: f64::MAX,
                max_r: max_corner_dist(gx - nx, gy - ny, nw, nh),
            });
            if rt.ripples.len() > RIPPLE_MAX {
                let drop = rt.ripples.len() - RIPPLE_MAX;
                rt.ripples.drain(0..drop);
            }
            invalidate_self_and_ancestors(rt, r);
        }
        rt.press = Some(Press {
            click_node: idx,
            ripple_node,
            gx,
            gy,
            click_rect,
        });
        (true, rt.nodes[idx].on_press.take())
    });
    if !hit {
        return false;
    }
    let Some(mut f) = handler else {
        return true;
    };
    f();
    crate::runtime::with_rt_pub(|rt| {
        if let Some(idx) = rt.press.as_ref().map(|p| p.click_node) {
            rt.nodes[idx].on_press = Some(f);
        }
    });
    true
}

/// 指针抬起：结束按压。位移在 [`TAP_SLOP`] 内且抬起链上的可点击节点与按下时同一个，
/// 才触发 `on_click`（对齐 Flutter `onTap`）。返回是否触发了点击。
pub fn dispatch_press_up(hit: Option<(NodeId, f32, f32)>, gx: f32, gy: f32) -> bool {
    let now = press_now_ns();
    let to_click = crate::runtime::with_rt_pub(|rt| {
        let Some(p) = rt.press.take() else { return None };
        // 波纹转入淡出。重组可能换掉 arena 下标：优先用抬起链上的同一节点，
        // 否则旧的归属可能已被别的控件复用 → 波纹会闪在错误位置。
        let up_idx = hit.and_then(|(t, _, _)| nearest_click_idx(rt, t));
        // 下标相同即同一节点。否则可能是 down/up 之间发生了重组：hover、动画等
        // 状态变化会重建节点并把旧下标放进 `node_free` 复用。此时只要抬起命中的
        // 控件几何与按下时一致，就仍是「同一个控件」，不该把这一次的点击吞掉。
        let same = up_idx == Some(p.click_node)
            || up_idx.is_some_and(|i| {
                let (x, y, w, h) = abs_rect(rt, NodeId(i as u32));
                let (px, py, pw, ph) = p.click_rect;
                (x - px).abs() <= 1.0
                    && (y - py).abs() <= 1.0
                    && (w - pw).abs() <= 1.0
                    && (h - ph).abs() <= 1.0
            });
        let fresh = up_idx
            .filter(|_| same)
            .filter(|i| !rt.nodes[*i].modifier.ripples_off());
        let mut marked: Option<usize> = None;
        if let Some(old) = p.ripple_node {
            let node = fresh.unwrap_or(old);
            if let Some(r) = rt.ripples.iter_mut().rev().find(|r| r.node == old) {
                r.up_ns = now;
                r.node = node;
                marked = Some(node);
            }
        }
        // 失效要再可变借 rt，不能与上面的 `iter_mut` 同行。
        if let Some(node) = marked {
            invalidate_self_and_ancestors(rt, node);
        }
        let within = (gx - p.gx).hypot(gy - p.gy) <= TAP_SLOP;
        match (within && same, hit) {
            (true, Some(h)) => Some(h),
            _ => None,
        }
    });
    match to_click {
        Some((target, lx, ly)) => dispatch_click_at(target, lx, ly),
        None => false,
    }
}

/// 是否有在飞的按压/波纹（宿主据此判断需要连续帧）。
pub fn press_active() -> bool {
    crate::runtime::with_rt_pub(|rt| rt.press.is_some() || !rt.ripples.is_empty())
}

/// 每帧推进按压状态：剪掉淡完/无主的波纹、失效在动的节点及其祖先、请求续帧。
/// 由 `runtime::advance_frame` 调用（与 `tick_animations` 同处）。
pub fn tick_press_state() {
    let now = press_now_ns();
    crate::runtime::with_rt_pub(|rt| {
        if rt.ripples.is_empty() {
            return;
        }
        // 归属节点被回收或已不带按压指示（控件被条件移除 / 槽位复用）→ 波纹无主，丢弃。
        let nodes = &rt.nodes;
        let dead = |r: &Ripple| {
            !ripple_alive(now, r)
                || !nodes.get(r.node).is_some_and(|n| {
                    n.alive && n.on_click.is_some() && !n.modifier.ripples_off()
                })
        };
        // 即将消失的节点要最后再失效一次：波纹画在它的前缀段里，不重排就残留淡到一半的圆。
        let dying: Vec<usize> = rt.ripples.iter().filter(|r| dead(*r)).map(|r| r.node).collect();
        rt.ripples.retain(|r| !dead(r));
        // 波纹画在归属节点的前缀段里 → 本节点 + 祖先链都要重排该段。
        // 先取下标：`invalidate_self_and_ancestors` 要再可变借 rt，不能与遍历同行。
        let ids: Vec<usize> = rt
            .ripples
            .iter()
            .filter(|r| ripple_animating(now, r))
            .map(|r| r.node)
            .collect();
        let animating = !ids.is_empty();
        for i in dying.into_iter().filter(|i| !ids.contains(i)) {
            invalidate_self_and_ancestors(rt, i);
        }
        for i in ids {
            invalidate_self_and_ancestors(rt, i);
        }
        // 长按已长满的 splash 不再请求续帧（画面停住，宿主可继续休眠）。
        if animating {
            crate::animation::request_anim_frames();
        }
    });
}

/// 把本节点上的波纹画进 `cmds`（按压点为圆心的圆，按线速度生长、抬手后淡出，裁到节点形状）。
fn push_ripples(rt: &Rt, idx: u32, cmds: &mut Vec<DrawCmd>, x: f32, y: f32, w: f32, h: f32) {
    // 每个节点每帧都会走到这里：先用一次廉价比较挡掉「本节点没有波纹」的绝大多数。
    if !rt.ripples.iter().any(|r| r.node as u32 == idx) {
        return;
    }
    let now = press_now_ns();
    let shape = ripple_shape(&rt.nodes[idx as usize].modifier);
    let color = crate::material::current_colors_global().on_surface;
    let mut clipped = false;
    for r in rt.ripples.iter().filter(|r| r.node as u32 == idx) {
        let radius = r.max_r.min(ripple_grow(now, r.down_ns));
        let alpha = ripple_alpha(now, r.up_ns);
        if radius <= 0.0 || alpha <= 0.0 {
            continue;
        }
        if !clipped {
            cmds.push(DrawCmd::ClipPush {
                x,
                y,
                w,
                h,
                radius: shape,
            });
            clipped = true;
        }
        cmds.push(DrawCmd::FillCircle {
            cx: r.gx,
            cy: r.gy,
            r: radius,
            color: color.with_alpha(RIPPLE_ALPHA * alpha),
        });
    }
    if clipped {
        cmds.push(DrawCmd::ClipPop);
    }
}

/// 波纹当前生长半径（线速度 × 按住时长；无上限，调用方再夹到 `max_r`）。
fn ripple_grow(now: f64, down_ns: f64) -> f32 {
    ((now - down_ns) as f32 / 1e9) * RIPPLE_SPEED
}

/// 波纹当前不透明度系数（按住 = 1，抬手后线性淡出到 0）。
fn ripple_alpha(now: f64, up_ns: f64) -> f32 {
    if up_ns == f64::MAX {
        1.0
    } else {
        (1.0 - (now - up_ns) / RIPPLE_FADE_NS) as f32
    }
}

/// 按压点到矩形四角的最大距离 = 波纹要「盖满」该矩形所需半径。
fn max_corner_dist(px: f32, py: f32, w: f32, h: f32) -> f32 {
    [
        (px, py),
        (px - w, py),
        (px, py - h),
        (px - w, py - h),
    ]
    .iter()
    .map(|(dx, dy)| dx.hypot(*dy))
    .fold(0.0f32, f32::max)
}

/// 波纹是否还活着（按住中，或抬手后仍在淡出窗口内）。
fn ripple_alive(now: f64, r: &Ripple) -> bool {
    r.up_ns == f64::MAX || now - r.up_ns < RIPPLE_FADE_NS
}

/// 波纹是否还在动（还在生长或还在淡出）——决定本帧要不要失效重绘、要不要请求续帧。
/// 长按到满半径后不再重绘：画面停在满 splash，与 Flutter 一致，且宿主可以重新休眠。
fn ripple_animating(now: f64, r: &Ripple) -> bool {
    if r.up_ns == f64::MAX {
        now - r.down_ns < (r.max_r as f64 / RIPPLE_SPEED as f64) * 1e9
    } else {
        now - r.up_ns < RIPPLE_FADE_NS
    }
}

/// 波纹裁剪用的形状圆角：取背景圆角与 `Clip` 圆角的较大者（M3 波纹跟组件形状走）。
fn ripple_shape(mods: &Modifier) -> f32 {
    let bg = mods
        .elems
        .iter()
        .filter_map(|e| match e {
            ModElem::Background { radius, .. } => Some(*radius),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    let clip = mods
        .elems
        .iter()
        .filter_map(|e| match e {
            ModElem::Clip(r) => Some(*r),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    bg.max(clip)
}

/// 滚轮分发：找目标链上第一个滚动容器。
pub fn dispatch_scroll(target: Option<NodeId>, dy: f32) -> bool {
    crate::runtime::with_rt_pub(|rt| {
        let mut cur = target;
        while let Some(id) = cur {
            let has_scroll = rt.nodes[id.index()].scroll.is_some();
            if has_scroll {
                let n = &mut rt.nodes[id.index()];
                let s = n.scroll.as_mut().unwrap();
                let max_off =
                    (s.content_size - if s.vertical { n.height } else { n.width }).max(0.0);
                s.offset = (s.offset + dy).clamp(0.0, max_off);
                // 增量绘制：偏移变化 → 子树与祖先缓存失效
                mark_dirty_subtree(rt, id);
                mark_draw_invalid_path(rt, id);
                return true;
            }
            cur = rt.nodes[id.index()].parent;
        }
        false
    })
}

thread_local! {
    /// 视口尺寸（平台每帧注入，浮层居中等场景需要绝对定位信息）。
    static VIEWPORT: Cell<(f32, f32)> = const { Cell::new((800.0, 600.0)) };
}

/// 注入视口尺寸（平台后端在布局前调用）。
pub fn set_viewport(width: f32, height: f32) {
    VIEWPORT.with(|v| v.set((width, height)));
}

/// 当前视口尺寸（组合期定位浮层用）。
pub fn viewport_size() -> (f32, f32) {
    VIEWPORT.with(|v| v.get())
}

/// 更新滚动内容尺寸（测量后由容器自记）。
pub(crate) fn update_scroll_content(rt: &mut Rt, id: NodeId) {
    let info = rt.nodes[id.index()]
        .scroll
        .as_ref()
        .map(|s| (s.vertical, s.offset, s.sticky));
    let Some((vertical, offset, sticky)) = info else {
        return;
    };
    let children = rt.nodes[id.index()].children.clone();
    let mut max_main = 0.0f32;
    for chid in children {
        let cn = &rt.nodes[chid.index()];
        let main = if vertical {
            cn.y + cn.height
        } else {
            cn.x + cn.width
        };
        max_main = max_main.max(main);
    }
    let viewport = if vertical {
        rt.nodes[id.index()].height
    } else {
        rt.nodes[id.index()].width
    };
    // sticky 子节点几何（内容坐标系；先取出，避免与 scroll 的可变借用冲突）
    let sticky_box = sticky.map(|sid| {
        let cn = &rt.nodes[sid.index()];
        if vertical {
            (cn.y, cn.height)
        } else {
            (cn.x, cn.width)
        }
    });
    // 增量：滚动偏移变化 → 后代绝对坐标变化，标记自身+子树+祖先脏（否则内容会被「跳过」而陈旧）。
    // 注意：mark 调用必须在 `s` 可变借用释放后进行，避免同时持有两处 rt.nodes 可变借用。
    let prev = rt.nodes[id.index()].last_scroll;
    let mut mark = false;
    let new_off;
    {
        let s = rt.nodes[id.index()].scroll.as_mut().unwrap();
        s.content_size = max_main;
        let max_off = (max_main - viewport).max(0.0);
        let base = offset.clamp(0.0, max_off);
        if (base - prev).abs() > 1e-3 {
            mark = true;
        }
        let mut off = base;
        // 保持 sticky 可见：越出视口则最小幅度校正偏移（仅本次生效，随后清除）
        if let Some((pos, len)) = sticky_box {
            if pos < off {
                off = pos;
            } else if pos + len > off + viewport {
                off = pos + len - viewport;
            }
            if (off - base).abs() > 1e-3 {
                mark = true;
            }
            s.sticky = None;
        }
        s.offset = off;
        new_off = off;
    }
    rt.nodes[id.index()].last_scroll = new_off;
    if mark {
        mark_dirty_path(rt, id);
        mark_dirty_subtree(rt, id);
    }
}

// ══════════════════════════════ 基础容器 widget ══════════════════════════════

/// 创建根节点（Composition::new 用；无父挂载）。
pub fn create_root_node(alignment: Alignment) -> NodeId {
    add_node(Policy::Box(alignment), Modifier::new())
}

/// `Box { content }` — Compose `Box`。
pub fn Box(modifier: Modifier, alignment: Alignment, content: impl Fn() + 'static) {
    let id = add_node(Policy::Box(alignment), modifier);
    with_content_scope(id, content);
}

/// `Column { content }` — Compose `Column`。
pub fn Column(
    modifier: Modifier,
    arrangement: Arrangement,
    cross: CrossAxisAlignment,
    content: impl Fn() + 'static,
) {
    let id = add_node(Policy::Column { arrangement, cross }, modifier);
    with_content_scope(id, content);
}

/// `Row { content }` — Compose `Row`。
pub fn Row(
    modifier: Modifier,
    arrangement: Arrangement,
    cross: CrossAxisAlignment,
    content: impl Fn() + 'static,
) {
    let id = add_node(Policy::Row { arrangement, cross }, modifier);
    with_content_scope(id, content);
}

/// `Spacer(modifier)` — Compose `Spacer`。
pub fn Spacer(modifier: Modifier) {
    add_node(Policy::Leaf, modifier);
}

/// `Canvas(modifier) { draw }` — Compose `Canvas`（DrawScope 闭包，本地坐标 + 节点尺寸）。
pub fn Canvas(modifier: Modifier, draw: impl Fn(&mut DrawBuilder, f32, f32, f32, f32) + 'static) {
    let id = add_node(Policy::Leaf, modifier);
    crate::runtime::with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(draw));
    });
}

/// `clickable { content }` — 可点击容器（Compose `Modifier.clickable` 的容器化）。
pub fn Clickable(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    let id = add_node(Policy::Box(Alignment::TopStart), modifier);
    crate::runtime::with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(on_click));
    });
    with_content_scope(id, content);
}

/// `focusable { content }` — 可点击且可接收键盘导航的容器。
///
/// 点击或显式 `set_focus` 后，方向键会派发给 `on_key`（take-put 防重入）。
/// `on_char` 可选：焦点作用域收到字符时调用（逐字搜索），返回 `None` 表示不处理字符。
/// 返回其所属作用域 `ScopeId`，供调用方在组合外持久持有焦点身份。
pub fn focusable(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    on_key: impl FnMut(crate::runtime::NavKey) + 'static,
    content: impl Fn() + 'static,
) -> crate::runtime::ScopeId {
    focusable_full(modifier, on_click, on_key, None::<fn(char)>, content)
}

/// `focusable` 的完整形态：附带可选字符处理器（逐字搜索 type-ahead）。
pub fn focusable_full(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    on_key: impl FnMut(crate::runtime::NavKey) + 'static,
    on_char: Option<impl FnMut(char) + 'static>,
    content: impl Fn() + 'static,
) -> crate::runtime::ScopeId {
    let id = add_node(Policy::Box(Alignment::TopStart), modifier);
    crate::runtime::with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(on_click));
        rt.nodes[id.index()].key_handler = Some(Box::new(on_key));
        if let Some(h) = on_char {
            rt.nodes[id.index()].char_handler = Some(Box::new(h));
        }
    });
    let sid = with_content_scope(id, content);
    sid
}

/// `hoverable { content }` — 悬停容器：鼠标进入/离开时回调（Compose `Modifier.hoverable`）。
pub fn hoverable(
    modifier: Modifier,
    on_hover: impl FnMut(bool) + 'static,
    content: impl Fn() + 'static,
) {
    let id = add_node(Policy::Box(Alignment::TopStart), modifier);
    crate::runtime::with_rt_pub(|rt| {
        rt.nodes[id.index()].hover_handler = Some(Box::new(on_hover));
    });
    with_content_scope(id, content);
}

/// 键盘导航派发：把方向键交给当前焦点作用域的节点处理器。
/// 处理器只应修改状态（失效读者会触发重组），不应同步重排整棵树。
pub fn dispatch_key(key: crate::runtime::NavKey) -> bool {
    let target = crate::runtime::with_rt_pub(|rt| {
        let sid = rt.focused_scope?;
        rt.nodes.iter().position(|n| n.owner_scope == Some(sid))
    });
    let Some(id) = target else { return false };
    let handler = crate::runtime::with_rt_pub(|rt| rt.nodes[id].key_handler.take());
    match handler {
        Some(mut h) => {
            h(key);
            crate::runtime::with_rt_pub(|rt| rt.nodes[id].key_handler = Some(h));
            true
        }
        None => false,
    }
}

/// 悬停离开：鼠标移出所有命中区（如离开窗口）时结束当前悬停。
pub fn dispatch_hover_left() -> bool {
    let prev = crate::runtime::with_rt_pub(|rt| rt.hovered_node.take());
    let Some(p) = prev else { return false };
    let h = crate::runtime::with_rt_pub(|rt| rt.nodes[p].hover_handler.take());
    if let Some(mut f) = h {
        f(false);
        crate::runtime::with_rt_pub(|rt| rt.nodes[p].hover_handler = Some(f));
    }
    true
}

/// 字符派发：把字符交给当前焦点作用域的节点（逐字搜索）。
/// 返回是否被消费；未消费时平台应回落到普通文本输入路由。
pub fn dispatch_char(ch: char) -> bool {
    let target = crate::runtime::with_rt_pub(|rt| {
        let sid = rt.focused_scope?;
        rt.nodes.iter().position(|n| n.owner_scope == Some(sid))
    });
    let Some(id) = target else { return false };
    let handler = crate::runtime::with_rt_pub(|rt| rt.nodes[id].char_handler.take());
    match handler {
        Some(mut h) => {
            h(ch);
            crate::runtime::with_rt_pub(|rt| rt.nodes[id].char_handler = Some(h));
            true
        }
        None => false,
    }
}

/// 悬停派发：mouse move 命中新目标时，沿祖先链找最近带 hover_handler 的节点。
/// 与上一悬停节点不同则依次回调 left(true→false)/entered(false→true)。
/// 处理器在 runtime 借用外执行（take-put 防重入）。
pub fn dispatch_hover_at(target: NodeId) -> bool {
    let new_hover = crate::runtime::with_rt_pub(|rt| {
        let mut cur = Some(target);
        while let Some(id) = cur {
            if rt.nodes[id.index()].hover_handler.is_some() {
                return Some(id.index());
            }
            cur = rt.nodes[id.index()].parent;
        }
        None
    });
    let prev = crate::runtime::with_rt_pub(|rt| std::mem::replace(&mut rt.hovered_node, new_hover));
    if new_hover == prev {
        return false;
    }
    // 先离开旧节点，再进入新节点（处理器在借用外执行）。
    if let Some(p) = prev {
        let h = crate::runtime::with_rt_pub(|rt| rt.nodes[p].hover_handler.take());
        if let Some(mut f) = h {
            f(false);
            crate::runtime::with_rt_pub(|rt| rt.nodes[p].hover_handler = Some(f));
        }
    }
    if let Some(n) = new_hover {
        let h = crate::runtime::with_rt_pub(|rt| rt.nodes[n].hover_handler.take());
        if let Some(mut f) = h {
            f(true);
            crate::runtime::with_rt_pub(|rt| rt.nodes[n].hover_handler = Some(f));
        }
    }
    true
}

/// `Layout(measurePolicy) { content }` — Compose 自定义布局原语。
pub fn layout(modifier: Modifier, policy: Box<dyn MeasurePolicy>, content: impl Fn() + 'static) {
    let id = add_node(Policy::Custom(policy), modifier);
    with_content_scope(id, content);
}

// Rect / Offset re-exports
pub use crate::foundation::{Offset as UiOffset, Rect as UiRect, Size as UiSize};
