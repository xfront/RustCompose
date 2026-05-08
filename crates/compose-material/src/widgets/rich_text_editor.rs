//! RichTextEditor — Quill 风格富文本编辑器。
//!
//! 已移植核心：
//!   - 块格式：Paragraph / H1-H3 / Blockquote / CodeBlock / OrderedList / UnorderedList / CheckList
//!   - 行内格式：Bold / Italic / Underline / Strikethrough / Code / 文字颜色 / 超链接
//!   - 光标、跨块选区、复制/剪切/粘贴（与 TextField 共用线程局部剪贴板）、全选
//!   - 内置工具栏（活动态高亮）、滚轮滚动、点击定位、IME 合成预览
//!   - `set_html` 导入（h1-h3/p/blockquote/pre/ul/ol/li/checkbox/span[color]/a[href]/实体）
//!
//! 暂未移植：撤销/重做、查找替换(F3)、缩进与对齐、
//! 上下标、字号/字体族、图片/视频/公式 embed、to_html/to_markdown 导出。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Column, CrossAxisAlignment, DrawBuilder, Modifier, Policy, Row,
    TextStyle, add_node, last_click_local, measure_text_with_measurer,
};
use compose::material::{glass_spec, theme_colors};
use compose::runtime::{MutableState, NavKey, mutableStateOf, remember, with_rt_pub};

use crate::tokens;

use crate::widgets::GlassOrBg;
use crate::widgets::input::{
    blur_fields, clipboard_get, clipboard_set, preedit_reset, preedit_text, shift_held,
};

// ── 常量 ─────────────────────────────────────────────────────────

const PAD_L: f32 = 16.0;
const PAD_T: f32 = 8.0;
const PAD_B: f32 = 8.0;
const LINE_SPACING: f32 = 1.35;
const BLOCK_SPACING: f32 = 4.0;
const TOOLBAR_BTN_W: f32 = 34.0;
const TOOLBAR_BTN_H: f32 = 24.0;

// ── 文档模型 ────────────────────────────────────────────────────────────────

/// 块格式类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Paragraph,
    H1,
    H2,
    H3,
    Blockquote,
    CodeBlock,
    OrderedList,
    UnorderedList,
    /// 任务列表：块首画复选框，`Block::checked` 为勾选状态。
    CheckList,
}

/// 行内格式。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    /// 文字颜色（None = 主题默认）。
    pub color: Option<Color>,
    /// 超链接目标（Some = 链接文本，渲染主题色 + 下划线）。
    pub link: Option<String>,
}

impl InlineFormat {
    pub fn is_default(&self) -> bool {
        *self == InlineFormat::default()
    }
}

/// 一段具有统一格式的文本。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub fmt: InlineFormat,
}

/// 文档块。
#[derive(Debug, Clone)]
pub struct Block {
    pub block_type: BlockType,
    pub runs: Vec<TextRun>,
    /// CheckList 勾选状态（仅 CheckList 有效）。
    pub checked: bool,
}

impl Block {
    pub fn new() -> Self {
        Block {
            block_type: BlockType::Paragraph,
            runs: Vec::new(),
            checked: false,
        }
    }

    /// 由纯文本构造（text 中的 \n 拆为多块）。
    pub fn from_plain(text: &str) -> Vec<Block> {
        text.split('\n')
            .map(|line| Block {
                block_type: BlockType::Paragraph,
                runs: if line.is_empty() {
                    Vec::new()
                } else {
                    vec![TextRun {
                        text: line.to_string(),
                        fmt: InlineFormat::default(),
                    }]
                },
                checked: false,
            })
            .collect()
    }

    /// 块的纯文本。
    pub fn flat_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// 编辑器内部状态。
pub struct EdState {
    pub blocks: Vec<Block>,
    /// 光标：块索引 + 块内字符偏移。
    pub cb: usize,
    pub co: usize,
    /// 选区锚点（块索引, 偏移）。
    pub anchor: Option<(usize, usize)>,
    /// 无选区时，后续输入携带的行内格式（Quill pending format）。
    pub pending: InlineFormat,
    pub scroll: f32,
    /// 绘制时重建的布局缓存：(块索引, 文档 y 顶, 行高)，供点击命中。
    pub layout: Vec<(usize, f32, f32)>,
}

pub type Ed = Rc<RefCell<EdState>>;

impl EdState {
    pub fn new() -> Self {
        EdState {
            blocks: vec![Block::new()],
            cb: 0,
            co: 0,
            anchor: None,
            pending: InlineFormat::default(),
            scroll: 0.0,
            layout: Vec::new(),
        }
    }

    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        let mut s = EdState::new();
        if !blocks.is_empty() {
            s.blocks = blocks;
        }
        s
    }

    fn current_bt(&self) -> BlockType {
        self.blocks[self.cb.min(self.blocks.len() - 1)].block_type
    }
}

// ── 纯操作辅助 ──────────────────────────────────────────────────────────────

fn block_len(b: &Block) -> usize {
    b.runs.iter().map(|r| r.text.chars().count()).sum()
}

/// 在字符偏移 `off` 处把 runs 切成（前, 后）两段。
fn split_runs(runs: &[TextRun], off: usize) -> (Vec<TextRun>, Vec<TextRun>) {
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut acc = 0usize;
    for r in runs {
        let len = r.text.chars().count();
        if acc + len <= off {
            before.push(r.clone());
        } else if acc >= off {
            after.push(r.clone());
        } else {
            let take = off - acc;
            let chars: Vec<char> = r.text.chars().collect();
            let mut a = r.clone();
            a.text = chars[..take].iter().collect();
            let mut b = r.clone();
            b.text = chars[take..].iter().collect();
            before.push(a);
            after.push(b);
        }
        acc += len;
    }
    (before, after)
}

/// 归一化选区 `((sb, so), (eb, eo))`，恒有 (sb,so) <= (eb,eo)。
fn sel_range(st: &EdState) -> ((usize, usize), (usize, usize)) {
    let a = st.anchor.unwrap_or((st.cb, st.co));
    let b = (st.cb, st.co);
    if a <= b { (a, b) } else { (b, a) }
}

fn has_selection(st: &EdState) -> bool {
    let ((sb, so), (eb, eo)) = sel_range(st);
    sb != eb || so != eo
}

fn clamp_caret(st: &mut EdState) {
    st.cb = st.cb.min(st.blocks.len() - 1);
    st.co = st.co.min(block_len(&st.blocks[st.cb]));
}

/// 在块的 `off` 处插入文本，相邻同格式 run 合并。
fn insert_into_block(b: &mut Block, off: usize, text: &str, fmt: &InlineFormat) {
    if text.is_empty() {
        return;
    }
    let (before, after) = split_runs(&b.runs, off);
    let mut runs = before;
    let mut merged = false;
    if let Some(last) = runs.last_mut() {
        if last.fmt == *fmt {
            last.text.push_str(text);
            merged = true;
        }
    }
    if !merged {
        runs.push(TextRun {
            text: text.to_string(),
            fmt: fmt.clone(),
        });
    }
    runs.extend(after);
    b.runs = runs;
}

fn delete_sel(st: &mut EdState) {
    let ((sb, so), (eb, eo)) = sel_range(st);
    if sb == eb {
        if so == eo {
            return;
        }
        let b = &mut st.blocks[sb];
        let (head, tail) = split_runs(&b.runs, eo);
        let (kept, _) = split_runs(&head, so);
        b.runs = kept;
        b.runs.extend(tail);
    } else {
        let tail_runs = split_runs(&st.blocks[eb].runs, eo).1;
        let head_runs = split_runs(&st.blocks[sb].runs, so).0;
        st.blocks[sb].runs = head_runs;
        st.blocks[sb].runs.extend(tail_runs);
        st.blocks.drain(sb + 1..=eb);
    }
    st.cb = sb;
    st.co = so;
    st.anchor = None;
    clamp_caret(st);
}

/// 插入文本（可含 \n，跨块拆分；列表内换行延续列表类型，其余回 Paragraph）。
fn insert_text(st: &mut EdState, s: &str) {
    if s.is_empty() {
        return;
    }
    delete_sel(st);
    let parts: Vec<&str> = s.split('\n').collect();
    let mut cur_b = st.cb;
    let mut cur_o = st.co;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            insert_into_block(&mut st.blocks[cur_b], cur_o, part, &st.pending);
            cur_o += part.chars().count();
        } else {
            let (head, tail) = split_runs(&st.blocks[cur_b].runs, cur_o);
            let bt = st.blocks[cur_b].block_type;
            st.blocks[cur_b].runs = head;
            let new_bt = match bt {
                BlockType::OrderedList | BlockType::UnorderedList | BlockType::CheckList => bt,
                _ => BlockType::Paragraph,
            };
            let mut nb = Block {
                block_type: new_bt,
                runs: tail,
                checked: false,
            };
            insert_into_block(&mut nb, 0, part, &st.pending);
            st.blocks.insert(cur_b + 1, nb);
            cur_b += 1;
            cur_o = part.chars().count();
        }
    }
    st.cb = cur_b;
    st.co = cur_o;
    st.anchor = None;
    clamp_caret(st);
}

fn backspace(st: &mut EdState) {
    if has_selection(st) {
        delete_sel(st);
        return;
    }
    if st.co > 0 {
        let b = &mut st.blocks[st.cb];
        let (before, after) = split_runs(&b.runs, st.co);
        let (kept, _) = split_runs(&before, st.co - 1);
        b.runs = kept;
        b.runs.extend(after);
        st.co -= 1;
    } else if st.cb > 0 {
        let cur = st.blocks.remove(st.cb);
        let prev_len = block_len(&st.blocks[st.cb - 1]);
        st.blocks[st.cb - 1].runs.extend(cur.runs);
        st.cb -= 1;
        st.co = prev_len;
    }
    st.anchor = None;
    clamp_caret(st);
}

fn delete_key(st: &mut EdState) {
    if has_selection(st) {
        delete_sel(st);
        return;
    }
    let len = block_len(&st.blocks[st.cb]);
    if st.co < len {
        let b = &mut st.blocks[st.cb];
        let (before, after) = split_runs(&b.runs, st.co + 1);
        let (kept, _) = split_runs(&before, st.co);
        b.runs = kept;
        b.runs.extend(after);
    } else if st.cb + 1 < st.blocks.len() {
        let next = st.blocks.remove(st.cb + 1);
        st.blocks[st.cb].runs.extend(next.runs);
    }
    st.anchor = None;
    clamp_caret(st);
}

fn enter(st: &mut EdState) {
    delete_sel(st);
    let is_list = matches!(
        st.blocks[st.cb].block_type,
        BlockType::OrderedList | BlockType::UnorderedList | BlockType::CheckList
    );
    if is_list && block_len(&st.blocks[st.cb]) == 0 {
        // 空列表项回车：退出列表（Quill 语义）
        st.blocks[st.cb].block_type = BlockType::Paragraph;
        st.blocks[st.cb].checked = false;
        return;
    }
    let (head, tail) = split_runs(&st.blocks[st.cb].runs, st.co);
    let bt = st.blocks[st.cb].block_type;
    st.blocks[st.cb].runs = head;
    let new_bt = if is_list { bt } else { BlockType::Paragraph };
    st.blocks.insert(
        st.cb + 1,
        Block {
            block_type: new_bt,
            runs: tail,
            checked: false,
        },
    );
    st.cb += 1;
    st.co = 0;
    st.anchor = None;
    clamp_caret(st);
}

fn move_key(st: &mut EdState, key: NavKey, shift: bool) {
    let n = st.blocks.len();
    let pos = match key {
        NavKey::Left => {
            if st.co > 0 {
                (st.cb, st.co - 1)
            } else if st.cb > 0 {
                (st.cb - 1, block_len(&st.blocks[st.cb - 1]))
            } else {
                (st.cb, st.co)
            }
        }
        NavKey::Right => {
            let l = block_len(&st.blocks[st.cb]);
            if st.co < l {
                (st.cb, st.co + 1)
            } else if st.cb + 1 < n {
                (st.cb + 1, 0)
            } else {
                (st.cb, st.co)
            }
        }
        NavKey::Up => {
            if st.cb > 0 {
                (st.cb - 1, st.co.min(block_len(&st.blocks[st.cb - 1])))
            } else {
                (st.cb, 0)
            }
        }
        NavKey::Down => {
            if st.cb + 1 < n {
                (st.cb + 1, st.co.min(block_len(&st.blocks[st.cb + 1])))
            } else {
                (st.cb, block_len(&st.blocks[st.cb]))
            }
        }
        NavKey::Home => (st.cb, 0),
        NavKey::End => (st.cb, block_len(&st.blocks[st.cb])),
        _ => (st.cb, st.co),
    };
    if shift {
        if st.anchor.is_none() {
            st.anchor = Some((st.cb, st.co));
        }
    } else {
        st.anchor = None;
    }
    st.cb = pos.0;
    st.co = pos.1;
    clamp_caret(st);
}

// ── 行内格式 ────────────────────────────────────────────────────────────────

/// 可切换的行内格式维度。
#[derive(Debug, Clone, Copy)]
pub enum FmtToggle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
}

fn fmt_get(f: &InlineFormat, t: FmtToggle) -> bool {
    match t {
        FmtToggle::Bold => f.bold,
        FmtToggle::Italic => f.italic,
        FmtToggle::Underline => f.underline,
        FmtToggle::Strikethrough => f.strikethrough,
        FmtToggle::Code => f.code,
    }
}

fn fmt_set(f: &mut InlineFormat, t: FmtToggle, v: bool) {
    match t {
        FmtToggle::Bold => f.bold = v,
        FmtToggle::Italic => f.italic = v,
        FmtToggle::Underline => f.underline = v,
        FmtToggle::Strikethrough => f.strikethrough = v,
        FmtToggle::Code => f.code = v,
    }
}

/// 对块内字符区间 [s, e) 应用格式变换。
fn map_runs_range(runs: &mut Vec<TextRun>, s: usize, e: usize, f: &impl Fn(&mut InlineFormat)) {
    let (pre, rest) = split_runs(runs, s);
    let (mut mid, mut post) = split_runs(&rest, e - s);
    for r in &mut mid {
        f(&mut r.fmt);
    }
    let mut out = pre;
    out.extend(mid);
    out.extend(post);
    *runs = out;
}

/// 选区内任一字符已带该格式 → 全部关掉；否则全部打开（Quill toggle 语义）。
fn toggle_inline(st: &mut EdState, t: FmtToggle) {
    if !has_selection(st) {
        let v = !fmt_get(&st.pending, t);
        fmt_set(&mut st.pending, t, v);
        return;
    }
    let ((sb, so), (eb, eo)) = sel_range(st);
    let mut any_on = false;
    for b in &st.blocks[sb..=eb] {
        for r in &b.runs {
            if fmt_get(&r.fmt, t) {
                any_on = true;
            }
        }
    }
    let target = !any_on;
    let apply = |f: &mut InlineFormat| fmt_set(f, t, target);
    if sb == eb {
        map_runs_range(&mut st.blocks[sb].runs, so, eo, &apply);
    } else {
        let sb_len = block_len(&st.blocks[sb]);
        map_runs_range(&mut st.blocks[sb].runs, so, sb_len, &apply);
        for i in sb + 1..eb {
            let l = block_len(&st.blocks[i]);
            map_runs_range(&mut st.blocks[i].runs, 0, l, &apply);
        }
        let eb_len = block_len(&st.blocks[eb]);
        map_runs_range(&mut st.blocks[eb].runs, 0, eb_len, &apply);
    }
}

/// 对选区（或 pending）应用任意格式变换。
fn map_sel(st: &mut EdState, apply: &impl Fn(&mut InlineFormat)) {
    if !has_selection(st) {
        apply(&mut st.pending);
        return;
    }
    let ((sb, so), (eb, eo)) = sel_range(st);
    if sb == eb {
        map_runs_range(&mut st.blocks[sb].runs, so, eo, apply);
    } else {
        let sb_len = block_len(&st.blocks[sb]);
        map_runs_range(&mut st.blocks[sb].runs, so, sb_len, apply);
        for i in sb + 1..eb {
            let l = block_len(&st.blocks[i]);
            map_runs_range(&mut st.blocks[i].runs, 0, l, apply);
        }
        map_runs_range(&mut st.blocks[eb].runs, 0, eo, apply);
    }
}

/// 设置超链接（空串移除）。
fn set_link(st: &mut EdState, url: &str) {
    let link = if url.is_empty() {
        None
    } else {
        Some(url.to_string())
    };
    let apply = move |f: &mut InlineFormat| f.link = link.clone();
    map_sel(st, &apply);
}

/// 设置文字颜色（None = 恢复默认）。
fn set_text_color(st: &mut EdState, color: Option<Color>) {
    let apply = move |f: &mut InlineFormat| f.color = color;
    map_sel(st, &apply);
}

/// 当前块切换块类型（相同则回 Paragraph）。
fn toggle_block(st: &mut EdState, bt: BlockType) {
    let i = st.cb.min(st.blocks.len() - 1);
    let cur = st.blocks[i].block_type;
    st.blocks[i].block_type = if cur == bt { BlockType::Paragraph } else { bt };
    if st.blocks[i].block_type != BlockType::CheckList {
        st.blocks[i].checked = false;
    }
}

/// 翻转当前 CheckList 块的勾选状态。
fn toggle_checked(st: &mut EdState) {
    let i = st.cb.min(st.blocks.len() - 1);
    if st.blocks[i].block_type == BlockType::CheckList {
        st.blocks[i].checked = !st.blocks[i].checked;
    }
}

/// 清除光标处/后续输入的全部行内格式（Quill Clean Format）。
fn clear_format(st: &mut EdState) {
    st.pending = InlineFormat::default();
    if has_selection(st) {
        let apply = |f: &mut InlineFormat| *f = InlineFormat::default();
        map_sel(st, &apply);
    }
}

/// 选区纯文本（跨块以 \n 连接）。
fn copy_sel(st: &EdState) -> String {
    let ((sb, so), (eb, eo)) = sel_range(st);
    if sb == eb {
        let chars: Vec<char> = st.blocks[sb].flat_text().chars().collect();
        return chars[so.min(chars.len())..eo.min(chars.len())]
            .iter()
            .collect();
    }
    let mut out = String::new();
    let head: Vec<char> = st.blocks[sb].flat_text().chars().collect();
    out.extend(head[so.min(head.len())..].iter());
    for b in &st.blocks[sb + 1..eb] {
        out.push('\n');
        out.push_str(&b.flat_text());
    }
    out.push('\n');
    let tail: Vec<char> = st.blocks[eb].flat_text().chars().collect();
    out.extend(tail[..eo.min(tail.len())].iter());
    out
}

fn select_all(st: &mut EdState) {
    let last = st.blocks.len() - 1;
    st.anchor = Some((0, 0));
    st.cb = last;
    st.co = block_len(&st.blocks[last]);
}

// ── HTML 导入（demo / 剪贴板粘贴所需的子集） ─────────────────────────────────

fn decode_entities(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != ';' && j - i < 10 {
                j += 1;
            }
            if j < chars.len() && chars[j] == ';' {
                let ent: String = chars[i + 1..j].iter().collect();
                let rep = match ent.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "nbsp" => Some('\u{a0}'),
                    _ => None,
                };
                if let Some(c) = rep {
                    out.push(c);
                    i = j + 1;
                    continue;
                }
            }
            out.push('&');
            i += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// 从属性串取 `color: #rrggbb`。
fn attr_color(attrs: &str) -> Option<Color> {
    let idx = attrs.find("color")?;
    let rest = &attrs[idx + 5..];
    let after = rest.split(':').nth(1)?;
    let hex: String = after
        .trim()
        .trim_start_matches('#')
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() < 6 {
        return None;
    }
    let ch = |r: std::ops::Range<usize>| u8::from_str_radix(&hex[r], 16).ok();
    Some(Color::rgba(
        ch(0..2)? as f32 / 255.0,
        ch(2..4)? as f32 / 255.0,
        ch(4..6)? as f32 / 255.0,
        1.0,
    ))
}

/// 从属性串取 `href="..."`。
fn attr_href(attrs: &str) -> Option<String> {
    let idx = attrs.find("href")?;
    let rest = &attrs[idx + 4..];
    let eq = rest.find('=')?;
    let rest = rest[eq + 1..].trim_start();
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let end = rest[1..].find(q)?;
    Some(rest[1..1 + end].to_string())
}

/// 极简 HTML → Block 解析（覆盖 demo 样例与常见剪贴板输出）。
pub fn parse_html(html: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut bt = BlockType::Paragraph;
    let mut checked = false;
    let mut list_kind: Option<BlockType> = None;
    let mut fmts: Vec<InlineFormat> = vec![InlineFormat::default()];
    let mut buf = String::new();

    macro_rules! flush_text {
        () => {{
            if !buf.is_empty() {
                runs.push(TextRun {
                    text: std::mem::take(&mut buf),
                    fmt: fmts.last().cloned().unwrap_or_default(),
                });
            }
        }};
    }
    macro_rules! flush_block {
        () => {{
            flush_text!();
            if !runs.is_empty() || checked {
                blocks.push(Block {
                    block_type: bt,
                    runs: std::mem::take(&mut runs),
                    checked,
                });
            }
            checked = false;
            bt = list_kind.unwrap_or(BlockType::Paragraph);
        }};
    }

    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            let tag: String = chars[i + 1..j.min(chars.len())].iter().collect();
            i = j + 1;
            let t = tag.trim();
            let lower = t.to_ascii_lowercase();
            if let Some(name) = lower.strip_prefix('/') {
                match name {
                    "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre" => {
                        flush_block!();
                        bt = list_kind.unwrap_or(BlockType::Paragraph);
                    }
                    "li" => flush_block!(),
                    "span" | "a" => {
                        flush_text!();
                        if fmts.len() > 1 {
                            fmts.pop();
                        }
                    }
                    _ => {}
                }
            } else {
                let name: String = lower
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                let attrs = if t.len() >= name.len() {
                    &t[name.len()..]
                } else {
                    ""
                };
                match name.as_str() {
                    "p" => {
                        flush_block!();
                        bt = BlockType::Paragraph;
                    }
                    "h1" => {
                        flush_block!();
                        bt = BlockType::H1;
                    }
                    "h2" => {
                        flush_block!();
                        bt = BlockType::H2;
                    }
                    "h3" => {
                        flush_block!();
                        bt = BlockType::H3;
                    }
                    "h4" | "h5" | "h6" => {
                        flush_block!();
                        bt = BlockType::H3;
                    }
                    "blockquote" => {
                        flush_block!();
                        bt = BlockType::Blockquote;
                    }
                    "pre" => {
                        flush_block!();
                        bt = BlockType::CodeBlock;
                    }
                    "ul" => list_kind = Some(BlockType::UnorderedList),
                    "ol" => list_kind = Some(BlockType::OrderedList),
                    "li" => {
                        flush_block!();
                        bt = list_kind.unwrap_or(BlockType::UnorderedList);
                    }
                    "input" => {
                        let la = attrs.to_ascii_lowercase();
                        if la.contains("checkbox") && la.contains("checked") {
                            checked = true;
                        }
                    }
                    "br" => flush_block!(),
                    "span" => {
                        flush_text!();
                        let mut f = fmts.last().cloned().unwrap_or_default();
                        if let Some(c) = attr_color(attrs) {
                            f.color = Some(c);
                        }
                        fmts.push(f);
                    }
                    "a" => {
                        flush_text!();
                        let mut f = fmts.last().cloned().unwrap_or_default();
                        if let Some(h) = attr_href(attrs) {
                            f.link = Some(h);
                            f.underline = true;
                        }
                        fmts.push(f);
                    }
                    _ => {}
                }
            }
        } else {
            let mut j = i;
            while j < chars.len() && chars[j] != '<' {
                j += 1;
            }
            let raw: String = chars[i..j].iter().collect();
            buf.push_str(&decode_entities(&raw));
            i = j;
        }
    }
    flush_block!();
    if blocks.is_empty() {
        blocks.push(Block::new());
    }
    blocks
}

// ── 焦点与平台事件路由（input.rs 各入口优先调用） ────────────────────────────

thread_local! {
    static RICH_ED_FOCUS: RefCell<Option<Ed>> = const { RefCell::new(None) };
}

fn with_ed(f: impl FnOnce(&mut EdState)) {
    let ed = RICH_ED_FOCUS.with(|f| f.borrow().clone());
    if let Some(ed) = ed {
        f(&mut ed.borrow_mut());
    }
}

/// 是否有 RichTextEditor 持有键盘焦点。
pub fn rich_editor_focused() -> bool {
    RICH_ED_FOCUS.with(|f| f.borrow().is_some())
}

/// 失焦（点击普通输入框 / 单行 RichTextEdit 时由 input.rs 调用）。
pub fn rich_editor_blur() {
    RICH_ED_FOCUS.with(|f| *f.borrow_mut() = None);
}

fn focus(ed: Ed) {
    RICH_ED_FOCUS.with(|f| *f.borrow_mut() = Some(ed));
}

/// 字符输入（平台路由）。
pub fn rich_char(c: char) {
    with_ed(|s| insert_text(s, &c.to_string()));
}

/// 退格。
pub fn rich_backspace() {
    with_ed(backspace);
}

/// 导航键（含 Delete）。
pub fn rich_key(key: NavKey) {
    with_ed(|s| match key {
        NavKey::Delete => delete_key(s),
        _ => move_key(s, key, shift_held()),
    });
}

/// 回车（换行 / 拆块 / 退出列表）。
pub fn rich_enter() {
    with_ed(enter);
}

/// IME 提交。
pub fn rich_commit(s: &str) {
    with_ed(move |st| insert_text(st, s));
}

/// 复制选区。
pub fn rich_copy() {
    with_ed(|s| {
        let t = copy_sel(s);
        if !t.is_empty() {
            clipboard_set(&t);
        }
    });
}

/// 剪切选区。
pub fn rich_cut() {
    with_ed(|s| {
        let t = copy_sel(s);
        if !t.is_empty() {
            clipboard_set(&t);
            delete_sel(s);
        }
    });
}

/// 粘贴（纯文本，含 \n 跨块）。
pub fn rich_paste() {
    let t = clipboard_get();
    if !t.is_empty() {
        with_ed(move |s| insert_text(s, &t));
    }
}

/// 全选。
pub fn rich_select_all() {
    with_ed(select_all);
}

/// 滚轮（聚焦编辑器滚动；越界由 draw 夹取）。
pub fn rich_scroll(dy_px: f32) {
    with_ed(move |s| s.scroll += dy_px);
}

// ── 绘制 ────────────────────────────────────────────────────────────────────

/// 绘制所需的颜色快照（`Color` 均为 Copy，draw 闭包捕获安全）。
#[derive(Clone, Copy)]
struct EdColors {
    on_surface: Color,
    on_surface_variant: Color,
    primary: Color,
    primary_container: Color,
    surface_variant: Color,
    secondary: Color,
    outline: Color,
}

impl EdColors {
    fn capture() -> Self {
        let c = theme_colors();
        EdColors {
            on_surface: c.on_surface,
            on_surface_variant: c.on_surface_variant,
            primary: c.primary,
            primary_container: c.primary_container,
            surface_variant: c.surface_variant,
            secondary: c.secondary,
            outline: c.outline,
        }
    }

    fn run_color(&self, r: &TextRun) -> Color {
        if let Some(c) = r.fmt.color {
            c
        } else if r.fmt.link.is_some() {
            self.primary
        } else if r.fmt.code {
            self.secondary
        } else {
            self.on_surface
        }
    }
}

fn block_font_size(bt: BlockType) -> f32 {
    match bt {
        BlockType::H1 => tokens::headline_small(Color::BLACK).size,
        BlockType::H2 => tokens::title_large(Color::BLACK).size,
        BlockType::H3 => tokens::title_medium(Color::BLACK).size,
        _ => tokens::body_large(Color::BLACK).size,
    }
}

/// 标题块用 600 中粗（M3 标题不是全粗，正文标题同轴）。
fn block_weight(bt: BlockType) -> u16 {
    match bt {
        BlockType::H1 | BlockType::H2 | BlockType::H3 => 600,
        _ => 400,
    }
}

fn block_indent(bt: BlockType) -> f32 {
    match bt {
        BlockType::OrderedList | BlockType::UnorderedList | BlockType::CheckList => 26.0,
        BlockType::Blockquote => 24.0,
        BlockType::CodeBlock => 12.0,
        _ => 0.0,
    }
}

fn run_style(r: &TextRun, bt: BlockType, size: f32, ec: &EdColors) -> TextStyle {
    let mut s = TextStyle::new().size(size).weight(block_weight(bt));
    if r.fmt.bold {
        s = s.weight(700);
    }
    s.color(ec.run_color(r))
}

fn run_px(r: &TextRun, bt: BlockType, size: f32, ec: &EdColors, upto: usize) -> f32 {
    let chars: Vec<char> = r.text.chars().collect();
    let st = run_style(r, bt, size, ec);
    let p: String = chars[..upto.min(chars.len())].iter().collect();
    measure_text_with_measurer(&p, &st).0
}

fn prefix_width(b: &Block, off: usize, size: f32, ec: &EdColors) -> f32 {
    let mut w = 0.0f32;
    let mut acc = 0usize;
    for r in &b.runs {
        let len = r.text.chars().count();
        if acc + len <= off {
            w += run_px(r, b.block_type, size, ec, len);
        } else if acc < off {
            w += run_px(r, b.block_type, size, ec, off - acc);
            break;
        } else {
            break;
        }
        acc += len;
    }
    w
}

/// 横向像素 → 块内字符偏移。
fn x_to_col(b: &Block, mut dx: f32, size: f32, ec: &EdColors) -> usize {
    let mut acc = 0usize;
    for r in &b.runs {
        let chars: Vec<char> = r.text.chars().collect();
        let st = run_style(r, b.block_type, size, ec);
        let mut prev = 0.0f32;
        for (i, _) in chars.iter().enumerate() {
            let p: String = chars[..=i].iter().collect();
            let cw = measure_text_with_measurer(&p, &st).0;
            if dx < (prev + cw) * 0.5 {
                return acc + i;
            }
            prev = cw;
        }
        dx -= measure_text_with_measurer(&r.text, &st).0;
        acc += chars.len();
    }
    acc
}

fn draw_editor(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    st: &mut EdState,
    ec: &EdColors,
    focused: bool,
) {
    // 重建布局缓存 + 夹取滚动
    st.layout.clear();
    let mut dy = PAD_T;
    for (i, blk) in st.blocks.iter().enumerate() {
        let size = block_font_size(blk.block_type);
        let lh = size * LINE_SPACING;
        st.layout.push((i, dy, lh));
        dy += lh + BLOCK_SPACING;
    }
    let content_h = (dy - BLOCK_SPACING + PAD_B).max(h);
    let max_scroll = (content_h - h).max(0.0);
    st.scroll = st.scroll.clamp(0.0, max_scroll);
    let scroll = st.scroll;

    let ((ssb, sso), (seb, seo)) = sel_range(st);
    let has_sel = (ssb, sso) != (seb, seo);
    let pre = if focused {
        preedit_text()
    } else {
        String::new()
    };

    for (i, blk) in st.blocks.iter().enumerate() {
        let bt = blk.block_type;
        let size = block_font_size(bt);
        let lh = size * LINE_SPACING;
        let ly = y + PAD_T + st.layout[i].1 - scroll;
        if ly + lh < y || ly > y + h {
            continue; // 视口裁剪
        }
        let base_x = x + PAD_L + block_indent(bt);
        let text_y = ly + (lh - size * 1.2).max(0.0) * 0.5;

        // 块装饰
        match bt {
            BlockType::Blockquote => {
                b.fill_rect(x + 8.0, ly + 2.0, 3.0, lh - 4.0, ec.primary.with_alpha(0.6));
            }
            BlockType::CodeBlock => {
                b.fill_rect(x + 4.0, ly, (w - 8.0).max(0.0), lh, ec.surface_variant);
            }
            BlockType::UnorderedList => {
                b.draw_text(
                    x + PAD_L + 8.0,
                    text_y,
                    "•",
                    &TextStyle::new().size(size).color(ec.on_surface_variant),
                    f32::INFINITY,
                );
            }
            BlockType::OrderedList => {
                let mut n = 1usize;
                for j in (0..i).rev() {
                    if st.blocks[j].block_type == BlockType::OrderedList {
                        n += 1;
                    } else {
                        break;
                    }
                }
                b.draw_text(
                    x + PAD_L,
                    text_y,
                    &format!("{n}."),
                    &TextStyle::new().size(size).color(ec.on_surface_variant),
                    f32::INFINITY,
                );
            }
            BlockType::CheckList => {
                let bs = 14.0;
                let by = ly + (lh - bs) * 0.5;
                if blk.checked {
                    b.fill_rect(x + PAD_L + 4.0, by, bs, bs, ec.primary);
                    b.draw_text(
                        x + PAD_L + 6.0,
                        by - 1.0,
                        "✓",
                        &tokens::label_small(Color::rgba(1.0, 1.0, 1.0, 0.95)),
                        f32::INFINITY,
                    );
                } else {
                    b.fill_rect(x + PAD_L + 4.0, by, bs, bs, ec.surface_variant);
                    b.fill_rect(x + PAD_L + 4.0, by, bs, 1.2, ec.outline);
                    b.fill_rect(x + PAD_L + 4.0, by + bs - 1.2, bs, 1.2, ec.outline);
                    b.fill_rect(x + PAD_L + 4.0, by, 1.2, bs, ec.outline);
                    b.fill_rect(x + PAD_L + 4.0 + bs - 1.2, by, 1.2, bs, ec.outline);
                }
            }
            _ => {}
        }

        // 中间块整行选区
        if has_sel && i > ssb && i < seb {
            b.fill_rect(
                x + 2.0,
                ly,
                (w - 4.0).max(0.0),
                lh,
                ec.primary.with_alpha(0.3),
            );
        }

        // runs
        let mut cx = base_x;
        let mut col0 = 0usize;
        for r in &blk.runs {
            let len = r.text.chars().count();
            let rstyle = run_style(r, bt, size, ec);
            let rw = measure_text_with_measurer(&r.text, &rstyle).0;
            // 选区高亮（与本 run 的交集）
            if has_sel && i >= ssb && i <= seb {
                let s_off = if i == ssb { sso } else { 0 };
                let e_off = if i == seb { seo } else { len };
                let os = s_off.max(col0).saturating_sub(col0);
                let oe = e_off.min(col0 + len).saturating_sub(col0);
                if oe > os {
                    let x1 = cx + run_px(r, bt, size, ec, os);
                    let x2 = cx + run_px(r, bt, size, ec, oe);
                    b.fill_rect(x1, ly, (x2 - x1).max(1.0), lh, ec.primary.with_alpha(0.3));
                }
            }
            b.draw_text(cx, text_y, &r.text, &rstyle, f32::INFINITY);
            let col = ec.run_color(r);
            if r.fmt.underline || r.fmt.link.is_some() {
                b.fill_rect(cx, ly + lh - 2.5, rw, 1.0, col);
            }
            if r.fmt.strikethrough || (bt == BlockType::CheckList && blk.checked) {
                b.fill_rect(cx, ly + lh * 0.55, rw, 1.0, col);
            }
            if r.fmt.code {
                b.fill_rect(
                    cx,
                    ly + 1.0,
                    rw,
                    lh - 2.0,
                    ec.surface_variant.with_alpha(0.5),
                );
            }
            cx += rw;
            col0 += len;
        }
    }

    // 光标 / IME 合成预览
    if focused {
        let cb = st.cb.min(st.blocks.len() - 1);
        let blk = &st.blocks[cb];
        let size = block_font_size(blk.block_type);
        let lh = size * LINE_SPACING;
        let text_h = size * 1.2;
        let cy = y + PAD_T + st.layout[cb].1 - scroll;
        let cx = x + PAD_L + block_indent(blk.block_type) + prefix_width(blk, st.co, size, ec);
        if !pre.is_empty() {
            let pstyle = TextStyle::new().size(size).color(ec.secondary);
            let (prw, prh) = measure_text_with_measurer(&pre, &pstyle);
            b.draw_text(
                cx + 1.0,
                cy + (lh - prh).max(0.0) * 0.5,
                &pre,
                &pstyle,
                f32::INFINITY,
            );
            b.fill_rect(cx + 1.0, cy + prh + 1.0, prw, 1.0, ec.secondary);
        } else {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            if (ms / 530) % 2 == 0 {
                b.fill_rect(cx + 1.0, cy + (lh - text_h) * 0.5, 2.0, text_h, ec.primary);
            }
        }
    }
}

/// 点击 → 光标定位（含 CheckList 复选框命中）。
fn click_to_caret(st: &mut EdState, lx: f32, ly: f32, ec: &EdColors) {
    if st.layout.is_empty() {
        return;
    }
    let doc_y = ly + st.scroll;
    let mut target = st.layout[0].0;
    for &e in &st.layout {
        target = e.0;
        if doc_y < e.1 + e.2 + BLOCK_SPACING {
            break;
        }
    }
    let bt = st.blocks[target].block_type;
    if bt == BlockType::CheckList && lx >= PAD_L + 2.0 && lx <= PAD_L + 22.0 {
        st.blocks[target].checked = !st.blocks[target].checked;
        return;
    }
    let dx = (lx - PAD_L - block_indent(bt)).max(0.0);
    st.cb = target;
    st.co = x_to_col(&st.blocks[target], dx, block_font_size(bt), ec);
    st.anchor = None;
    clamp_caret(st);
}

// ── 工具栏 ──────────────────────────────────────────────────────────────────

fn tbtn(
    label: &str,
    active: bool,
    rev: MutableState<u32>,
    ed: &Ed,
    action: impl Fn(&mut EdState) + 'static,
) {
    let label = label.to_string();
    let ed = ed.clone();
    let ec = EdColors::capture();
    let id = add_node(
        Policy::Leaf,
        Modifier::new().size(TOOLBAR_BTN_W, TOOLBAR_BTN_H),
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(move || {
            action(&mut ed.borrow_mut());
            rev.update(|v| *v += 1);
        }));
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            if active {
                b.fill_rect(x, y, w, h, ec.primary_container);
            }
            let st = tokens::label_small(if active { ec.primary } else { ec.on_surface })
                .weight(if active { 700 } else { 500 });
            let (lw, lh) = measure_text_with_measurer(&label, &st);
            b.draw_text(
                x + (w - lw) * 0.5,
                y + (h - lh) * 0.5,
                &label,
                &st,
                f32::INFINITY,
            );
        }));
    });
}

fn toolbar_row(ed: &Ed, rev: MutableState<u32>) {
    let ed2 = ed.clone();
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let _ = rev.get(); // 订阅：工具栏动作后重组刷新活动态
            let (cur, pend) = {
                let s = ed2.borrow();
                (s.current_bt(), s.pending.clone())
            };
            tbtn("¶", cur == BlockType::Paragraph, rev, &ed2, |s| {
                toggle_block(s, BlockType::Paragraph)
            });
            tbtn("H1", cur == BlockType::H1, rev, &ed2, |s| {
                toggle_block(s, BlockType::H1)
            });
            tbtn("H2", cur == BlockType::H2, rev, &ed2, |s| {
                toggle_block(s, BlockType::H2)
            });
            tbtn("H3", cur == BlockType::H3, rev, &ed2, |s| {
                toggle_block(s, BlockType::H3)
            });
            tbtn("B", pend.bold, rev, &ed2, |s| {
                toggle_inline(s, FmtToggle::Bold)
            });
            tbtn("I", pend.italic, rev, &ed2, |s| {
                toggle_inline(s, FmtToggle::Italic)
            });
            tbtn("U", pend.underline, rev, &ed2, |s| {
                toggle_inline(s, FmtToggle::Underline)
            });
            tbtn("S", pend.strikethrough, rev, &ed2, |s| {
                toggle_inline(s, FmtToggle::Strikethrough)
            });
            tbtn("</>", pend.code, rev, &ed2, |s| {
                toggle_inline(s, FmtToggle::Code)
            });
            tbtn("❝", cur == BlockType::Blockquote, rev, &ed2, |s| {
                toggle_block(s, BlockType::Blockquote)
            });
            tbtn("UL", cur == BlockType::UnorderedList, rev, &ed2, |s| {
                toggle_block(s, BlockType::UnorderedList)
            });
            tbtn("OL", cur == BlockType::OrderedList, rev, &ed2, |s| {
                toggle_block(s, BlockType::OrderedList)
            });
            tbtn("☑", cur == BlockType::CheckList, rev, &ed2, |s| {
                toggle_block(s, BlockType::CheckList)
            });
            tbtn("⌫F", false, rev, &ed2, clear_format);
        },
    );
}

// ── 对外控件 ────────────────────────────────────────────────────────────────

/// `RichTextEditor` 的句柄：内容注入与程序化操作入口。
#[derive(Clone)]
pub struct RichTextEditorHandle {
    ed: Ed,
    rev: MutableState<u32>,
}

impl RichTextEditorHandle {
    fn bump(&self) {
        self.rev.update(|v| *v += 1);
    }

    /// 导入 HTML（`set_html`）。
    pub fn set_html(&self, html: &str) {
        *self.ed.borrow_mut() = EdState::from_blocks(parse_html(html));
        self.bump();
    }

    /// 直接替换文档块。
    pub fn set_blocks(&self, blocks: Vec<Block>) {
        *self.ed.borrow_mut() = EdState::from_blocks(blocks);
        self.bump();
    }

    /// 当前文档块快照。
    pub fn blocks(&self) -> Vec<Block> {
        self.ed.borrow().blocks.clone()
    }

    /// 纯文本（跨块 \n 连接）。
    pub fn to_plain_text(&self) -> String {
        let s = self.ed.borrow();
        s.blocks
            .iter()
            .map(|b| b.flat_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 字数（Unicode 字符数，跨块合计）。
    pub fn char_count(&self) -> usize {
        self.ed
            .borrow()
            .blocks
            .iter()
            .map(|b| b.flat_text().chars().count())
            .sum()
    }

    pub fn toggle_bold(&self) {
        self.with(|s| toggle_inline(s, FmtToggle::Bold));
    }
    pub fn toggle_italic(&self) {
        self.with(|s| toggle_inline(s, FmtToggle::Italic));
    }
    pub fn toggle_underline(&self) {
        self.with(|s| toggle_inline(s, FmtToggle::Underline));
    }
    pub fn toggle_strikethrough(&self) {
        self.with(|s| toggle_inline(s, FmtToggle::Strikethrough));
    }
    pub fn toggle_code(&self) {
        self.with(|s| toggle_inline(s, FmtToggle::Code));
    }
    /// 设置超链接（空串移除）。
    pub fn set_link(&self, url: &str) {
        let u = url.to_string();
        self.with(move |s| set_link(s, &u));
    }
    /// 设置文字颜色（None 恢复默认）。
    pub fn set_text_color(&self, color: Option<Color>) {
        self.with(move |s| set_text_color(s, color));
    }
    /// 清除格式。
    pub fn clear_format(&self) {
        self.with(clear_format);
    }
    /// 当前块切换块类型。
    pub fn toggle_block_type(&self, bt: BlockType) {
        self.with(move |s| toggle_block(s, bt));
    }
    /// 翻转当前任务列表勾选。
    pub fn toggle_checked(&self) {
        self.with(toggle_checked);
    }

    fn with(&self, f: impl FnOnce(&mut EdState)) {
        f(&mut self.ed.borrow_mut());
        self.bump();
    }
}

/// `RichTextEditor(modifier)` — Quill 风格富文本编辑器（工具栏 + 编辑区）。
///
/// `modifier` 作用于编辑区（建议 `fill_max_width().height(..)`）。
/// 返回句柄用于注入内容（`set_html` / `set_blocks`）与程序化操作。
pub fn RichTextEditor(modifier: Modifier) -> RichTextEditorHandle {
    let ed: Ed = remember(|| Rc::new(RefCell::new(EdState::new())));
    let rev = remember(|| mutableStateOf(0u32));

    let ed_col = ed.clone();
    let rev_col = rev;
    let user_mod = modifier;
    Column(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            let _ = rev_col.get(); // 订阅：句柄操作后重组
            toolbar_row(&ed_col, rev_col);
            compose::layout::Spacer(Modifier::new().height(6.0));

            // 编辑区（glass / 描边同 TextField）
            let g = glass_spec();
            let ec = EdColors::capture();
            let mut m = GlassOrBg(user_mod.clone(), g, theme_colors().Surface, 8.0).clip(8.0);
            if !g.enabled {
                m = m.border(1.5, ec.outline, 8.0);
            }
            let id = add_node(Policy::Box(Alignment::TopStart), m);
            let ed_click = ed_col.clone();
            let ed_draw = ed_col.clone();
            with_rt_pub(|rt| {
                rt.nodes[id.index()].on_click = Some(Box::new(move || {
                    focus(ed_click.clone());
                    blur_fields();
                    preedit_reset();
                    let (lx, ly) = last_click_local();
                    let ec2 = EdColors::capture();
                    click_to_caret(&mut ed_click.borrow_mut(), lx, ly, &ec2);
                }));
                rt.nodes[id.index()].draw =
                    Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                        let mut st = ed_draw.borrow_mut();
                        draw_editor(b, x, y, w, h, &mut st, &ec, rich_editor_focused());
                    }));
            });
            // 焦点期间只在闪烁相位翻转时失效编辑区节点（含子树与祖先）：否则光标被绘制
            // 缓存冻结，且增量快路径会因根未脏而直接回放旧画面。id 经 `remember` 容器刷新，
            // 不会指向陈旧节点；闪烁走定点唤醒，不让聚焦把整个应用拖回满帧率。
            crate::widgets::input::blink_caret_while(id, rich_editor_focused);
        },
    );

    RichTextEditorHandle { ed, rev }
}

// ── 单元测试 ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn st_with(lines: &[&str]) -> EdState {
        EdState::from_blocks(Block::from_plain(&lines.join("\n")))
    }

    fn flat(st: &EdState) -> String {
        st.blocks
            .iter()
            .map(|b| b.flat_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn insert_and_backspace() {
        let mut s = st_with(&["hello"]);
        s.cb = 0;
        s.co = 5;
        insert_text(&mut s, " world");
        assert_eq!(flat(&s), "hello world");
        assert_eq!((s.cb, s.co), (0, 11));
        backspace(&mut s);
        backspace(&mut s);
        assert_eq!(flat(&s), "hello wor");
    }

    #[test]
    fn insert_replaces_selection() {
        let mut s = st_with(&["hello"]);
        s.anchor = Some((0, 1));
        s.cb = 0;
        s.co = 4; // 选中 "ell"
        insert_text(&mut s, "X");
        assert_eq!(flat(&s), "hXo");
        assert_eq!(s.anchor, None);
    }

    #[test]
    fn enter_splits_block_and_merges_back() {
        let mut s = st_with(&["ab", "cd"]);
        s.cb = 0;
        s.co = 1;
        enter(&mut s);
        assert_eq!(flat(&s), "a\nb\ncd");
        assert_eq!(s.blocks.len(), 3);
        assert_eq!((s.cb, s.co), (1, 0));
        backspace(&mut s); // 合并回上一块
        assert_eq!(flat(&s), "ab\ncd");
        assert_eq!(s.blocks.len(), 2);
    }

    #[test]
    fn enter_on_heading_resets_to_paragraph() {
        let mut s = st_with(&["Title"]);
        s.blocks[0].block_type = BlockType::H2;
        s.cb = 0;
        s.co = 5;
        enter(&mut s);
        assert_eq!(s.blocks[0].block_type, BlockType::H2);
        assert_eq!(s.blocks[1].block_type, BlockType::Paragraph);
    }

    #[test]
    fn enter_on_empty_list_item_exits_list() {
        let mut s = st_with(&["item", ""]);
        s.blocks[0].block_type = BlockType::UnorderedList;
        s.blocks[1].block_type = BlockType::UnorderedList;
        s.cb = 1;
        s.co = 0;
        enter(&mut s);
        assert_eq!(s.blocks.len(), 2);
        assert_eq!(s.blocks[1].block_type, BlockType::Paragraph);
    }

    #[test]
    fn delete_selection_across_blocks() {
        let mut s = st_with(&["abcd", "efgh", "ijkl"]);
        s.anchor = Some((0, 2)); // "cd\nefgh\nij"
        s.cb = 2;
        s.co = 2;
        delete_sel(&mut s);
        assert_eq!(flat(&s), "abkl");
        assert_eq!(s.blocks.len(), 1);
    }

    #[test]
    fn toggle_inline_on_selection() {
        let mut s = st_with(&["abcdef"]);
        s.anchor = Some((0, 1));
        s.cb = 0;
        s.co = 5; // 选中 [1,5) = "bcde"
        toggle_inline(&mut s, FmtToggle::Bold);
        let runs = &s.blocks[0].runs;
        assert_eq!(runs.len(), 3);
        assert!(!runs[0].fmt.bold);
        assert!(runs[1].fmt.bold);
        assert_eq!(runs[1].text, "bcde");
        // 再次 toggle → 取消
        toggle_inline(&mut s, FmtToggle::Bold);
        assert!(s.blocks[0].runs.iter().all(|r| !r.fmt.bold));
    }

    #[test]
    fn pending_format_applies_to_typed_text() {
        let mut s = st_with(&["ab"]);
        s.cb = 0;
        s.co = 2;
        toggle_inline(&mut s, FmtToggle::Bold); // 无选区 → pending
        assert!(s.pending.bold);
        insert_text(&mut s, "C");
        assert_eq!(flat(&s), "abC");
        let runs = &s.blocks[0].runs;
        assert!(runs.last().unwrap().fmt.bold);
        assert_eq!(runs.last().unwrap().text, "C");
    }

    #[test]
    fn copy_spans_blocks() {
        let mut s = st_with(&["abcd", "efgh"]);
        s.anchor = Some((0, 2));
        s.cb = 1;
        s.co = 2;
        assert_eq!(copy_sel(&s), "cd\nef");
    }

    #[test]
    fn paste_with_newlines_splits_blocks() {
        let mut s = st_with(&["ab"]);
        s.cb = 0;
        s.co = 1;
        insert_text(&mut s, "X\nY\nZ");
        assert_eq!(flat(&s), "aX\nY\nZb");
        assert_eq!(s.blocks.len(), 3);
    }

    #[test]
    fn html_import_demo_sample() {
        let html = "<h2>Rich Text Editor Demo</h2>\
<p>Welcome to the <span style=\"color: #3577ff;\">Quill-like</span> rich text editor for <a href=\"https://quilljs.com\">Compose</a>.</p>\
<blockquote>Supports headings, lists, code blocks, find &amp; replace (F3) and more.</blockquote>\
<ul><li><input type=\"checkbox\" checked>HTML import &amp; clipboard paste (Ctrl+V)</li>\
<li><input type=\"checkbox\">video / formula embed</li>\
<li><input type=\"checkbox\">export to_html / to_markdown</li></ul>";
        let blocks = parse_html(html);
        assert_eq!(blocks.len(), 6);
        assert_eq!(blocks[0].block_type, BlockType::H2);
        assert_eq!(blocks[0].flat_text(), "Rich Text Editor Demo");
        assert_eq!(blocks[1].block_type, BlockType::Paragraph);
        let runs = &blocks[1].runs;
        assert!(
            runs.iter()
                .any(|r| r.fmt.color.is_some() && r.text.contains("Quill-like"))
        );
        assert!(
            runs.iter()
                .any(|r| r.fmt.link.as_deref() == Some("https://quilljs.com"))
        );
        assert_eq!(blocks[2].block_type, BlockType::Blockquote);
        assert!(blocks[2].flat_text().contains("find & replace"));
        assert_eq!(blocks[3].block_type, BlockType::UnorderedList);
        assert!(blocks[3].checked);
        assert!(!blocks[4].checked);
        assert_eq!(
            blocks[3].flat_text(),
            "HTML import & clipboard paste (Ctrl+V)"
        );
    }

    #[test]
    fn block_type_toggle_and_checklist() {
        let mut s = st_with(&["task"]);
        toggle_block(&mut s, BlockType::CheckList);
        assert_eq!(s.blocks[0].block_type, BlockType::CheckList);
        toggle_checked(&mut s);
        assert!(s.blocks[0].checked);
        toggle_block(&mut s, BlockType::CheckList); // 再切回 Paragraph
        assert_eq!(s.blocks[0].block_type, BlockType::Paragraph);
        assert!(!s.blocks[0].checked);
    }

    #[test]
    fn select_all_and_copy() {
        let mut s = st_with(&["ab", "cde"]);
        select_all(&mut s);
        assert_eq!(copy_sel(&s), "ab\ncde");
        assert_eq!(s.anchor, Some((0, 0)));
        assert_eq!((s.cb, s.co), (1, 3));
    }
}
