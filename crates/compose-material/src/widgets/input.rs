//! 选择与输入控件 — Checkbox / Radio / Switch / Slider / TextField …

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use compose::animation::{animate_color_as_state, animate_float_as_state, animate_progress_on};
use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Clickable, Column, CrossAxisAlignment, DrawBuilder, Modifier, NodeId,
    Policy, Row, Spacer, TextStyle, add_node, begin_drag_2d, begin_drag_session, last_click_local,
    last_pointer_global, measure_text_with_measurer, node_abs_rect, with_content_scope,
};
use compose::material::{ColorScheme, IconName, glass_spec, icon, theme_colors};
use compose::runtime::{
    MutableState, NavKey, ScopeId, add_frame_driver, disposable_effect, mutableStateOf, remember,
    remove_frame_driver, state_id, with_rt_pub,
};

use crate::tokens;
use crate::widgets::overlay::RememberOverlayKey;
// Quill 富文本编辑器的路由入口（与 RichTextEdit 内部的 rich_* 重名，故别名 rte_*）。
use crate::widgets::rich_text_editor::{
    rich_backspace as rte_backspace, rich_char, rich_commit, rich_copy as rte_copy,
    rich_cut as rte_cut, rich_editor_blur, rich_editor_focused, rich_enter,
    rich_find_next, rich_key as rte_key, rich_paste as rte_paste, rich_redo, rich_scroll,
    rich_select_all as rte_select_all, rich_undo,
};
use crate::widgets::{GlassOrBg, Overlay, RotatingChevron, Text, VerticalScroll};

// ══════════════════════════════ TextField 焦点 ══════════════════════════════

thread_local! {
    // 焦点身份绑定在**状态句柄**上（重组会重建节点、NodeId 必然变化，见 `add_node`），
    // 因此用稳定的 state key 标识当前输入框，避免一次重组后焦点即失效。
    static FOCUSED_FIELD: std::cell::Cell<Option<MutableState<String>>> =
        const { std::cell::Cell::new(None) };
    static PREEDIT: RefCell<String> = RefCell::new(String::new());
    /// OTP 定位写入：`state key → (焦点格索引, 格子数, 行节点)`。
    /// 焦点位用非响应式 `Rc<Cell>` 保存，便于在 draw 闭包里实时读取（不触发 RUNTIME 重入）；
    /// 行节点随每次组合刷新，焦点位变化时由输入函数直接失效绘制
    /// （光标只在闪烁相位翻转时重绘，不能等下一次相位唤醒才反映焦点移动）。
    static OTP_INPUT: RefCell<
        HashMap<u32, (std::rc::Rc<std::cell::Cell<usize>>, usize, std::rc::Rc<std::cell::Cell<NodeId>>)>,
    > = RefCell::new(HashMap::new());
    /// 普通输入框的光标位置（字符索引）：`state key → 光标索引`，支持点击落到任意字符处。
    static FIELD_CARET: RefCell<HashMap<u32, MutableState<usize>>> = RefCell::new(HashMap::new());
    /// 各输入框所在的组合作用域：`state key → scope`。焦点本身存在线程局部里，
    /// 读它不会建立依赖，所以焦点变化必须自己调度新旧两个作用域重组。
    static FIELD_SCOPE: RefCell<HashMap<u32, ScopeId>> = RefCell::new(HashMap::new());
}

/// 是否存在获得焦点的 TextField（平台据此决定字符路由到输入框还是逐字搜索）。
pub fn InputFocused() -> bool {
    FOCUSED_FIELD.with(|f| f.get().is_some())
}

/// 当前聚焦字段是否就是 `st`（绘制时判断是否画光标）。
fn is_focused(st: MutableState<String>) -> bool {
    FOCUSED_FIELD.with(|f| {
        f.get()
            .map(state_id)
            .map(|k| k == state_id(st))
            .unwrap_or(false)
    })
}

/// 切换文本字段焦点，并让受影响的字段重组。
///
/// `FOCUSED_FIELD` 是线程局部而非响应式状态：描边 2dp、标签上浮这些**组合期**读到的
/// 焦点值不会自动失效，不手动调度的话，聚焦后要等第一次按键（光标状态变化）才换成
/// 聚焦外观，失焦后更是长期停在 2dp 上。
fn set_field_focus(next: Option<MutableState<String>>) {
    let prev = FOCUSED_FIELD.with(|f| f.replace(next));
    let same = match (prev, next) {
        (Some(a), Some(b)) => state_id(a) == state_id(b),
        (None, None) => true,
        _ => false,
    };
    if same {
        return;
    }
    for st in [prev, next].into_iter().flatten() {
        let key = state_id(st);
        let sid = FIELD_SCOPE.with(|m| m.borrow().get(&key).copied());
        match sid {
            Some(sid) if compose::runtime::scope_alive(sid) => compose::runtime::schedule_scope(sid),
            // 作用域已销毁（字段被条件移除）：顺手清掉登记，状态句柄会被复用
            Some(_) => {
                FIELD_SCOPE.with(|m| m.borrow_mut().remove(&key));
            }
            None => {}
        }
    }
}

/// 焦点格变化后失效 OTP 行（含格子子树）。光标闪烁只在相位翻转时重绘，
/// 若等下一次相位唤醒，方向键移动光标最多要 530ms 才可见。
fn otp_invalidate(key: u32) {
    let node = OTP_INPUT.with(|m| m.borrow().get(&key).map(|(_, _, n)| n.get()));
    if let Some(id) = node {
        compose::layout::invalidate_draw_node(id);
    }
}

/// 字段的光标索引状态（无登记时返回 None，退化为「末尾」语义）。
fn field_caret(value: MutableState<String>) -> Option<MutableState<usize>> {
    FIELD_CARET.with(|m| m.borrow().get(&state_id(value)).copied())
}

/// 注册（并记住）字段光标索引状态；组合期读取即建立依赖，点击改索引会自动重组。
fn remember_caret(value: MutableState<String>) -> MutableState<usize> {
    let caret = remember(|| mutableStateOf(0usize));
    FIELD_CARET.with(|m| m.borrow_mut().insert(state_id(value), caret));
    caret
}

/// 按点击的横向偏移求字符索引：取距离最近的字符边界（点击文字中部即落到该处）。
fn caret_index_at(text: &str, dx: f32) -> usize {
    let chars: Vec<char> = text.chars().collect();
    // 与字段绘制同一排版（`bodyLarge`），否则点击定位的字符边界与画出来的字对不上。
    let style = field_style(Color::BLACK);
    let mut prev = 0.0f32;
    for (i, _) in chars.iter().enumerate() {
        let prefix: String = chars[..=i].iter().collect();
        let (w, _) = measure_text_with_measurer(&prefix, &style);
        if dx < (prev + w) * 0.5 {
            return i;
        }
        prev = w;
    }
    chars.len()
}

/// 平台键盘字符输入 → 聚焦的 TextField（OTP 为定位写入）。
pub fn InputChar(ch: char) {
    if rich_editor_focused() {
        rich_char(ch);
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_insert_char(st, ch);
        return;
    }
    let Some(st) = FOCUSED_FIELD.with(|f| f.get()) else {
        return;
    };
    let otp = OTP_INPUT.with(|m| m.borrow().get(&state_id(st)).cloned());
    if let Some((sel, len, _)) = otp {
        let i = sel.get().min(len.saturating_sub(1));
        let mut v: Vec<char> = st.read_clone().chars().collect();
        if i < v.len() {
            v[i] = ch; // 覆盖该格（写入定位格）
        } else if v.len() < len {
            v.push(ch); // 追加到末尾（含点击靠后时的补位）
        } else {
            return;
        }
        st.set(v.into_iter().collect());
        // 自动前进：(idx+1).min(len)（填满后焦点停在末尾之外）
        sel.set((i + 1).min(len));
        otp_invalidate(state_id(st));
        return;
    }
    // 普通输入框：插入到光标处（缺省末尾即追加）
    match field_caret(st) {
        Some(caret) => {
            let mut v: Vec<char> = st.read_clone().chars().collect();
            let i = caret.get().min(v.len());
            v.insert(i, ch);
            st.set(v.into_iter().collect());
            caret.set(i + 1);
        }
        None => st.update(|s| s.push(ch)),
    }
}

/// 平台退格（OTP 删除选中位；光标在末尾时删除前一位）。
pub fn InputBackspace() {
    if rich_editor_focused() {
        rte_backspace();
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_backspace_char(st);
        return;
    }
    let Some(st) = FOCUSED_FIELD.with(|f| f.get()) else {
        return;
    };
    let otp = OTP_INPUT.with(|m| m.borrow().get(&state_id(st)).cloned());
    if let Some((sel, len, _)) = otp {
        let i = sel.get().min(len.saturating_sub(1));
        let mut v: Vec<char> = st.read_clone().chars().collect();
        if i < v.len() {
            // 当前格有字：删除该格，光标停在此格
            v.remove(i);
            st.set(v.into_iter().collect());
            sel.set(i);
        } else if i > 0 {
            // 当前格空：删除前一格并后退
            v.remove(i - 1);
            st.set(v.into_iter().collect());
            sel.set(i - 1);
        } else {
            return;
        }
        otp_invalidate(state_id(st));
        return;
    }
    // 普通输入框：删除光标前一位
    match field_caret(st) {
        Some(caret) => {
            let mut v: Vec<char> = st.read_clone().chars().collect();
            let i = caret.get().min(v.len());
            if i == 0 {
                return;
            }
            v.remove(i - 1);
            st.set(v.into_iter().collect());
            caret.set(i - 1);
        }
        None => st.update(|s| {
            s.pop();
        }),
    }
}

/// 平台导航键 → 聚焦的 OTP 左右移动光标（其余键忽略，不影响列表等 scope 导航）。
pub fn InputKey(key: NavKey) {
    // Quill 富文本编辑器：完整方向键 / 选区 / Home/End/Delete
    if rich_editor_focused() {
        rte_key(key);
        return;
    }
    // 富文本：完整方向键 / 选区 / Home/End/Delete
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_key(st, key);
        return;
    }
    let Some(st) = FOCUSED_FIELD.with(|f| f.get()) else {
        return;
    };
    let otp = OTP_INPUT.with(|m| m.borrow().get(&state_id(st)).cloned());
    if let Some((sel, len, _)) = otp {
        match key {
            NavKey::Left => {
                let i = sel.get();
                if i > 0 {
                    sel.set(i - 1);
                    otp_invalidate(state_id(st));
                }
            }
            NavKey::Right => {
                let i = sel.get();
                if i + 1 < len {
                    sel.set(i + 1);
                    otp_invalidate(state_id(st));
                }
            }
            _ => {}
        }
    } else {
        // 普通单行 TextField：补上光标移动
        match key {
            NavKey::Left => {
                if let Some(caret) = field_caret(st) {
                    let v = st.read_clone().chars().count();
                    caret.set(caret.get().saturating_sub(1).min(v));
                }
            }
            NavKey::Right => {
                if let Some(caret) = field_caret(st) {
                    let v = st.read_clone().chars().count();
                    caret.set((caret.get() + 1).min(v));
                }
            }
            NavKey::Home => {
                if let Some(caret) = field_caret(st) {
                    caret.set(0);
                }
            }
            NavKey::End => {
                if let Some(caret) = field_caret(st) {
                    caret.set(st.read_clone().chars().count());
                }
            }
            _ => {}
        }
    }
}

/// 平台 IME 提交文本 → 聚焦 TextField（整段写入，并清除候选 preedit）。
pub fn InputCommit(s: &str) {
    if rich_editor_focused() {
        rich_commit(s);
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_insert_text(st, s);
        return;
    }
    if let Some(st) = FOCUSED_FIELD.with(|f| f.get()) {
        // 提交文本写到光标处（而非一律末尾），并把光标移到提交文本之后；
        // 否则中文（IME）提交后文本追加在末尾而光标不动，表现为「光标没后移」。
        match field_caret(st) {
            Some(caret) => {
                let mut v: Vec<char> = st.read_clone().chars().collect();
                let i = caret.get().min(v.len());
                let ins: Vec<char> = s.chars().collect();
                v.splice(i..i, ins);
                st.set(v.into_iter().collect());
                caret.set(i + s.chars().count());
            }
            None => st.update(|v| v.push_str(s)),
        }
    }
    PREEDIT.with(|p| p.borrow_mut().clear());
}

/// 平台 IME 候选（preedit）预览 → 仅显示，不落定。
pub fn InputPreedit(s: &str) {
    PREEDIT.with(|p| *p.borrow_mut() = s.to_string());
}

/// 登记本字段所在的组合作用域（焦点变化时要用它调度重组）——每次组合刷新都要重跑，
/// 作用域换代后旧句柄已失效。
fn register_field_scope(value: MutableState<String>) {
    if let Some(sid) = compose::runtime::current_scope_id() {
        FIELD_SCOPE.with(|m| m.borrow_mut().insert(state_id(value), sid));
    }
}

/// 绑定文本值 + 点击聚焦；`pad_l` 为绘制文本相对节点左边的内缩（用于把点击 x 换算成字符索引）。
fn bind_text_value(id: NodeId, value: MutableState<String>, pad_l: f32) {
    register_field_scope(value);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].text_value = Some(value);
        rt.nodes[id.index()].on_click = Some(Box::new(move || {
            set_field_focus(Some(value));
            FOCUSED_RICH.with(|r| r.set(None));
            PREEDIT.with(|p| p.borrow_mut().clear());
            // 点击定位光标：按点击处的字符边界确定索引（点击文字末尾右侧 → 末尾）
            if let Some(caret) = field_caret(value) {
                let (lx, _) = last_click_local();
                caret.set(caret_index_at(&value.read_clone(), (lx - pad_l).max(0.0)));
            }
        }));
    });
}

/// 聚焦期间持续请求帧，使光标闪烁。
///
/// 不续帧的话，输入一次只重绘一帧：若该帧恰好落在闪烁的「灭」相位，
/// 光标就一直不显示，直到下次事件（点击）才重绘——表现为「输入后光标不见了」。
///
/// 增量绘制下，节点默认会被缓存、跳过重绘 → 光标会冻结。故只在**相位翻转**的帧
/// 把该字段节点（含子树与祖先）标绘制失效，并请求「下一次翻转」的定点唤醒。
fn caret_blink_driver(value: MutableState<String>, id: NodeId) {
    blink_caret_while(id, move || is_focused(value));
}

/// 光标闪烁间隔（毫秒）：与 `draw_field` 里 `(ms / 530) % 2` 保持同一节奏。
const CARET_BLINK_MS: u64 = 530;

fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 聚焦期间按闪烁相位**只在翻转时**失效节点，并定点唤醒下一帧。
///
/// 对齐 Compose 光标闪烁（`withFrameNanos` 只在动画帧被调度）：聚焦不等于持续 60fps
/// 重绘。旧实现每帧都 `request_anim_frames()` + 失效，输入框一聚焦就让整个应用回到
/// 满帧率——本项目「静止即停驱」的性能不变式在聚焦路径上被绕开了。
///
/// 帧驱动只注册一次，节点 id 与相位状态放在 `remember` 容器里每次重组刷新
/// （避免驱动捕获陈旧 `NodeId` 后永久冻结光标）。
pub(crate) fn blink_caret_while(id: NodeId, active: impl Fn() -> bool + 'static) {
    let slot = remember(|| std::rc::Rc::new(std::cell::Cell::new(Some(id))));
    slot.set(Some(id));
    // (相位序号, 上一帧是否聚焦)：失去焦点后重置，重新聚焦当帧立即重绘。
    let state = remember(|| std::rc::Rc::new(std::cell::Cell::new((u64::MAX, false))));
    let drv = remember(move || {
        let c = slot.clone();
        let st = state.clone();
        add_frame_driver(move || {
            let on = active();
            let (last_phase, was_on) = st.get();
            if !on {
                if was_on {
                    // 失去焦点当帧重绘一次，抹掉已画出的光标/聚焦框
                    if let Some(i) = c.get() {
                        compose::layout::invalidate_draw_node(i);
                    }
                    st.set((u64::MAX, false));
                }
                // 未聚焦：直接返回，不取墙钟（避免每个空闲输入框每帧一次系统调用）
                return;
            }
            let now_ms = wall_clock_ms();
            let phase = now_ms / CARET_BLINK_MS;
            if !was_on || phase != last_phase {
                if let Some(i) = c.get() {
                    compose::layout::invalidate_draw_node(i);
                }
            }
            st.set((phase, true));
            // 下一次亮/灭翻转的定点唤醒（不逐帧请求渲染）。
            let wait_ms = CARET_BLINK_MS - now_ms % CARET_BLINK_MS;
            compose::animation::request_anim_frame_after(wait_ms as f32 / 1000.0);
        })
    });
    // 控件消失（焦点输入框被条件移除 / 页面切走）时注销驱动：否则驱动常驻，
    // 每帧遍历一条已无人认领的闭包，且其 `Rc` 链一直活着。
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
}

/// 输入框内容排版（M3 = `bodyLarge` 16/24）。
fn field_style(color: Color) -> TextStyle {
    tokens::body_large(color)
}

/// M3 输入框内容绘制。`lt` = 标签上浮进度（0=居中当占位，1=顶行小标签），由**组合期**动画给出，
/// 绘制闭包只读值 —— 在闭包里起动画会踩「绘制阶段禁止改状态」这条线。
fn draw_field(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    h: f32,
    pad_l: f32,
    shown: &str,
    ph: &str,
    c: &compose::material::ColorScheme,
    fid: MutableState<String>,
    caret_idx: usize,
    lt: f32,
) {
    let preedit = PREEDIT.with(|p| p.borrow().clone());
    let focused = is_focused(fid);
    // 文本在字段内垂直居中（用 "Ag" 探针取行高，避免空文本时高度为 0 导致光标不可见）
    let style = field_style(c.on_surface);
    let (_, th) = measure_text_with_measurer("Ag", &style);
    // M3：标签「聚焦或有内容」即上浮，正文同步从居中滑到值带 —— 共用一条进度，
    // 否则首字符落下时正文会跳一行。
    let rest_y = y + ((h - th) / 2.0).max(0.0);
    let ty = rest_y + (y + tokens::FIELD_VALUE_TOP + (tokens::FIELD_VALUE_H - th) * 0.5 - rest_y) * lt;
    let tx = x + pad_l; // 自定义绘制拿到的是含 padding 的整框，需自行内缩
    if lt > 0.001 {
        let lc = if focused { c.primary } else { c.on_surface_variant };
        let lc = lc.with_alpha(lc.a * lt);
        let sty = tokens::lerp_role(&field_style(lc), &tokens::label_small(lc), lt, lc);
        let top = y + tokens::FIELD_LABEL_TOP;
        b.draw_text(tx, rest_y + (top - rest_y) * lt, ph, &sty, f32::INFINITY);
    }
    if shown.is_empty() && preedit.is_empty() {
        // 占位就是标签本体：浮起后由上面的上浮标签接手，这里按 `1-lt` 淡出，避免同一句话画两遍
        let po = c.on_surface_variant.with_alpha(c.on_surface_variant.a * (1.0 - lt));
        if lt < 0.999 {
            b.draw_text(tx, ty, ph, &field_style(po), f32::INFINITY);
        }
    } else {
        b.draw_text(tx, ty, shown, &style, f32::INFINITY);
        if !preedit.is_empty() {
            let (sw, _) = measure_text_with_measurer(shown, &style);
            b.draw_text(
                tx + sw,
                ty,
                &preedit,
                &field_style(c.primary),
                f32::INFINITY,
            );
        }
    }
    if focused {
        // 光标画在「光标索引」处（而非一律文本末尾）：按前缀宽度定位
        let chars: Vec<char> = shown.chars().collect();
        let i = caret_idx.min(chars.len());
        let prefix: String = chars[..i].iter().collect();
        let (pw, _) = measure_text_with_measurer(&prefix, &style);
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        if (ms / 530) % 2 == 0 {
            // M3 光标：2dp 宽、整行高（24dp）
            let lh = style.line_height.unwrap_or(th);
            b.fill_rect(tx + pw + 1.0, ty + (th - lh) * 0.5, 2.0, lh, c.primary);
        }
    }
}

/// 输入框水平内边距（自定义绘制不自动内缩 padding，绘制时需显式加上）。
const FIELD_PAD_L: f32 = tokens::FIELD_PAD_H;

/// 无外观输入框（SearchBar / ComboBox / TokenInput 内部复用）。
pub fn BareTextField(value: MutableState<String>, placeholder: &str, modifier: Modifier) {
    let colors = theme_colors();
    let shown = value.read_clone();
    let ph = placeholder.to_string();
    let caret = remember_caret(value);
    let caret_idx = caret.get();
    let id = add_node(Policy::Box(Alignment::CenterStart), modifier.height(24.0));
    bind_text_value(id, value, 0.0);
    caret_blink_driver(value, id);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, h| {
            draw_field(
                b,
                x,
                y,
                h,
                FIELD_PAD_L,
                &shown,
                &ph,
                &colors,
                value,
                caret_idx,
                0.0,
            );
        }));
    });
}

/// `TextField(value, placeholder)` — Compose `TextField`（M3 filled：`surface_variant`
/// 底 + 4dp 圆角 + 底部指示条，聚焦 2dp `primary`）。
/// 第二个参数是 **label**：有内容时上浮到顶行，空且未聚焦时充当占位文本。
pub fn TextField(value: MutableState<String>, placeholder: &str, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let focused = is_focused(value);
    let shown = value.read_clone();
    let spec = tokens::state_spec();
    let (tw, tc) = field_indicator(&colors, focused);
    let ind_w = animate_float_as_state(tw, spec);
    let ind_c = animate_color_as_state(tc, spec);
    let lt = animate_float_as_state(
        if focused || !shown.is_empty() {
            1.0
        } else {
            0.0
        },
        spec,
    );
    let m = GlassOrBg(modifier, g, colors.surface_variant, tokens::FIELD_RADIUS)
        .padding_ltrb(FIELD_PAD_L, 0.0, FIELD_PAD_L, 0.0)
        .height(tokens::FIELD_HEIGHT);
    let id = add_node(Policy::Box(Alignment::CenterStart), m);
    caret_blink_driver(value, id);
    let caret_idx = remember_caret(value).get();
    let ph = placeholder.to_string();
    bind_text_value(id, value, FIELD_PAD_L);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            b.fill_rect(x, y + h - ind_w, w, ind_w, ind_c);
            draw_field(
                b,
                x,
                y,
                h - ind_w,
                FIELD_PAD_L,
                &shown,
                &ph,
                &colors,
                value,
                caret_idx,
                lt,
            );
        }));
    });
}

/// M3 输入框指示条：未聚焦 1dp `on_surface_variant` / 聚焦 2dp `primary`。
fn field_indicator(
    colors: &compose::material::ColorScheme,
    focused: bool,
) -> (f32, compose::foundation::Color) {
    if focused {
        (tokens::OUTLINE_W_FOCUSED, colors.primary)
    } else {
        (tokens::OUTLINE_W, colors.on_surface_variant)
    }
}

/// `OutlinedTextField(value, label)` — Compose `OutlinedTextField`（1dp 全边框，聚焦
/// 2dp `primary`；标签在聚焦或有内容时上浮到顶行，与正文同步滑到位）。
pub fn OutlinedTextField(value: MutableState<String>, label: &str, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let lbl = label.to_string();
    let focused = is_focused(value);
    let shown = value.read_clone();
    let spec = tokens::state_spec();
    let bw = animate_float_as_state(
        if focused {
            tokens::OUTLINE_W_FOCUSED
        } else {
            tokens::OUTLINE_W
        },
        spec,
    );
    let bc = animate_color_as_state(
        if focused {
            colors.primary
        } else {
            colors.outline
        },
        spec,
    );
    let lt = animate_float_as_state(
        if focused || !shown.is_empty() {
            1.0
        } else {
            0.0
        },
        spec,
    );
    let m = GlassOrBg(
        modifier,
        g,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        tokens::FIELD_RADIUS,
    )
    .padding_ltrb(FIELD_PAD_L, 0.0, FIELD_PAD_L, 0.0)
    .height(tokens::FIELD_HEIGHT)
    .border(bw, bc, tokens::FIELD_RADIUS);
    let id = add_node(Policy::Box(Alignment::CenterStart), m);
    caret_blink_driver(value, id);
    let caret_idx = remember_caret(value).get();
    bind_text_value(id, value, FIELD_PAD_L);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, h| {
            draw_field(
                b,
                x,
                y,
                h,
                FIELD_PAD_L,
                &shown,
                &lbl,
                &colors,
                value,
                caret_idx,
                lt,
            );
        }));
    });
}

/// `PasswordField(value, label)` — 掩码输入（与 `TextField` 同款 M3 filled 外框，
/// 聚焦或有内容时标签同样上浮，指示条 1dp ⇄ 2dp 插值）。
pub fn PasswordField(value: MutableState<String>, label: &str, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let focused = is_focused(value);
    let raw = value.read_clone();
    let spec = tokens::state_spec();
    let (tw, tc) = field_indicator(&colors, focused);
    let ind_w = animate_float_as_state(tw, spec);
    let ind_c = animate_color_as_state(tc, spec);
    let lt = animate_float_as_state(
        if focused || !raw.is_empty() {
            1.0
        } else {
            0.0
        },
        spec,
    );
    let m = GlassOrBg(modifier, g, colors.surface_variant, tokens::FIELD_RADIUS)
        .padding_ltrb(FIELD_PAD_L, 0.0, FIELD_PAD_L, 0.0)
        .height(tokens::FIELD_HEIGHT);
    let id = add_node(Policy::Box(Alignment::CenterStart), m);
    caret_blink_driver(value, id);
    let empty = raw.is_empty();
    let masked: String = std::iter::repeat_n('•', raw.chars().count()).collect();
    let lbl = label.to_string();
    let caret_idx = remember_caret(value).get();
    bind_text_value(id, value, FIELD_PAD_L);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            b.fill_rect(x, y + h - ind_w, w, ind_w, ind_c);
            let style = field_style(colors.on_surface);
            let th = style.line_height.unwrap_or(tokens::FIELD_VALUE_H);
            let tx = x + FIELD_PAD_L;
            // 与 `draw_field` 同一套插值：正文从居中滑到值带，标签同时上浮 + 缩字号。
            let inner_h = h - ind_w;
            let rest_y = y + ((inner_h - th) / 2.0).max(0.0);
            let ty = rest_y
                + (y + tokens::FIELD_VALUE_TOP + (tokens::FIELD_VALUE_H - th) * 0.5 - rest_y) * lt;
            if lt > 0.001 {
                let lc = if focused {
                    colors.primary
                } else {
                    colors.on_surface_variant
                };
                let lc = lc.with_alpha(lc.a * lt);
                let sty = tokens::lerp_role(&field_style(lc), &tokens::label_small(lc), lt, lc);
                let top = y + tokens::FIELD_LABEL_TOP;
                b.draw_text(tx, rest_y + (top - rest_y) * lt, &lbl, &sty, f32::INFINITY);
            }
            if empty {
                if lt < 0.999 {
                    let pc = colors
                        .on_surface_variant
                        .with_alpha(colors.on_surface_variant.a * (1.0 - lt));
                    b.draw_text(tx, ty, &lbl, &field_style(pc), f32::INFINITY);
                }
            } else {
                b.draw_text(tx, ty, &masked, &style, f32::INFINITY);
            }
            if focused {
                // 掩码文本按光标索引定位（圆点等宽，取前缀字符数即可）
                let mc: Vec<char> = masked.chars().collect();
                let prefix: String = mc[..caret_idx.min(mc.len())].iter().collect();
                let (pw, _) = measure_text_with_measurer(&prefix, &style);
                let ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                if (ms / 530) % 2 == 0 {
                    b.fill_rect(tx + pw + 1.0, ty, 2.0, th, colors.primary);
                }
            }
        }));
    });
}

/// `FormField(label, error) { content }` — 标签 + 控件 + 错误提示（M3 支撑文本）。
pub fn FormField(
    label: &str,
    error: Option<String>,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let lbl = label.to_string();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            let colors = theme_colors();
            Text(
                lbl.clone(),
                tokens::label_medium(colors.on_surface_variant),
                Modifier::new().padding_ltrb(0.0, 0.0, 0.0, tokens::SPACE_XS),
            );
            content();
            if let Some(e) = error.clone() {
                Text(
                    e,
                    tokens::body_small(colors.error),
                    Modifier::new().padding_ltrb(0.0, tokens::SPACE_XS, 0.0, 0.0),
                );
            }
        },
    );
}

// ══════════════════════════════ 选择控件 ══════════════════════════════

/// M3 选择控件的 40dp 触控目标 + 圆形状态层（悬停 `on_surface` 8%）。
///
/// 返回选择框本体（18/20dp）居中所需的偏移。
fn select_target(
    modifier: Modifier,
    colors: &compose::material::ColorScheme,
    hovered: bool,
) -> Modifier {
    let m = modifier.size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT);
    if hovered {
        m.background(
            tokens::state_layer(colors.on_surface, tokens::STATE_HOVER),
            tokens::full_radius(tokens::CONTROL_HEIGHT),
        )
    } else {
        m
    }
}

/// `Checkbox(checked)` — Compose `Checkbox`（M3：40dp 目标 + 18dp 方框、2dp 圆角）。
pub fn Checkbox(checked: MutableState<bool>, modifier: Modifier) {
    let colors = theme_colors();
    // 必须**注册**依赖：动画目标由 `is_checked` 推出，不登记则点击后本作用域不重组、
    // `t` 永远停在旧值（勾画不出来）。
    let is_checked = checked.get();
    let hovered = compose::runtime::remember(|| compose::runtime::mutableStateOf(false));
    let spec = tokens::state_spec();
    let t = animate_float_as_state(if is_checked { 1.0 } else { 0.0 }, spec);
    let m = select_target(modifier, &colors, hovered.get());
    let id = add_node(Policy::Leaf, m);
    let st = checked;
    let box_fill = animate_color_as_state(
        if is_checked {
            colors.primary
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        },
        spec,
    );
    let stroke = animate_color_as_state(
        if is_checked {
            colors.primary
        } else {
            colors.on_surface_variant
        },
        spec,
    );
    let mark = colors.on_primary;
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[id.index()];
        n.on_click = Some(Box::new(move || {
            st.update(|v| *v = !*v);
        }));
        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
        n.draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            // 18dp 方框居中于 40dp 目标 → 偏移 11
            let (bx, by) = (
                x + (tokens::CONTROL_HEIGHT - tokens::CHECKBOX_BOX) * 0.5,
                y + (tokens::CONTROL_HEIGHT - tokens::CHECKBOX_BOX) * 0.5,
            );
            let s = tokens::CHECKBOX_BOX;
            b.fill_rrect(bx, by, s, s, 2.0, box_fill);
            b.stroke_rrect(bx, by, s, s, 2.0, stroke, 2.0);
            draw_check(b, bx, by, s, mark, t);
        }));
    });
}

/// M3 勾按 `t`（0→1）**逐段描出**（Flutter `_CheckboxPainter` 用 PathMetrics 裁剪勾的路径）。
/// 拐点前短边占全程 35%，其余给长边；`t <= 0` 时什么都不画。
pub(crate) fn draw_check(b: &mut DrawBuilder, bx: f32, by: f32, s: f32, c: Color, t: f32) {
    if t <= 0.001 {
        return;
    }
    let p = |fx: f32, fy: f32| (bx + s * fx, by + s * fy);
    let (x1, y1) = p(0.23, 0.5);
    let (x2, y2) = p(0.45, 0.72);
    let (x3, y3) = p(0.8, 0.33);
    let f1 = (t / 0.35).min(1.0);
    b.line(x1, y1, x1 + (x2 - x1) * f1, y1 + (y2 - y1) * f1, c, 2.0);
    if f1 >= 1.0 {
        let f2 = ((t - 0.35) / 0.65).min(1.0);
        b.line(x2, y2, x2 + (x3 - x2) * f2, y2 + (y3 - y2) * f2, c, 2.0);
    }
}

/// 三态（用于「全选」等半选场景）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriState {
    Unchecked,
    Checked,
    Indeterminate,
}

/// `TriStateCheckbox(state)` — 未选 / 全选 / 半选（M3 半选 = 主色底 + 横杠）。
pub fn TriStateCheckbox(state: MutableState<TriState>, modifier: Modifier) {
    let colors = theme_colors();
    let cur = state.get();
    let filled = cur != TriState::Unchecked;
    let hovered = compose::runtime::remember(|| compose::runtime::mutableStateOf(false));
    let spec = tokens::state_spec();
    // 勾选进度与「横杠」进度各自插值：全选→半选时勾先收起、横杠再长出（Flutter 同一时序）。
    let t = animate_float_as_state(if filled { 1.0 } else { 0.0 }, spec);
    let dash = animate_float_as_state(if cur == TriState::Indeterminate { 1.0 } else { 0.0 }, spec);
    let m = select_target(modifier, &colors, hovered.get());
    let id = add_node(Policy::Leaf, m);
    let st = state;
    let box_fill = animate_color_as_state(
        if filled {
            colors.primary
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        },
        spec,
    );
    let stroke = animate_color_as_state(
        if filled {
            colors.primary
        } else {
            colors.on_surface_variant
        },
        spec,
    );
    let mark = colors.on_primary;
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[id.index()];
        n.on_click = Some(Box::new(move || {
            st.update(|v| {
                *v = match *v {
                    TriState::Unchecked | TriState::Indeterminate => TriState::Checked,
                    TriState::Checked => TriState::Unchecked,
                }
            });
        }));
        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
        n.draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            let s = tokens::CHECKBOX_BOX;
            let (bx, by) = (
                x + (tokens::CONTROL_HEIGHT - s) * 0.5,
                y + (tokens::CONTROL_HEIGHT - s) * 0.5,
            );
            b.fill_rrect(bx, by, s, s, 2.0, box_fill);
            b.stroke_rrect(bx, by, s, s, 2.0, stroke, 2.0);
            if dash < 0.999 {
                draw_check(b, bx, by, s, mark, t * (1.0 - dash));
            }
            if dash > 0.001 {
                // 横杠从中心向两端长出（半宽 = 0.28s × dash）。
                let half = s * 0.28 * dash * t;
                b.line(
                    bx + s * 0.5 - half,
                    by + s * 0.5,
                    bx + s * 0.5 + half,
                    by + s * 0.5,
                    mark,
                    2.0,
                );
            }
        }));
    });
}

/// `Checkbox + 标签` 一行（整行可点）。
pub fn CheckboxRow(checked: MutableState<bool>, label: &str, modifier: Modifier) {
    let lbl = label.to_string();
    crate::widgets::basic::InkWell(
        modifier.fill_max_width(),
        move || checked.update(|v| *v = !*v),
        move || {
            let l2 = lbl.clone();
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Checkbox(checked, Modifier::new());
                    Spacer(Modifier::new().width(tokens::SPACE_SM));
                    Text(
                        l2.clone(),
                        tokens::body_large(Color::BLACK),
                        Modifier::new(),
                    );
                },
            );
        },
    );
}

/// `RadioGroup(options, selected)` — Compose `RadioButton` 列（M3：40dp 目标 + 20dp 环）。
pub fn RadioGroup(options: &[String], selected: MutableState<usize>, modifier: Modifier) {
    let opts: Vec<String> = options.to_vec();
    let cur = selected.get();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            for (i, opt) in opts.clone().into_iter().enumerate() {
                let on = cur == i;
                let st = selected;
                crate::widgets::basic::InkWell(
                    Modifier::new()
                        .fill_max_width()
                        .padding_hv(0.0, tokens::SPACE_XS),
                    move || st.set(i),
                    move || {
                        let colors = theme_colors();
                        let o2 = opt.clone();
                        Row(
                            Modifier::new(),
                            Arrangement::Start,
                            CrossAxisAlignment::Center,
                            move || {
                                let id = add_node(
                                    Policy::Leaf,
                                    Modifier::new()
                                        .size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
                                );
                                let spec = tokens::state_spec();
                                // M3：中心点从 0 长到 10dp（Flutter `_RadioPainter` 的 scale 动画）。
                                let t = animate_float_as_state(if on { 1.0 } else { 0.0 }, spec);
                                let ring = animate_color_as_state(
                                    if on {
                                        colors.primary
                                    } else {
                                        colors.on_surface_variant
                                    },
                                    spec,
                                );
                                with_rt_pub(|rt| {
                                    rt.nodes[id.index()].draw =
                                        Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                                            let (cx, cy) = (
                                                x + tokens::CONTROL_HEIGHT * 0.5,
                                                y + tokens::CONTROL_HEIGHT * 0.5,
                                            );
                                            let r = tokens::RADIO_DIAMETER * 0.5;
                                            b.stroke_circle(cx, cy, r, ring, 2.0);
                                            if t > 0.001 {
                                                // 选中：外环 primary，中心 10dp 实心点
                                                b.fill_circle(cx, cy, r * 0.5 * t, colors.primary);
                                            }
                                        }));
                                });
                                Spacer(Modifier::new().width(tokens::SPACE_SM));
                                Text(
                                    o2.clone(),
                                    tokens::body_large(colors.on_surface),
                                    Modifier::new(),
                                );
                            },
                        );
                    },
                );
            }
        },
    );
}

/// `Switch(checked)` — Compose `Switch`（M3：52×32 轨道 + 2dp 描边，拇指 16→24dp）。
pub fn Switch(checked: MutableState<bool>, modifier: Modifier) {
    let colors = theme_colors();
    let is_checked = checked.get();
    let hovered = compose::runtime::remember(|| compose::runtime::mutableStateOf(false));
    // M3 动效：位移与配色一起插值。Flutter 用 ~100ms `Curves.fastEaseInToSlowEaseOut`，
    // 本库统一走 `tokens::state_spec()`（short4 + standard）。
    let spec = tokens::state_spec();
    let t = animate_float_as_state(if is_checked { 1.0 } else { 0.0 }, spec);
    // M3：未选中轨道 surfaceContainerHighest + 2dp outline 描边；选中整轨 primary
    let track = animate_color_as_state(
        if is_checked {
            colors.primary
        } else {
            colors.surface_container_highest
        },
        spec,
    );
    let border = animate_color_as_state(
        if is_checked {
            colors.primary
        } else {
            colors.outline
        },
        spec,
    );
    let thumb = animate_color_as_state(
        if is_checked {
            colors.on_primary
        } else {
            colors.outline
        },
        spec,
    );
    let m = modifier
        .size(tokens::SWITCH_W, tokens::SWITCH_H)
        .background(track, tokens::full_radius(tokens::SWITCH_H))
        .border(2.0, border, tokens::full_radius(tokens::SWITCH_H));
    let id = add_node(Policy::Leaf, m);
    let st = checked;
    // 依赖登记必须在 runtime 借用之外（`get()` 内部还要 `borrow_mut`，嵌套即 panic）。
    let is_hover = hovered.get();
    // 勾只在真正选中（t→1）时画全；随 t 淡入，避免切换瞬间「弹」出来。
    let mark_visible = t > 0.02;
    let mark_alpha = (t * 1.4 - 0.4).clamp(0.0, 1.0);
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[id.index()];
        n.on_click = Some(Box::new(move || st.update(|v| *v = !*v)));
        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
        n.draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            // 拇指：未选 16dp 靠左（内缩 6），选中 24dp 靠右（内缩 4）；位移按 t 线性插值。
            let d = 16.0 + 8.0 * t;
            let cx = x + 14.0 + t * (tokens::SWITCH_W - 30.0);
            let cy = y + tokens::SWITCH_H * 0.5;
            if is_hover {
                // M3 状态层跟着拇指走（圆形，半径 = 轨道半高 + 4），且画在拇指**之下**。
                b.fill_circle(
                    cx,
                    cy,
                    tokens::SWITCH_H * 0.5 + 4.0,
                    colors.on_surface.with_alpha(tokens::STATE_HOVER),
                );
            }
            b.fill_circle(cx, cy, d * 0.5, thumb);
            if mark_visible {
                // M3 选中态拇指内画勾（Flutter `Switch` 的 selectedIcon，缩到拇指的 0.62）。
                let s = d * 0.62;
                let c = colors.primary.with_alpha(mark_alpha);
                let p = |fx: f32, fy: f32| (cx - s * 0.5 + s * fx, cy - s * 0.5 + s * fy);
                let (x1, y1) = p(0.2, 0.5);
                let (x2, y2) = p(0.42, 0.72);
                let (x3, y3) = p(0.8, 0.3);
                b.line(x1, y1, x2, y2, c, 2.0);
                b.line(x2, y2, x3, y3, c, 2.0);
            }
        }));
    });
}

/// `Switch + 标签` 一行。
pub fn SwitchRow(checked: MutableState<bool>, label: &str, modifier: Modifier) {
    let lbl = label.to_string();
    Row(
        modifier.fill_max_width(),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            let colors = theme_colors();
            Text(
                lbl.clone(),
                tokens::body_large(colors.on_surface),
                Modifier::new(),
            );
            Switch(checked, Modifier::new());
        },
    );
}

/// `Slider(value: 0..1)` — Compose `Slider`（M3：4dp 轨道 + 20dp 拇指，点击/拖动设值）。
pub fn Slider(value: MutableState<f32>, modifier: Modifier) {
    let colors = theme_colors();
    let v = value.get().clamp(0.0, 1.0);
    let id = add_node(
        Policy::Leaf,
        modifier
            .no_indication()
            .height(tokens::SLIDER_THUMB + tokens::SPACE_XS),
    );
    let st = value;
    with_rt_pub(|rt| {
        // 按下即抓取：开启比例拖拽会话并把值跳到按下点（M3 Slider 的「按住即跟随」）。
        rt.nodes[id.index()].on_press = Some(Box::new(move || {
            let (ax, _, aw, _) = node_abs_rect(id);
            let (gx, _) = last_pointer_global();
            begin_drag_session(ax, 0.0, aw, 1.0, st, true, 0.0, 1.0);
            if aw > 0.0 {
                st.set(((gx - ax) / aw).clamp(0.0, 1.0));
            }
        }));
        rt.nodes[id.index()].on_click = Some(Box::new(move || {
            let (local_x, _) = last_click_local();
            let width = with_rt_pub(|rt2| rt2.nodes[id.index()].width);
            if width > 0.0 {
                st.set((local_x / width).clamp(0.0, 1.0));
            }
        }));
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            // 轨道两端各留半个拇指，避免拇指出界（对齐 Compose Slider 的 track 内缩）
            let pad = tokens::SLIDER_THUMB * 0.5;
            let span = (w - pad * 2.0).max(1.0);
            let cy = y + h * 0.5;
            let r = tokens::SLIDER_TRACK * 0.5;
            // M3：未激活段用比轨道底更实的一档，否则在 surface 上几乎看不见
            b.fill_rrect(
                x + pad,
                cy - r,
                span,
                tokens::SLIDER_TRACK,
                r,
                colors.surface_variant,
            );
            b.fill_rrect(
                x + pad,
                cy - r,
                v * span,
                tokens::SLIDER_TRACK,
                r,
                colors.primary,
            );
            b.fill_circle(
                x + pad + v * span,
                cy,
                tokens::SLIDER_THUMB * 0.5,
                colors.primary,
            );
        }));
    });
}

/// `SpinBox(value, min..max)` — 数字微调框（M3：40dp 高、`shapeSmall`、1dp `outline`）。
pub fn SpinBox(value: MutableState<i32>, min: i32, max: i32, modifier: Modifier) {
    let v = value.get();
    let colors = theme_colors();
    Row(
        modifier
            .border(1.0, colors.outline, tokens::SHAPE_SMALL)
            .height(tokens::CONTROL_HEIGHT),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let st = value;
            crate::widgets::basic::IconButton(IconName::Remove, Modifier::new(), move || {
                st.update(|v| *v = (*v - 1).max(min))
            });
            let id = add_node(
                Policy::Leaf,
                Modifier::new()
                    .min_width(48.0)
                    .height(tokens::CONTROL_HEIGHT),
            );
            with_rt_pub(|rt| {
                let s = v.to_string();
                rt.nodes[id.index()].draw =
                    Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                        crate::widgets::basic::CenteredText(
                            b,
                            x,
                            y,
                            w,
                            h,
                            &s,
                            &tokens::body_large(colors.on_surface),
                        );
                    }));
            });
            let st2 = value;
            crate::widgets::basic::IconButton(IconName::Add, Modifier::new(), move || {
                st2.update(|v| *v = (*v + 1).min(max))
            });
        },
    );
}

/// `RatingBar(value, max)` — 星级评分（24dp 星 + 4dp 间隙，选中 `primary`）。
pub fn RatingBar(value: MutableState<i32>, max: i32, modifier: Modifier) {
    let colors = theme_colors();
    let v = value.get();
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for i in 1..=max {
                let st = value;
                let id = add_node(
                    Policy::Leaf,
                    Modifier::new()
                        .size(tokens::ICON_DEFAULT + tokens::SPACE_XS, 28.0)
                        .padding_ltrb(tokens::SPACE_XS * 0.5, 0.0, tokens::SPACE_XS * 0.5, 0.0),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                    let filled = i <= v;
                    rt.nodes[id.index()].draw =
                        Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                            star(
                                b,
                                x + tokens::SPACE_XS * 0.5,
                                y + 2.0,
                                tokens::ICON_DEFAULT,
                                if filled {
                                    colors.primary
                                } else {
                                    colors.outline_variant
                                },
                            );
                        }));
                });
            }
        },
    );
}

fn star(b: &mut DrawBuilder, x: f32, y: f32, s: f32, color: Color) {
    let cx = x + s * 0.55;
    let cy = y + s * 0.55;
    let r = s * 0.5;
    let mut pts = Vec::with_capacity(11);
    for i in 0..10 {
        let rr = if i % 2 == 0 { r } else { r * 0.45 };
        let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        pts.push((cx + ang.cos() * rr, cy + ang.sin() * rr));
    }
    pts.push(pts[0]);
    b.fill_path(&pts, color);
}

// ══════════════════════════════ 下拉 / 菜单 ══════════════════════════════

struct MenuCb {
    select: Box<dyn FnMut(usize)>,
    close: Option<Box<dyn Fn()>>,
}

thread_local! {
    /// 菜单回调表（key = 浮层 key）。零 `Rc`：回调存线程局部表，
    /// 多个菜单项通过 `key` 索引同一个 `FnMut`。
    static MENU_CB: RefCell<HashMap<u64, MenuCb>> = RefCell::new(HashMap::new());
}

fn fire_menu_cb(key: u64, index: usize) {
    MENU_CB.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(cb) = m.get_mut(&key) {
            (cb.select)(index);
            if let Some(c) = &cb.close {
                c();
            }
        }
    });
}

fn put_menu_cb(key: u64, cb: impl FnMut(usize) + 'static) {
    MENU_CB.with(|m| {
        m.borrow_mut().insert(
            key,
            MenuCb {
                select: Box::new(cb),
                close: None,
            },
        );
    });
}

/// 登记菜单项回调（供 CommandPalette 等复用同一套零 `Rc` 回调表）。
pub fn set_menu_callback(key: u64, cb: impl FnMut(usize) + 'static) {
    put_menu_cb(key, cb);
}

/// 触发菜单项回调（含关闭回调）。
pub fn run_menu_callback(key: u64, index: usize) {
    fire_menu_cb(key, index);
}

/// 设置菜单关闭回调（点项后自动调用）。
pub fn set_menu_close(key: u64, close: impl Fn() + 'static) {
    MENU_CB.with(|m| {
        let mut mm = m.borrow_mut();
        let e = mm.entry(key).or_insert_with(|| MenuCb {
            select: Box::new(|_| {}),
            close: None,
        });
        e.close = Some(Box::new(close));
    });
}

/// 浮层菜单面板：锚定在 `anchor` 节点下方，点项触发回调并关闭。
///
/// M3 `Menu`：`surface_container` 底、4dp 圆角、elevation 3、上下 8dp 内边距，
/// 菜单项 48dp 高 / 左右 12dp、文字 `bodyLarge`。
pub fn menu_panel(key: u64, anchor: NodeId, items: &[String]) {
    menu_panel_impl(key, anchor, items, true);
}

/// 同 [`menu_panel`]，但面板按内容**自然宽**展开（而非对齐锚点宽）。
///
/// 适用窄锚点场景（如顶部菜单栏的 `File/View/…` 标签）：锚点很窄但项文字较长，
/// 对齐锚点宽会裁字。复用 `menu_panel` 的全部行为（滚动封顶 / 入场动画 / 边缘重定位 /
/// 悬停高亮 / `fire_menu_cb(key, i)` 回调），仅宽度模式不同。
pub fn menu_panel_auto_width(key: u64, anchor: NodeId, items: &[String]) {
    menu_panel_impl(key, anchor, items, false);
}

/// 按内容自然宽计算菜单面板宽度：最宽项文字 + 左右内边距 + 滚动条 gutter。
///
/// `labels` 只含可点项（不含分隔条），分隔条无文字不参与算宽。
fn natural_menu_width(labels: &[&String], colors: ColorScheme) -> f32 {
    let style = tokens::body_large(colors.on_surface);
    let max_text = labels
        .iter()
        .map(|s| measure_text_with_measurer(s, &style).0)
        .fold(0.0f32, f32::max);
    // 面板是 `VerticalScroll` 容器：交叉轴会**稳定预留** `SCROLLBAR_GUTTER`（桌面端 12px，
    // 无论是否溢出都扣），故孩子可用宽 = 面板宽 − gutter；item 绘制又内缩 `SPACE_MD×2`。
    // 若只留 `SPACE_SM`(8) 会把最宽项挤到不足而换行（如 "Tab 10" 占 2 行）。
    max_text + tokens::SPACE_MD * 2.0 + compose::layout::SCROLLBAR_GUTTER + 2.0
}

/// 菜单行：普通可点项，或两段之间的水平分隔条（TabRow 溢出「右在上、左在下」用之）。
#[derive(Clone)]
enum MenuRow {
    Item(String),
    Sep,
}

/// 分隔条占位高度（上下留白 + 1px 线）。
const MENU_SEP_H: f32 = 9.0;

/// `menu_panel` 实现体：全部为普通项，转发到行序列实现 [`menu_panel_rows`]。
fn menu_panel_impl(key: u64, anchor: NodeId, items: &[String], match_width: bool) {
    let rows: Vec<MenuRow> = items.iter().cloned().map(MenuRow::Item).collect();
    menu_panel_rows(key, anchor, &rows, match_width);
}

/// 把「分段标签列表」摊平成 [`MenuRow`] 序列：相邻非空段之间插一条 `Sep`（空段整体跳过，
/// 故只有一段时不产生分隔条）。段的先后即菜单上下顺序。
fn build_section_rows(sections: &[Vec<String>]) -> Vec<MenuRow> {
    let mut rows = Vec::new();
    let mut wrote_any = false;
    for sec in sections {
        if sec.is_empty() {
            continue;
        }
        if wrote_any {
            rows.push(MenuRow::Sep);
        }
        wrote_any = true;
        for s in sec {
            rows.push(MenuRow::Item(s.clone()));
        }
    }
    rows
}

/// 溢出菜单面板（自然宽 + 段间分隔条）：`on_selected` 的扁平下标跨段连续编号，
/// 与 [`build_section_rows`] 摊平后仅计 `Item` 的顺序一致。
pub fn menu_panel_sections(key: u64, anchor: NodeId, sections: &[Vec<String>]) {
    let rows = build_section_rows(sections);
    menu_panel_rows(key, anchor, &rows, false);
}

/// 由 `expanded` 控制的「分段 + 分隔条」浮层菜单（TabRow 溢出）。行为同 [`DropdownMenu`]，
/// 惟内容走 [`menu_panel_sections`]：右溢出段在上、左溢出段在下，中间一条分隔条。
pub fn dropdown_menu_sections(
    anchor: NodeId,
    key: u64,
    sections: &[Vec<String>],
    expanded: MutableState<bool>,
    on_selected: impl FnMut(usize) + 'static,
) {
    put_menu_cb(key, on_selected);
    set_menu_close(key, move || expanded.set(false));
    let is_open = expanded.get();
    let sections: Vec<Vec<String>> = sections.to_vec();
    let close = expanded.clone();
    Overlay(key, is_open, move || {
        let sections = sections.clone();
        Clickable(
            Modifier::new().fill_max_size().no_indication(),
            move || close.set(false),
            move || {
                menu_panel_sections(key, anchor, &sections);
            },
        );
    });
}

/// 菜单面板行序列实现：`Item` 绘制可点行（`fire_menu_cb(key, i)`，`i` 只数 Item 不数 Sep），
/// `Sep` 绘制一条水平分隔条。`match_width` 决定宽度取锚点宽还是内容自然宽。
fn menu_panel_rows(key: u64, anchor: NodeId, rows: &[MenuRow], match_width: bool) {
    let colors = theme_colors();
    let g = glass_spec();
    // 可点项标签（不含分隔条）：用于算宽与行高。
    let labels: Vec<&String> = rows
        .iter()
        .filter_map(|r| match r {
            MenuRow::Item(s) => Some(s),
            MenuRow::Sep => None,
        })
        .collect();
    let sep_count = rows.iter().filter(|r| matches!(r, MenuRow::Sep)).count();
    let rows_h = labels.len() as f32 * tokens::MENU_ITEM_HEIGHT + sep_count as f32 * MENU_SEP_H;
    // M3 菜单入场：淡入 + 自锚点向下**生长**（引擎无 graphicsLayer 缩放，改用
    // `max_height` 逐帧展开 + `.clip` 裁掉溢出，观感等同 Flutter 的 scale-from-anchor）。
    let t = animate_progress_on(key, tokens::enter_spec());
    // 滚动判据：菜单全展开后若「弹出位置到屏幕底部的剩余空间」装不下，就封顶到该剩余空间，
    // 超出部分纵向滚动（`rows_h > view_h`  ⇔  内容溢出  ⇔  `scrollbar_geometry` 出条）。
    // 弹出位置 = 锚点下沿 + 4px 间距；不再用固定项数或窗口比例，纯粹以「是否伸出屏幕底部」为准。
    // menubar / contextmenu / dropdown 共用此路径，一并生效。
    let vh = compose::layout::viewport_size().1;
    let (_, ay, _, ah) = compose::layout::node_abs_rect(anchor);
    let popup_top = (ay + ah + 4.0).min(vh);
    let view_h = (vh - popup_top - tokens::SPACE_MD * 2.0).max(tokens::MENU_ITEM_HEIGHT);
    let target_h = rows_h.min(view_h);
    // 位置/宽度延迟到布局期解析（`pin_to`）：组合期锚点节点可能刚随重组重建，坐标尚为 (0,0)。
    let pin = if match_width {
        Modifier::new().pin_below_width(anchor, 4.0)
    } else {
        Modifier::new()
            .pin_below(anchor, 4.0)
            .width(natural_menu_width(&labels, colors))
    };
    let mut m = GlassOrBg(
        pin,
        g,
        colors.surface_container,
        tokens::FIELD_RADIUS,
    )
    .padding_hv(0.0, tokens::SPACE_MD)
    .max_height(target_h * t)
    .clip(tokens::FIELD_RADIUS)
    .shadow(
        tokens::ELEVATION_2,
        tokens::FIELD_RADIUS,
        tokens::shadow_color(colors.is_dark()),
    );
    if !g.enabled {
        m = m.border(1.0, colors.outline_variant, tokens::FIELD_RADIUS);
    }
    // 面板即滚动容器：项数超出视口时滚轮/拖拽滚动条可滚动选择（偏移经 `remember` 持久化，
    // 悬停重组不会把滚动位顶回顶部）。
    let rows_vec: Vec<MenuRow> = rows.to_vec();
    VerticalScroll(
        m,
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
        let rows = rows_vec.clone();
        let mut item_i = 0usize;
        for row in rows {
            match row {
                MenuRow::Sep => {
                    let sid = add_node(
                        Policy::Leaf,
                        Modifier::new().fill_max_width().height(MENU_SEP_H),
                    );
                    let line = colors.outline_variant;
                    with_rt_pub(move |rt| {
                        rt.nodes[sid.index()].draw = Some(Box::new(
                            move |b: &mut DrawBuilder, x, y, w, h| {
                                b.fill_rect(
                                    x + tokens::SPACE_MD,
                                    y + (h - 1.0) * 0.5,
                                    (w - tokens::SPACE_MD * 2.0).max(0.0),
                                    1.0,
                                    line,
                                );
                            },
                        ));
                    });
                }
                MenuRow::Item(label) => {
                    let i = item_i;
                    item_i += 1;
                    let hovered = compose::runtime::remember(|| mutableStateOf(false));
                    let cid = add_node(
                        Policy::Leaf,
                        Modifier::new()
                            .fill_max_width()
                            .height(tokens::MENU_ITEM_HEIGHT)
                            .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0),
                    );
                    // 依赖登记必须在 runtime 借用之外：绘制闭包由 `leaf_intrinsic` 在
                    // `perform_layout` 的 `borrow_mut` 里执行，闭包内 `get()` 嵌套可变借必 panic。
                    let is_hover = hovered.get();
                    with_rt_pub(|rt| {
                        let n = &mut rt.nodes[cid.index()];
                        n.on_click = Some(Box::new(move || {
                            fire_menu_cb(key, i);
                        }));
                        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
                        n.draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                            // Leaf 绘制原点 = 节点外角（引擎不对叶子自身 padding 做偏移），
                            // 故文字手动内缩 `SPACE_MD`，hover 高亮铺满整行宽度。
                            if is_hover {
                                b.fill_rect(
                                    x,
                                    y,
                                    w,
                                    h,
                                    tokens::state_layer(colors.on_surface, tokens::STATE_HOVER),
                                );
                            }
                            let style = tokens::body_large(colors.on_surface);
                            let th = style.line_height.unwrap_or(h);
                            b.draw_text(
                                x + tokens::SPACE_MD,
                                y + (h - th) * 0.5,
                                &label,
                                &style,
                                (w - tokens::SPACE_MD * 2.0).max(0.0),
                            );
                        }));
                    });
                }
            }
        }
    });
}

/// `FloatingMenu(key, anchor, items, onSelected)` — 直接画一个浮层菜单（回调存表，零 `Rc`）。
pub fn FloatingMenu(
    key: u64,
    anchor: NodeId,
    items: &[String],
    on_selected: impl FnMut(usize) + 'static,
) {
    put_menu_cb(key, on_selected);
    menu_panel(key, anchor, items);
}

/// `DropdownMenu(anchor, key, items, expanded, match_width, onSelected)` — 由 `expanded` 控制开关的浮层菜单。
///
/// `match_width`：`true` 面板宽对齐锚点（下拉框）；`false` 按内容自然宽（TabRow 溢出「⋮」）。
pub fn DropdownMenu(
    anchor: NodeId,
    key: u64,
    items: &[String],
    expanded: MutableState<bool>,
    match_width: bool,
    on_selected: impl FnMut(usize) + 'static,
) {
    put_menu_cb(key, on_selected);
    set_menu_close(key, move || expanded.set(false));
    let is_open = expanded.get();
    let items: Vec<String> = items.to_vec();
    let close = expanded.clone();
    Overlay(key, is_open, move || {
        // 整屏可点：点浮层外（含触发框）即关闭；点菜单项由 item 的 on_click 优先消费。
        let items = items.clone();
        Clickable(
            Modifier::new().fill_max_size().no_indication(),
            move || close.set(false),
            move || {
                menu_panel_impl(key, anchor, &items, match_width);
            },
        );
    });
}

/// `Dropdown(options, selected)` — 点击弹出选项浮层（M3 `ExposedDropdownMenu` 形态）。
pub fn Dropdown(options: &[String], selected: MutableState<usize>, modifier: Modifier) {
    let colors = theme_colors();
    let opts: Vec<String> = options.to_vec();
    let cur = selected.get();
    let open = remember(|| mutableStateOf(false));
    let key = RememberOverlayKey();
    let label = opts.get(cur).cloned().unwrap_or_default();
    let is_open = open.get();
    // M3 `ExposedDropdownMenu`：展开时描边加粗到 2dp、箭头转 180°，两条都用 state layer 时长插值。
    let spec = tokens::state_spec();
    let edge = animate_float_as_state(
        if is_open {
            tokens::OUTLINE_W_FOCUSED
        } else {
            tokens::OUTLINE_W
        },
        spec,
    );
    let edge_c = animate_color_as_state(
        if is_open {
            colors.primary
        } else {
            colors.outline
        },
        spec,
    );
    let rot = animate_float_as_state(if is_open { 1.0 } else { 0.0 }, spec);

    let trigger = add_node(
        Policy::Row {
            arrangement: Arrangement::SpaceBetween,
            cross: CrossAxisAlignment::Center,
        },
        GlassOrBg(
            modifier,
            glass_spec(),
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            tokens::FIELD_RADIUS,
        )
        .padding_ltrb(tokens::FIELD_PAD_H, 0.0, tokens::SPACE_SM, 0.0)
        .height(tokens::DROPDOWN_HEIGHT)
        .min_width(tokens::DROPDOWN_MIN_W)
        .border(edge, edge_c, tokens::FIELD_RADIUS),
    );
    with_rt_pub(|rt| {
        rt.nodes[trigger.index()].on_click = Some(Box::new(move || open.update(|v| *v = !*v)));
    });
    with_content_scope(trigger, move || {
        Text(
            label.clone(),
            tokens::body_large(colors.on_surface),
            Modifier::new(),
        );
        RotatingChevron(
            rot,
            tokens::ICON_SMALL,
            colors.on_surface_variant,
            Modifier::new(),
        );
    });

    DropdownMenu(trigger, key, &opts, open, true, move |i| selected.set(i));
}

/// `ComboBox(options, selected)` — 可搜索下拉（内联过滤列表）。
pub fn ComboBox(options: &[String], selected: MutableState<usize>, modifier: Modifier) {
    let query = remember(|| mutableStateOf(String::new()));
    let cc = theme_colors();
    let opts: Vec<String> = options.to_vec();
    let q = query.read_clone().to_lowercase();
    let filtered: Vec<(usize, String)> = opts
        .iter()
        .enumerate()
        .filter(|(_, o)| q.is_empty() || o.to_lowercase().contains(&q))
        .map(|(i, o)| (i, o.clone()))
        .collect();
    let cur = selected.get();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            crate::widgets::basic::SearchBar(query, "Search…", Modifier::new().fill_max_width());
            for (real_index, o) in filtered.clone() {
                let on = real_index == cur;
                let st = selected;
                crate::widgets::basic::InkWell(
                    Modifier::new()
                        .fill_max_width()
                        .height(tokens::MENU_ITEM_HEIGHT)
                        .background(
                            if on {
                                cc.secondary_container
                            } else {
                                Color::rgba(0.0, 0.0, 0.0, 0.0)
                            },
                            tokens::SHAPE_SMALL,
                        )
                        .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0),
                    move || st.set(real_index),
                    move || {
                        Text(
                            o.clone(),
                            tokens::body_large(cc.on_surface),
                            Modifier::new(),
                        );
                    },
                );
            }
        },
    );
}

/// `PopupMenuButton(items, onSelected)` — 溢出菜单按钮（M3 40dp 图标按钮 + 状态层）。
pub fn PopupMenuButton(
    items: &[String],
    modifier: Modifier,
    on_selected: impl FnMut(usize) + 'static,
) {
    let expanded = remember(|| mutableStateOf(false));
    let key = RememberOverlayKey();
    let trigger = add_node(
        Policy::Box(Alignment::Center),
        modifier.size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
    );
    let colors = theme_colors();
    let hovered = compose::runtime::remember(|| mutableStateOf(false));
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[trigger.index()];
        n.on_click = Some(Box::new(move || expanded.update(|v| *v = !*v)));
        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
    });
    with_content_scope(trigger, move || {
        icon(
            IconName::MoreVert,
            tokens::ICON_DEFAULT,
            colors.on_surface_variant,
            Modifier::new().background(
                tokens::state_layer(
                    colors.on_surface,
                    if hovered.get() {
                        tokens::STATE_HOVER
                    } else {
                        0.0
                    },
                ),
                tokens::full_radius(tokens::CONTROL_HEIGHT),
            ),
        );
    });
    DropdownMenu(trigger, key, items, expanded, true, on_selected);
}

/// `ContextMenu(items, onSelected) { content }` — **右键**点击内容弹出菜单。
///
/// 右键走引擎的 `on_secondary_click`（`Kernel::pointer_secondary_click` →
/// `dispatch_secondary_click_at`）：不进入左键的按压/波纹链路，也不会被
/// `on_click` 结算。左键 `on_click` 作为兼容入口保留（点击同样可开关菜单）。
pub fn ContextMenu(
    items: &[String],
    modifier: Modifier,
    on_selected: impl FnMut(usize) + 'static,
    content: impl Fn() + 'static,
) {
    let expanded = remember(|| mutableStateOf(false));
    let key = RememberOverlayKey();
    let id = add_node(Policy::Box(Alignment::TopStart), modifier);
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[id.index()];
        n.on_click = Some(Box::new(move || expanded.update(|v| *v = !*v)));
        // 右键总是「打开」而非 toggle：重复右键不应把已打开的菜单关掉。
        n.on_secondary_click = Some(Box::new(move || expanded.set(true)));
    });
    with_content_scope(id, content);
    DropdownMenu(id, key, items, expanded, true, on_selected);
}

// ══════════════════════════════ 复合输入 ══════════════════════════════

/// `ColorPicker(color)` — HSV 取色器：左侧饱和度/明度方块 + 右侧竖向色相条 + 底部预览条。
///
/// 交互：方块内点击/拖拽设饱和度（x）与明度（y），色相条点击/拖拽设色相；
/// HSV 存于内部 `remember` 状态（低饱和时 hue 不丢），每次改动同步写回 `color`。
pub fn ColorPicker(color: MutableState<Color>, modifier: Modifier) {
    let colors = theme_colors();
    // HSV 内部状态：首次组合从传入 color 播种；此后以内部状态为准（hue 在灰阶下仍保留）。
    let h_st = remember(|| {
        let (h, _, _) = rgb_to_hsv(color.get());
        mutableStateOf(h)
    });
    let s_st = remember(|| {
        let (_, s, _) = rgb_to_hsv(color.get());
        mutableStateOf(s)
    });
    let v_st = remember(|| {
        let (_, _, v) = rgb_to_hsv(color.get());
        mutableStateOf(v)
    });
    // 组合期读取 → 建立依赖，任一变化即重组（重绘各区域）。
    let h = h_st.get();
    let s = s_st.get();
    let v = v_st.get();

    const SV_W: f32 = 300.0;
    const SV_H: f32 = 200.0;
    const HUE_W: f32 = 22.0;

    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                move || {
                    // ── SV 方块（饱和度→明度）──
                    let sv = add_node(
                        Policy::Leaf,
                        Modifier::new().size(SV_W, SV_H).no_indication().border(
                            1.0,
                            colors.outline_variant,
                            2.0,
                        ),
                    );
                    let hue = h;
                    with_rt_pub(move |rt| {
                        let ss = s_st;
                        let vv = v_st;
                        let cc = color;
                        rt.nodes[sv.index()].on_press = Some(Box::new(move || {
                            let (ax, ay, aw, ah) = node_abs_rect(sv);
                            let (gx, gy) = last_pointer_global();
                            let apply = move |gx: f32, gy: f32| {
                                if aw > 0.0 && ah > 0.0 {
                                    let ns = ((gx - ax) / aw).clamp(0.0, 1.0);
                                    let nv = 1.0 - ((gy - ay) / ah).clamp(0.0, 1.0);
                                    ss.set(ns);
                                    vv.set(nv);
                                    cc.set(hsv_to_rgb(hue, ns, nv));
                                }
                            };
                            apply(gx, gy);
                            begin_drag_2d(move |gx, gy| apply(gx, gy));
                        }));
                        // 命中链放行：`dispatch_press_down` 靠 `nearest_click_idx`（只认 `on_click`）
                        // 判定是否命中，找不到即丢弃 `on_press`。取色已在按下瞬间完成，
                        // 故 `on_click` 留空即可（同 Slider / Splitter 的 on_press+on_click 成对约定）。
                        rt.nodes[sv.index()].on_click = Some(Box::new(move || {}));
                        let marker = (s, v);
                        rt.nodes[sv.index()].draw =
                            Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                                // 15×10 离散色块（留 1px 缝），对齐参考图的格子观感
                                let cols = 15.0f32;
                                let rows = 10.0f32;
                                let cw = w / cols;
                                let ch = h / rows;
                                for r in 0..rows as usize {
                                    for c in 0..cols as usize {
                                        let sat = c as f32 / (cols - 1.0);
                                        let val = 1.0 - r as f32 / (rows - 1.0);
                                        b.fill_rect(
                                            x + c as f32 * cw,
                                            y + r as f32 * ch,
                                            cw - 1.0,
                                            ch - 1.0,
                                            hsv_to_rgb(hue, sat, val),
                                        );
                                    }
                                }
                                // 选中指示：当前 (s, v) 处画圆环
                                let mx = x + marker.0 * w;
                                let my = y + (1.0 - marker.1) * h;
                                b.stroke_circle(mx, my, 6.0, Color::WHITE, 2.0);
                                b.stroke_circle(mx, my, 6.0, Color::BLACK.with_alpha(0.5), 1.0);
                            }));
                    });
                    Spacer(Modifier::new().width(10.0));
                    // ── 色相条（竖向 0..360）──
                    let hs = add_node(
                        Policy::Leaf,
                        Modifier::new().size(HUE_W, SV_H).no_indication(),
                    );
                    with_rt_pub(move |rt| {
                        let hh = h_st;
                        let cc = color;
                        let cur_s = s;
                        let cur_v = v;
                        rt.nodes[hs.index()].on_press = Some(Box::new(move || {
                            let (_ax, ay, _aw, ah) = node_abs_rect(hs);
                            let (gx, gy) = last_pointer_global();
                            let apply = move |gx: f32, gy: f32| {
                                let _ = gx;
                                if ah > 0.0 {
                                    let nh = ((gy - ay) / ah).clamp(0.0, 1.0) * 360.0;
                                    hh.set(nh);
                                    cc.set(hsv_to_rgb(nh, cur_s, cur_v));
                                }
                            };
                            apply(gx, gy);
                            begin_drag_2d(move |gx, gy| apply(gx, gy));
                        }));
                        rt.nodes[hs.index()].on_click = Some(Box::new(move || {}));
                        let cur_h = h;
                        rt.nodes[hs.index()].draw =
                            Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                                let segs = 60.0f32;
                                let sh = h / segs;
                                for i in 0..segs as usize {
                                    let hue = i as f32 / (segs - 1.0) * 360.0;
                                    b.fill_rect(
                                        x,
                                        y + i as f32 * sh,
                                        w,
                                        sh + 1.0,
                                        hsv_to_rgb(hue, 1.0, 1.0),
                                    );
                                }
                                // 当前色相位置指示（白色横条）
                                let my = y + cur_h / 360.0 * h;
                                b.fill_rrect(x - 1.0, my - 3.0, w + 2.0, 6.0, 3.0, Color::WHITE);
                                b.stroke_rrect(x - 1.0, my - 3.0, w + 2.0, 6.0, 3.0, Color::BLACK.with_alpha(0.4), 1.0);
                            }));
                    });
                },
            );
            Spacer(Modifier::new().height(10.0));
            // ── 预览条：当前选中色（实色）──
            let pv = add_node(
                Policy::Leaf,
                Modifier::new().size(SV_W + 10.0 + HUE_W, 20.0).border(
                    1.0,
                    colors.outline_variant,
                    4.0,
                ),
            );
            let sel = hsv_to_rgb(h, s, v);
            with_rt_pub(move |rt| {
                rt.nodes[pv.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                    b.fill_rrect(x, y, w, h, 4.0, sel);
                }));
            });
        },
    );
}

/// HSV → RGB（`h` 0..360，`s`/`v` 0..1）。
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let i = hp.floor() as i32;
    let f = hp - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match ((i % 6) + 6) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color::rgb(r, g, b)
}

/// RGB → HSV（返回 `h` 0..360，`s`/`v` 0..1）。
fn rgb_to_hsv(c: Color) -> (f32, f32, f32) {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    let d = max - min;
    let v = max;
    let s = if max <= 0.0 { 0.0 } else { d / max };
    let h = if d <= 1e-6 {
        0.0
    } else if max == c.r {
        (c.g - c.b) / d
    } else if max == c.g {
        2.0 + (c.b - c.r) / d
    } else {
        4.0 + (c.r - c.g) / d
    };
    ((h * 60.0).rem_euclid(360.0), s, v)
}

/// `TimePicker(hour, minute)` — 时 / 分竖向步进器（上 ▲ / 补零数值框 / 下 ▼）。
pub fn TimePicker(hour: MutableState<i32>, minute: MutableState<i32>, modifier: Modifier) {
    let colors = theme_colors();
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            time_field(hour, 0, 23);
            Text(
                ":",
                tokens::title_large(colors.on_surface),
                Modifier::new().padding_ltrb(tokens::SPACE_SM, 0.0, tokens::SPACE_SM, 0.0),
            );
            time_field(minute, 0, 59);
        },
    );
}

/// 竖向时间步进列：▲ 按钮 / 补零两位数值框 / ▼ 按钮（交叉轴居中，按钮窄于数值框）。
fn time_field(value: MutableState<i32>, min: i32, max: i32) {
    let colors = theme_colors();
    // 读取 value 建立依赖：点击步进后本作用域重组，数值框文本随之更新。
    let s = format!("{:02}", value.get());
    Column(
        Modifier::new(),
        Arrangement::Center,
        CrossAxisAlignment::Center,
        move || {
            step_button(value, min, max, true);
            Spacer(Modifier::new().height(tokens::SPACE_XXS));
            let id = add_node(
                Policy::Leaf,
                Modifier::new()
                    .size(64.0, tokens::CONTROL_HEIGHT + 8.0)
                    .background(colors.Surface, tokens::SHAPE_SMALL)
                    .border(1.0, colors.outline, tokens::SHAPE_SMALL),
            );
            with_rt_pub(|rt| {
                let st = tokens::title_large(colors.on_surface);
                let txt = s.clone();
                rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                    crate::widgets::basic::CenteredText(b, x, y, w, h, &txt, &st);
                }));
            });
            Spacer(Modifier::new().height(tokens::SPACE_XXS));
            step_button(value, min, max, false);
        },
    );
}

/// 步进三角按钮：`up` = ▲（加一），否则 ▼（减一）。浅底圆角方块 + 实心三角。
fn step_button(value: MutableState<i32>, min: i32, max: i32, up: bool) {
    let colors = theme_colors();
    let id = add_node(
        Policy::Leaf,
        Modifier::new()
            .size(44.0, 22.0)
            .background(colors.surface_variant, tokens::SHAPE_SMALL),
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(move || {
            value.update(|v| {
                *v = if up {
                    (*v + 1).min(max)
                } else {
                    (*v - 1).max(min)
                };
            });
        }));
        let fg = colors.on_surface_variant;
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let r = 6.0;
            let pts: [(f32, f32); 3] = if up {
                [(cx, cy - r), (cx - r, cy + r), (cx + r, cy + r)]
            } else {
                [(cx, cy + r), (cx - r, cy - r), (cx + r, cy - r)]
            };
            b.fill_path(&pts, fg);
        }));
    });
}

/// `DatePicker(year, month, selected)` — 基于日历的日期选择。
pub fn DatePicker(year: i32, month: u32, selected: MutableState<u32>, modifier: Modifier) {
    crate::widgets::advanced::Calendar(year, month, selected, modifier);
}

/// `OtpInput(code, len)` — 验证码输入。
///
/// `OTPInput`：n 个单字符格子，键入自动前进，Backspace 删当前格（有字）或
/// 前一格（空格）并后退，左右键移动光标，点击定位，填满后焦点停在末尾之外。
pub fn OtpInput(code: MutableState<String>, len: usize, modifier: Modifier) {
    let colors = theme_colors();
    let value: Vec<char> = code.read_clone().chars().collect();
    let id = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        modifier.padding_hv(0.0, 4.0),
    );
    bind_text_value(id, code, 0.0);
    // 焦点格（点击 / 输入 / 方向键后更新；边框与光标在 draw 闭包里实时读取，无需重组）；
    // 行节点每次组合刷新，输入函数改焦点后据此直接失效绘制。
    let focus = OTP_INPUT.with(|m| {
        let mut m = m.borrow_mut();
        let e = m.entry(state_id(code)).or_insert_with(|| {
            (
                std::rc::Rc::new(std::cell::Cell::new(0usize)),
                len,
                std::rc::Rc::new(std::cell::Cell::new(id)),
            )
        });
        e.1 = len;
        e.2.set(id);
        e.0.clone()
    });
    caret_blink_driver(code, id);
    with_content_scope(id, move || {
        for i in 0..len {
            if i > 0 {
                Spacer(Modifier::new().size(8.0, 44.0));
            }
            let ch = value.get(i).copied();
            let s = ch.map(|c| c.to_string()).unwrap_or_default();
            let cid = add_node(
                Policy::Leaf,
                Modifier::new()
                    .size(tokens::CONTROL_HEIGHT + 8.0, tokens::DROPDOWN_HEIGHT)
                    .background(Color::rgba(0.0, 0.0, 0.0, 0.0), tokens::FIELD_RADIUS),
            );
            with_rt_pub(|rt| {
                // 点击格子：聚焦整行 + 定位到该格
                let focus_on = focus.clone();
                let row = id;
                rt.nodes[cid.index()].on_click = Some(Box::new(move || {
                    set_field_focus(Some(code));
                    PREEDIT.with(|p| p.borrow_mut().clear());
                    focus_on.set(i.min(len.saturating_sub(1)));
                    compose::layout::invalidate_draw_node(row);
                }));
                // 边框（聚焦主色 2px / 其余 outline 1px）与光标（聚焦空格的竖线）实时反映焦点
                let focus_draw = focus.clone();
                rt.nodes[cid.index()].draw =
                    Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                        let focused = is_focused(code);
                        let fidx = if focused { focus_draw.get() } else { len + 1 };
                        let is_focus = i == fidx;
                        b.stroke_rrect(
                            x,
                            y,
                            w,
                            h,
                            tokens::FIELD_RADIUS,
                            if is_focus {
                                colors.primary
                            } else {
                                colors.outline
                            },
                            if is_focus { 2.0 } else { 1.0 },
                        );
                        if !s.is_empty() {
                            crate::widgets::basic::CenteredText(
                                b,
                                x,
                                y,
                                w,
                                h,
                                &s,
                                &tokens::body_large(colors.on_surface),
                            );
                        }
                        // 光标：仅画在「聚焦且为空」的格子里（竖线光标），按 530ms 闪烁
                        if is_focus && s.is_empty() {
                            let ms = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0);
                            if (ms / 530) % 2 == 0 {
                                b.fill_rect(
                                    x + w * 0.5 - 0.75,
                                    y + h * 0.5 - 8.0,
                                    1.5,
                                    16.0,
                                    colors.primary,
                                );
                            }
                        }
                    }));
            });
        }
    });
}

/// `TokenInput(tokens, draft)` — 标签输入（输入后点 + 提交）。
pub fn TokenInput(
    tokens: MutableState<Vec<String>>,
    draft: MutableState<String>,
    modifier: Modifier,
) {
    let list: Vec<String> = tokens.read_clone();
    let colors = theme_colors();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            let l2 = list.clone();
            crate::widgets::layout::FlowRow(
                Modifier::new().fill_max_width(),
                crate::tokens::SPACE_SM,
                move || {
                    for t in l2.clone() {
                        let tk = tokens;
                        let t2 = t.clone();
                        let id = add_node(
                            Policy::Row {
                                arrangement: Arrangement::Center,
                                cross: CrossAxisAlignment::Center,
                            },
                            // M3 InputChip：32dp 高、8dp 圆角、secondaryContainer 底
                            Modifier::new()
                                .min_height(crate::tokens::CHIP_HEIGHT)
                                .background(colors.secondary_container, crate::tokens::CHIP_RADIUS)
                                .padding_ltrb(
                                    crate::tokens::SPACE_SM,
                                    0.0,
                                    crate::tokens::CHIP_PAD_H_ICON,
                                    0.0,
                                ),
                        );
                        with_rt_pub(|rt| {
                            rt.nodes[id.index()].on_click = Some(Box::new(move || {
                                tk.update(|v| v.retain(|x| x != &t2));
                            }));
                        });
                        with_content_scope(id, move || {
                            let colors = theme_colors();
                            Text(
                                t.clone(),
                                crate::tokens::label_large(colors.on_secondary_container),
                                Modifier::new(),
                            );
                            Spacer(Modifier::new().width(crate::tokens::SPACE_XS));
                            icon(
                                IconName::Close,
                                crate::tokens::ICON_SMALL,
                                colors.on_secondary_container,
                                Modifier::new(),
                            );
                        });
                    }
                },
            );
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    BareTextField(
                        draft,
                        "Add tag…",
                        Modifier::new().weight(1.0, false).height(36.0),
                    );
                    let tk = tokens;
                    let dr = draft;
                    crate::widgets::basic::IconButton(IconName::Add, Modifier::new(), move || {
                        let d = dr.read_clone();
                        if !d.is_empty() {
                            tk.update(|v| v.push(d));
                            dr.set(String::new());
                        }
                    });
                },
            );
        },
    );
}

// ══════════════════════════════ RichTextEdit ══════════════════════════════
//
// 多行富文本编辑器（`TextEditor`）：回车换行、上下键跨行、Home/End、
// Shift 方向键选区、Ctrl+A 全选、Ctrl+C/V/X 复制/粘贴/剪切（线程局部剪贴板）、
// 点击定位光标、选区高亮、光标闪烁。不含多风格 Span（单一样式）。
//
// 焦点与光标沿用现有 TextField 的线程局部机制：文本存于 `MutableState<String>`
// （含 `\n`），光标/选区存于 `RICHTEXT_CARET`（按 state_id 索引）。

thread_local! {
    /// 当前聚焦的富文本字段（用于把按键/字符/剪贴板操作路由给它）。
    static FOCUSED_RICH: std::cell::Cell<Option<MutableState<String>>> =
        const { std::cell::Cell::new(None) };
    /// 富文本字段的光标 + 选区状态（按文本 state_id 索引）。
    static RICHTEXT_CARET: RefCell<HashMap<u32, MutableState<CaretSel>>> =
        RefCell::new(HashMap::new());
    /// 富文本字段的滚动偏移（非响应式 `Rc<Cell>`，在 draw 闭包内实时调整）。
    static RICHTEXT_SCROLL: RefCell<HashMap<u32, std::rc::Rc<std::cell::Cell<f32>>>> =
        RefCell::new(HashMap::new());
    /// 线程局部剪贴板（无系统剪贴板依赖）。
    static CLIPBOARD: RefCell<String> = RefCell::new(String::new());
    /// 当前是否按住 Shift（方向键据此扩展选区）。
    static SHIFT_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 富文本编辑器正文排版（与 `TextField` 同轴：`bodyLarge` 16/24）。
fn rich_style() -> TextStyle {
    tokens::body_large(Color::BLACK)
}

/// 光标 + 选区。所有偏移均为「整段文本的字符索引」（含 `\n` 占 1 个字符）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaretSel {
    pub caret: usize,
    pub anchor: Option<usize>,
}

impl Default for CaretSel {
    fn default() -> Self {
        CaretSel {
            caret: 0,
            anchor: None,
        }
    }
}

/// 选区范围 `(start, end)`，`start <= end`。无选区时 `start == end == caret`。
fn sel_range(sel: &CaretSel) -> (usize, usize) {
    match sel.anchor {
        Some(a) => (a.min(sel.caret), a.max(sel.caret)),
        None => (sel.caret, sel.caret),
    }
}

// ───────────────────────── 几何辅助（绘制 / 点击定位 / 测试） ─────────────────────────

/// `(行, 列)`：列是行内字符数。
pub(crate) fn line_col(chars: &[char], idx: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for &c in &chars[..idx.min(chars.len())] {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// 总行数。
pub(crate) fn line_count(chars: &[char]) -> usize {
    line_col(chars, chars.len()).0 + 1
}

/// 第 `line` 行的字符数。
pub(crate) fn line_len_at(chars: &[char], line: usize) -> usize {
    let mut cur = 0usize;
    let mut count = 0usize;
    let mut in_line = false;
    for &c in chars {
        if c == '\n' {
            if in_line {
                return count;
            }
            cur += 1;
            in_line = false;
        } else if cur == line {
            count += 1;
            in_line = true;
        }
    }
    count
}

/// 第 `line` 行、第 `col` 列的字符索引（col 越界时夹取到行尾）。
pub(crate) fn idx_of_line_col(chars: &[char], line: usize, col: usize) -> usize {
    let mut start = 0usize;
    let mut cur = 0usize;
    let mut i = 0usize;
    while cur < line && i < chars.len() {
        if chars[i] == '\n' {
            cur += 1;
        }
        i += 1;
    }
    start = i;
    let mut pos = start;
    let mut c = 0usize;
    while pos < chars.len() && chars[pos] != '\n' && c < col {
        pos += 1;
        c += 1;
    }
    pos
}

/// 行内点击定位：给定行文本与横向偏移 `dx`，返回最近的字符边界索引。
pub(crate) fn caret_index_in_line(line_text: &str, dx: f32) -> usize {
    let chars: Vec<char> = line_text.chars().collect();
    let style = rich_style();
    let mut prev = 0.0f32;
    for (i, _) in chars.iter().enumerate() {
        let prefix: String = chars[..=i].iter().collect();
        let (w, _) = measure_text_with_measurer(&prefix, &style);
        if dx < (prev + w) * 0.5 {
            return i;
        }
        prev = w;
    }
    chars.len()
}

// ───────────────────────── 纯编辑逻辑（可单测） ─────────────────────────

pub(crate) fn rich_move(chars: &[char], sel: &mut CaretSel, key: NavKey, shift: bool) {
    let len = chars.len();
    let (line, col) = line_col(chars, sel.caret);
    let new_caret = match key {
        NavKey::Left => sel.caret.saturating_sub(1),
        NavKey::Right => (sel.caret + 1).min(len),
        NavKey::Up => {
            if line == 0 {
                sel.caret
            } else {
                let tl = line - 1;
                idx_of_line_col(chars, tl, col.min(line_len_at(chars, tl)))
            }
        }
        NavKey::Down => {
            let total = line_count(chars);
            if line + 1 >= total {
                sel.caret
            } else {
                let tl = line + 1;
                idx_of_line_col(chars, tl, col.min(line_len_at(chars, tl)))
            }
        }
        NavKey::Home => idx_of_line_col(chars, line, 0),
        NavKey::End => idx_of_line_col(chars, line, line_len_at(chars, line)),
        NavKey::PageUp | NavKey::PageDown | NavKey::Delete => sel.caret,
    };
    if shift {
        sel.anchor = Some(sel.anchor.unwrap_or(sel.caret));
        sel.caret = new_caret;
    } else {
        sel.anchor = None;
        sel.caret = new_caret;
    }
}

pub(crate) fn rich_insert(chars: &mut Vec<char>, sel: &mut CaretSel, ins: &str) {
    let (s, e) = sel_range(sel);
    chars.drain(s..e);
    let ins: Vec<char> = ins.chars().collect();
    chars.splice(s..s, ins.iter().cloned());
    sel.caret = s + ins.len();
    sel.anchor = None;
}

pub(crate) fn rich_backspace(chars: &mut Vec<char>, sel: &mut CaretSel) {
    let (s, e) = sel_range(sel);
    if s == e {
        if s == 0 {
            return;
        }
        chars.remove(s - 1);
        sel.caret = s - 1;
    } else {
        chars.drain(s..e);
        sel.caret = s;
    }
    sel.anchor = None;
}

pub(crate) fn rich_delete(chars: &mut Vec<char>, sel: &mut CaretSel) {
    let (s, e) = sel_range(sel);
    if s == e {
        if s >= chars.len() {
            return;
        }
        chars.remove(s);
    } else {
        chars.drain(s..e);
    }
    sel.caret = s;
    sel.anchor = None;
}

pub(crate) fn rich_copy(chars: &[char], sel: &CaretSel, clip: &mut String) {
    let (s, e) = sel_range(sel);
    if s < e {
        *clip = chars[s..e].iter().collect();
    }
}

pub(crate) fn rich_cut(chars: &mut Vec<char>, sel: &mut CaretSel, clip: &mut String) {
    let (s, e) = sel_range(sel);
    if s < e {
        *clip = chars[s..e].iter().collect();
        chars.drain(s..e);
        sel.caret = s;
        sel.anchor = None;
    }
}

pub(crate) fn rich_paste(chars: &mut Vec<char>, sel: &mut CaretSel, clip: &str) {
    let (s, e) = sel_range(sel);
    chars.drain(s..e);
    let ins: Vec<char> = clip.chars().collect();
    chars.splice(s..s, ins.iter().cloned());
    sel.caret = s + ins.len();
    sel.anchor = None;
}

pub(crate) fn rich_select_all(chars: &[char], sel: &mut CaretSel) {
    sel.caret = chars.len();
    sel.anchor = if chars.is_empty() { None } else { Some(0) };
}

// ───────────────────────── MutableState 包装（事件入口调用） ─────────────────────────

fn rich_caret_state(st: MutableState<String>) -> Option<MutableState<CaretSel>> {
    RICHTEXT_CARET.with(|m| m.borrow().get(&state_id(st)).copied())
}

/// 在文本 + 光标状态上应用一次编辑，并回写。
fn with_rich(st: MutableState<String>, f: impl FnOnce(&mut Vec<char>, &mut CaretSel)) {
    let Some(cs) = rich_caret_state(st) else {
        return;
    };
    let mut chars: Vec<char> = st.read_clone().chars().collect();
    let mut sel = cs.get();
    f(&mut chars, &mut sel);
    st.set(chars.into_iter().collect());
    cs.set(sel);
}

fn rich_insert_char(st: MutableState<String>, ch: char) {
    with_rich(st, |chars, sel| rich_insert(chars, sel, &ch.to_string()));
}

fn rich_insert_text(st: MutableState<String>, s: &str) {
    with_rich(st, |chars, sel| rich_insert(chars, sel, s));
}

fn rich_backspace_char(st: MutableState<String>) {
    with_rich(st, |chars, sel| rich_backspace(chars, sel));
}

fn rich_key(st: MutableState<String>, key: NavKey) {
    match key {
        NavKey::Delete => with_rich(st, |chars, sel| rich_delete(chars, sel)),
        _ => {
            let shift = SHIFT_HELD.with(|s| s.get());
            with_rich(st, move |chars, sel| rich_move(chars, sel, key, shift));
        }
    }
}

fn rich_copy_to_clipboard(st: MutableState<String>) {
    if let Some(cs) = rich_caret_state(st) {
        let chars: Vec<char> = st.read_clone().chars().collect();
        let mut clip = CLIPBOARD.with(|c| c.borrow().clone());
        rich_copy(&chars, &cs.get(), &mut clip);
        CLIPBOARD.with(|c| *c.borrow_mut() = clip);
    }
}

fn rich_cut_to_clipboard(st: MutableState<String>) {
    if let Some(cs) = rich_caret_state(st) {
        let mut chars: Vec<char> = st.read_clone().chars().collect();
        let mut sel = cs.get();
        let mut clip = CLIPBOARD.with(|c| c.borrow().clone());
        rich_cut(&mut chars, &mut sel, &mut clip);
        CLIPBOARD.with(|c| *c.borrow_mut() = clip);
        st.set(chars.into_iter().collect());
        cs.set(sel);
    }
}

fn rich_paste_from_clipboard(st: MutableState<String>) {
    let clip = CLIPBOARD.with(|c| c.borrow().clone());
    with_rich(st, |chars, sel| rich_paste(chars, sel, &clip));
}

fn rich_select_all_field(st: MutableState<String>) {
    with_rich(st, |chars, sel| rich_select_all(chars, sel));
}

/// 回车：在光标处插入换行（替换选区）。
pub fn InputEnter() {
    if rich_editor_focused() {
        rich_enter();
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_insert_text(st, "\n");
    }
}

/// Ctrl+C：复制选区到剪贴板。
pub fn InputCopy() {
    if rich_editor_focused() {
        rte_copy();
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_copy_to_clipboard(st);
    }
}

/// Ctrl+X：剪切选区到剪贴板。
pub fn InputCut() {
    if rich_editor_focused() {
        rte_cut();
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_cut_to_clipboard(st);
    }
}

/// Ctrl+V：从剪贴板粘贴（替换选区，支持多行）。
pub fn InputPaste() {
    if rich_editor_focused() {
        rte_paste();
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_paste_from_clipboard(st);
    }
}

/// Ctrl+A：全选。
pub fn InputSelectAll() {
    if rich_editor_focused() {
        rte_select_all();
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        rich_select_all_field(st);
    }
}

/// Ctrl+Z：撤销。
pub fn InputUndo() {
    if rich_editor_focused() {
        rich_undo();
    }
}

/// Ctrl+Y / Ctrl+Shift+Z：重做。
pub fn InputRedo() {
    if rich_editor_focused() {
        rich_redo();
    }
}

/// F3：查找下一个命中。
pub fn InputFindNext() {
    if rich_editor_focused() {
        rich_find_next();
    }
}

/// 平台每帧按键时回报 Shift 状态（方向键据此扩展选区）。
pub fn SetShiftState(shift: bool) {
    SHIFT_HELD.with(|s| s.set(shift));
}

// ── 供 `rich_text_editor` 模块复用的焦点 / 剪贴板 / IME 辅助（pub(crate)） ──

/// 清空所有输入焦点（普通 TextField + RichTextEdit + Quill 富文本编辑器 + IME 合成）。
pub(crate) fn blur_fields() {
    set_field_focus(None);
    FOCUSED_RICH.with(|r| r.set(None));
    rich_editor_blur();
    PREEDIT.with(|p| p.borrow_mut().clear());
}

/// 读取线程局部剪贴板。
pub(crate) fn clipboard_get() -> String {
    CLIPBOARD.with(|c| c.borrow().clone())
}

/// 写入线程局部剪贴板。
pub(crate) fn clipboard_set(s: &str) {
    CLIPBOARD.with(|c| *c.borrow_mut() = s.to_string());
}

/// 公开：将文本写入线程局部剪贴板（供程序化「复制」按钮等使用）。
pub fn copy_text(s: &str) {
    CLIPBOARD.with(|c| *c.borrow_mut() = s.to_string());
}

/// 当前 IME 合成预览文本。
pub(crate) fn preedit_text() -> String {
    PREEDIT.with(|p| p.borrow().clone())
}

/// 清除 IME 合成预览文本。
pub(crate) fn preedit_reset() {
    PREEDIT.with(|p| p.borrow_mut().clear());
}

/// 当前是否按住 Shift（方向键据此扩展选区）。
pub(crate) fn shift_held() -> bool {
    SHIFT_HELD.with(|s| s.get())
}

/// 滚轮：调整当前聚焦 RichTextEdit 的垂直滚动偏移（像素，向下为正）。
/// 仅作用于已聚焦的多行编辑器（与单行 TextField 的 scope 滚动互不干扰）。
pub fn InputScroll(dy_px: f32) {
    if rich_editor_focused() {
        rich_scroll(dy_px);
        return;
    }
    if let Some(st) = FOCUSED_RICH.with(|f| f.get()) {
        if let Some(cell) = RICHTEXT_SCROLL.with(|m| m.borrow().get(&state_id(st)).cloned()) {
            cell.set(cell.get() + dy_px);
        }
    }
}

// ───────────────────────── 绘制 ─────────────────────────

#[allow(clippy::too_many_arguments)]
fn draw_rich(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    _w: f32,
    h: f32,
    pad_l: f32,
    pad_t: f32,
    pad_b: f32,
    line_h: f32,
    shown: &str,
    ph: &str,
    colors: &ColorScheme,
    cs: CaretSel,
    scroll: &std::rc::Rc<std::cell::Cell<f32>>,
    focused: bool,
) {
    let chars: Vec<char> = shown.chars().collect();
    let visible = (h - pad_t - pad_b).max(0.0);
    // 自动滚动使光标行可见
    let caret_line = line_col(&chars, cs.caret).0;
    let mut sy = scroll.get();
    if visible > 0.0 {
        if (caret_line as f32 + 1.0) * line_h - sy > visible {
            sy = (caret_line as f32 + 1.0) * line_h - visible;
        }
        if caret_line as f32 * line_h < sy {
            sy = (caret_line as f32 * line_h).max(0.0);
        }
    }
    let content_h = line_count(&chars) as f32 * line_h;
    let max_scroll = (content_h - visible).max(0.0);
    sy = sy.clamp(0.0, max_scroll);
    scroll.set(sy);

    let style = rich_style().color(colors.on_surface);

    // 选区高亮（逐行画矩形）
    if let Some(a) = cs.anchor {
        let s = a.min(cs.caret);
        let e = a.max(cs.caret);
        if s < e {
            let mut line = 0usize;
            let mut i = 0usize;
            while i < chars.len() {
                let line_start = i;
                let mut j = i;
                while j < chars.len() && chars[j] != '\n' {
                    j += 1;
                }
                let seg_s = s.max(line_start);
                let seg_e = e.min(j);
                if seg_s < seg_e {
                    let pre_s: String = chars[line_start..seg_s].iter().collect();
                    let pre_e: String = chars[line_start..seg_e].iter().collect();
                    let (ws, _) = measure_text_with_measurer(&pre_s, &style);
                    let (we, _) = measure_text_with_measurer(&pre_e, &style);
                    let ly = y + pad_t + line as f32 * line_h - sy;
                    b.fill_rect(
                        x + pad_l + ws,
                        ly,
                        (we - ws).max(1.0),
                        line_h,
                        colors.primary.with_alpha(0.3),
                    );
                }
                if j < chars.len() {
                    i = j + 1;
                } else {
                    i = j;
                }
                line += 1;
            }
        }
    }

    if chars.is_empty() {
        if focused {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            if (ms / 530) % 2 == 0 {
                b.fill_rect(x + pad_l + 1.0, y + pad_t - sy, 2.0, line_h, colors.primary);
            }
        } else {
            b.draw_text(
                x + pad_l,
                y + pad_t - sy,
                ph,
                &rich_style().color(colors.on_surface_variant),
                f32::INFINITY,
            );
        }
        return;
    }

    // 文本（逐行）
    let mut line = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let line_start = i;
        let mut j = i;
        while j < chars.len() && chars[j] != '\n' {
            j += 1;
        }
        let line_str: String = chars[line_start..j].iter().collect();
        let ly = y + pad_t + line as f32 * line_h - sy;
        b.draw_text(x + pad_l, ly, &line_str, &style, f32::INFINITY);
        if j < chars.len() {
            i = j + 1;
        } else {
            i = j;
        }
        line += 1;
    }

    // 光标 / IME 合成预览
    if focused {
        let cl = line_col(&chars, cs.caret).0;
        let line_start = idx_of_line_col(&chars, cl, 0);
        let prefix: String = chars[line_start..cs.caret].iter().collect();
        let (pw, _) = measure_text_with_measurer(&prefix, &style);
        let ly = y + pad_t + cl as f32 * line_h - sy;
        let pre = PREEDIT.with(|p| p.borrow().clone());
        if !pre.is_empty() {
            // 对齐 TextField：合成文本以 secondary 色绘制并加下划线
            let (prw, prh) = measure_text_with_measurer(&pre, &style);
            b.draw_text(
                x + pad_l + pw + 1.0,
                ly,
                &pre,
                &rich_style().color(colors.secondary),
                f32::INFINITY,
            );
            b.fill_rect(
                x + pad_l + pw + 1.0,
                ly + prh + 1.0,
                prw,
                1.0,
                colors.secondary,
            );
        } else {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            if (ms / 530) % 2 == 0 {
                b.fill_rect(x + pad_l + pw + 1.0, ly, 2.0, line_h, colors.primary);
            }
        }
    }
}

/// `RichTextEdit(value, placeholder, modifier)` — 多行富文本编辑器。
///
/// 支持：回车换行、上下键跨行、Home/End、Shift 方向键选区、Ctrl+A 全选、
/// Ctrl+C/V/X 复制/粘贴/剪切（线程局部剪贴板）、点击定位光标、选区高亮、光标闪烁。
pub fn RichTextEdit(value: MutableState<String>, placeholder: &str, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let pad_l = 12.0f32;
    let pad_t = 8.0f32;
    let pad_b = 8.0f32;
    let line_h = measure_text_with_measurer("Ag", &rich_style()).1;
    let mut m = GlassOrBg(modifier, g, colors.Surface, 8.0)
        .padding_ltrb(pad_l, pad_t, pad_l, pad_b)
        .min_height(120.0)
        .clip(8.0);
    if !g.enabled {
        m = m.border(1.5, colors.outline, 8.0);
    }
    let id = add_node(Policy::Box(Alignment::TopStart), m);

    let caret = remember(|| mutableStateOf(CaretSel::default()));
    RICHTEXT_CARET.with(|mm| {
        mm.borrow_mut().insert(state_id(value), caret);
    });
    let scroll = RICHTEXT_SCROLL.with(|mm| {
        mm.borrow_mut()
            .entry(state_id(value))
            .or_insert_with(|| std::rc::Rc::new(std::cell::Cell::new(0.0f32)))
            .clone()
    });
    let cs = caret.get();
    let shown = value.read_clone();
    let ph = placeholder.to_string();
    let focused = is_focused(value);
    let scroll_draw = scroll.clone();
    register_field_scope(value);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].text_value = Some(value);
        rt.nodes[id.index()].on_click = Some(Box::new(move || {
            set_field_focus(Some(value));
            FOCUSED_RICH.with(|r| r.set(Some(value)));
            PREEDIT.with(|p| p.borrow_mut().clear());
            let (lx, ly) = last_click_local();
            let chars: Vec<char> = value.read_clone().chars().collect();
            if chars.is_empty() {
                caret.set(CaretSel::default());
                return;
            }
            let total = line_count(&chars);
            let raw_line = (((ly - pad_t + scroll.get()) / line_h).max(0.0) as usize)
                .min(total.saturating_sub(1));
            let line_start = idx_of_line_col(&chars, raw_line, 0);
            let mut j = line_start;
            while j < chars.len() && chars[j] != '\n' {
                j += 1;
            }
            let line_text: String = chars[line_start..j].iter().collect();
            let col = caret_index_in_line(&line_text, (lx - pad_l).max(0.0));
            caret.set(CaretSel {
                caret: line_start + col,
                anchor: None,
            });
        }));
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            draw_rich(
                b,
                x,
                y,
                w,
                h,
                pad_l,
                pad_t,
                pad_b,
                line_h,
                &shown,
                &ph,
                &colors,
                cs,
                &scroll_draw,
                focused,
            );
        }));
    });
    caret_blink_driver(value, id);
}

#[cfg(test)]
mod richtext_tests {
    use super::*;
    use compose::runtime::NavKey;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }
    fn to_str(v: &[char]) -> String {
        v.iter().collect()
    }

    #[test]
    fn insert_at_caret() {
        let mut v = chars("hello");
        let mut sel = CaretSel {
            caret: 5,
            anchor: None,
        };
        rich_insert(&mut v, &mut sel, " world");
        assert_eq!(to_str(&v), "hello world");
        assert_eq!(sel.caret, 11);
    }

    #[test]
    fn insert_replaces_selection() {
        let mut v = chars("hello");
        let mut sel = CaretSel {
            caret: 5,
            anchor: Some(2),
        };
        rich_insert(&mut v, &mut sel, "X");
        assert_eq!(to_str(&v), "heX");
        assert_eq!(sel.caret, 3);
        assert_eq!(sel.anchor, None);
    }

    #[test]
    fn backspace_deletes_before() {
        let mut v = chars("hello");
        let mut sel = CaretSel {
            caret: 5,
            anchor: None,
        };
        rich_backspace(&mut v, &mut sel);
        assert_eq!(to_str(&v), "hell");
        assert_eq!(sel.caret, 4);
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut v = chars("hello");
        let mut sel = CaretSel {
            caret: 5,
            anchor: Some(2),
        };
        rich_backspace(&mut v, &mut sel);
        assert_eq!(to_str(&v), "he");
        assert_eq!(sel.caret, 2);
    }

    #[test]
    fn move_across_lines() {
        let v = chars("ab\ncd");
        let mut sel = CaretSel {
            caret: 1,
            anchor: None,
        };
        rich_move(&v, &mut sel, NavKey::Down, false);
        assert_eq!(sel.caret, 4);
        rich_move(&v, &mut sel, NavKey::Up, false);
        assert_eq!(sel.caret, 1);
        rich_move(&v, &mut sel, NavKey::Home, false);
        assert_eq!(sel.caret, 0);
        rich_move(&v, &mut sel, NavKey::End, false);
        assert_eq!(sel.caret, 2);
    }

    #[test]
    fn shift_selection_and_copy() {
        let v = chars("hello world");
        let mut sel = CaretSel {
            caret: 5,
            anchor: None,
        };
        rich_move(&v, &mut sel, NavKey::Right, true);
        assert_eq!(sel.anchor, Some(5));
        assert_eq!(sel.caret, 6);
        let mut clip = String::new();
        rich_copy(&v, &sel, &mut clip);
        assert_eq!(clip, " ");
    }

    #[test]
    fn copy_cut_paste() {
        let mut v = chars("hello");
        let mut sel = CaretSel {
            caret: 5,
            anchor: Some(0),
        };
        let mut clip = String::new();
        rich_copy(&v, &sel, &mut clip);
        assert_eq!(clip, "hello");
        rich_cut(&mut v, &mut sel, &mut clip);
        assert_eq!(to_str(&v), "");
        let mut v2: Vec<char> = Vec::new();
        let mut sel2 = CaretSel {
            caret: 0,
            anchor: None,
        };
        rich_paste(&mut v2, &mut sel2, "xyz");
        assert_eq!(to_str(&v2), "xyz");
        assert_eq!(sel2.caret, 3);
    }

    #[test]
    fn select_all_marks_anchor() {
        let v = chars("abc");
        let mut sel = CaretSel {
            caret: 0,
            anchor: None,
        };
        rich_select_all(&v, &mut sel);
        assert_eq!(sel.caret, 3);
        assert_eq!(sel.anchor, Some(0));
    }

    #[test]
    fn geometry_line_mapping() {
        let v = chars("ab\ncd");
        assert_eq!(line_count(&v), 2);
        assert_eq!(line_len_at(&v, 0), 2);
        assert_eq!(line_len_at(&v, 1), 2);
        assert_eq!(idx_of_line_col(&v, 1, 1), 4);
        assert_eq!(line_col(&v, 4), (1, 1));
    }
}
