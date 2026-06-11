//! Semantics — 对齐 Jetpack Compose `androidx.compose.ui.semantics`：语义树与无障碍。
//!
//! Compose 用 `SemanticsNode` 树向无障碍服务（TalkBack / VoiceOver / ARIA）描述 UI：
//! 每个节点有角色(role)、标签(label/contentDescription)、状态(selected/checked/disabled…)、
//! 边界(bounds)、动作(click/scroll/focus…)，并支持 `mergeDescendants` 合并叶子文本、
//! `clearAndSetSemantics` 覆盖子语义。
//!
//! 本实现适配工程 arena 模型（零 `Rc`/`unsafe`）：
//! - [`SemanticsPropertyKey<T>`]：类型化属性键（对齐 Compose `SemanticsPropertyKey`）；
//! - [`SemanticsConfig`]：节点语义配置（属性 + 动作 + 合并策略）；
//! - [`SemanticsNode`]/[`SemanticsNodeId`]：语义树节点与 arena 句柄；
//! - [`SemanticsTree`]：构建好的语义树，支持查询 / 坐标命中 / 执行动作 / 焦点 / JSON 导出。
//!
//! 属性用 `HashMap<&'static str, Box<dyn Any>>` 存类型擦除值，键即 Compose 的
//! `SemanticsProperties.Name` 等标准名；读写经类型化 `get/set` 保证类型安全。

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::layout::NodeId;

/// 类型化语义属性键（对齐 Compose `SemanticsPropertyKey<T>`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticsPropertyKey<T: 'static> {
    pub name: &'static str,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: 'static> SemanticsPropertyKey<T> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _marker: std::marker::PhantomData,
        }
    }
}

// ── 标准语义属性（对齐 Compose SemanticsProperties 常用集） ────────────────
pub const CONTENT_DESCRIPTION: SemanticsPropertyKey<String> =
    SemanticsPropertyKey::new("contentDescription");
pub const TEXT: SemanticsPropertyKey<String> = SemanticsPropertyKey::new("text");
pub const TEST_TAG: SemanticsPropertyKey<String> = SemanticsPropertyKey::new("testTag");
pub const ROLE: SemanticsPropertyKey<Role> = SemanticsPropertyKey::new("role");
pub const SELECTED: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("selected");
pub const CHECKED: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("checked");
pub const DISABLED: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("disabled");
pub const PASSWORD: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("password");
pub const EDITABLE: SemanticsPropertyKey<bool> = SemanticsPropertyKey::new("editable");
pub const PROGRESS: SemanticsPropertyKey<f32> = SemanticsPropertyKey::new("progress");
pub const VALUE: SemanticsPropertyKey<String> = SemanticsPropertyKey::new("value");

/// 语义角色（对齐 Compose Role.*，映射到平台无障碍 role）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Button,
    Checkbox,
    Switch,
    RadioButton,
    Tab,
    TextField,
    Image,
    Heading,
    List,
    ListItem,
    Menu,
    MenuItem,
    ProgressBar,
    Slider,
    Container,
    Text,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Button => "button",
            Role::Checkbox => "checkbox",
            Role::Switch => "switch",
            Role::RadioButton => "radio",
            Role::Tab => "tab",
            Role::TextField => "textfield",
            Role::Image => "image",
            Role::Heading => "heading",
            Role::List => "list",
            Role::ListItem => "listitem",
            Role::Menu => "menu",
            Role::MenuItem => "menuitem",
            Role::ProgressBar => "progressbar",
            Role::Slider => "slider",
            Role::Container => "container",
            Role::Text => "text",
        }
    }
}

/// 语义动作（对齐 Compose `SemanticsActions.*`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticsAction {
    Click,
    LongClick,
    Focus,
    Scroll,
    SetText,
}

/// 一个节点的语义配置（对齐 Compose `SemanticsConfiguration`）。
#[derive(Default)]
pub struct SemanticsConfig {
    props: HashMap<&'static str, Box<dyn Any>>,
    actions: Vec<SemanticsAction>,
    /// 合并后代语义（Compose `mergeDescendants = true`）。
    pub merge_descendants: bool,
    /// 清除并覆盖后代语义（Compose `clearAndSetSemantics`）。
    pub clear_descendants: bool,
    /// 该节点不参与语义树（Compose `hideFromAccessibility` / `invisibleToUser`）。
    pub hidden: bool,
}

impl Clone for SemanticsConfig {
    fn clone(&self) -> Self {
        let props = self
            .props
            .iter()
            .map(|(k, v)| (*k, clone_any(v)))
            .collect();
        Self {
            props,
            actions: self.actions.clone(),
            merge_descendants: self.merge_descendants,
            clear_descendants: self.clear_descendants,
            hidden: self.hidden,
        }
    }
}

impl std::fmt::Debug for SemanticsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticsConfig")
            .field("props", &self.prop_names())
            .field("actions", &self.actions)
            .field("merge_descendants", &self.merge_descendants)
            .field("clear_descendants", &self.clear_descendants)
            .field("hidden", &self.hidden)
            .finish()
    }
}

impl SemanticsConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置类型化属性（对齐 `SemanticsPropertyReceiver` 的 `set`）。
    pub fn set<T: Clone + 'static>(&mut self, key: SemanticsPropertyKey<T>, value: T) -> &mut Self {
        self.props.insert(key.name, Box::new(value));
        self
    }

    /// 读取类型化属性。
    pub fn get<T: Clone + 'static>(&self, key: SemanticsPropertyKey<T>) -> Option<T> {
        self.props.get(key.name)?.downcast_ref::<T>().cloned()
    }

    pub fn has(&self, name: &str) -> bool {
        self.props.contains_key(name)
    }

    pub fn add_action(&mut self, action: SemanticsAction) -> &mut Self {
        if !self.actions.contains(&action) {
            self.actions.push(action);
        }
        self
    }

    pub fn actions(&self) -> &[SemanticsAction] {
        &self.actions
    }

    /// 合并另一个配置的属性（子 → 父，父已有属性优先，对齐 Compose merge）。
    #[allow(dead_code)]
    pub(crate) fn merge_from(&mut self, other: &SemanticsConfig) {
        for (k, v) in &other.props {
            self.props.entry(k).or_insert_with(|| clone_any(v));
        }
        for a in &other.actions {
            self.add_action(*a);
        }
    }

    pub(crate) fn prop_names(&self) -> Vec<&'static str> {
        self.props.keys().copied().collect()
    }

    pub(crate) fn prop_debug(&self, name: &str) -> Option<String> {
        match name {
            "contentDescription" | "text" | "testTag" | "value" => self
                .get(CONTENT_DESCRIPTION)
                .or_else(|| self.get(TEXT))
                .or_else(|| self.get(TEST_TAG))
                .or_else(|| self.get(VALUE)),
            "role" => self.get(ROLE).map(|r| r.as_str().to_string()),
            "selected" => self.get(SELECTED).map(|v| v.to_string()),
            "checked" => self.get(CHECKED).map(|v| v.to_string()),
            "disabled" => self.get(DISABLED).map(|v| v.to_string()),
            "progress" => self.get(PROGRESS).map(|v| v.to_string()),
            _ => None,
        }
    }
}

/// `Box<dyn Any>` 不可 Clone；语义属性值均为 `Clone + 'static`，借策略按名克隆。
fn clone_any(v: &Box<dyn Any>) -> Box<dyn Any> {
    if let Some(x) = v.downcast_ref::<String>() {
        Box::new(x.clone())
    } else if let Some(x) = v.downcast_ref::<bool>() {
        Box::new(*x)
    } else if let Some(x) = v.downcast_ref::<f32>() {
        Box::new(*x)
    } else if let Some(x) = v.downcast_ref::<Role>() {
        Box::new(*x)
    } else if let Some(x) = v.downcast_ref::<u64>() {
        Box::new(*x)
    } else if let Some(x) = v.downcast_ref::<usize>() {
        Box::new(*x)
    } else {
        // 未知类型：不合并（保持父属性优先语义）。
        Box::new(0u8)
    }
}

/// 语义节点句柄（arena 索引）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticsNodeId(pub u32);

/// 语义边界（屏幕坐标）。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SemanticsBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SemanticsBounds {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x
            && px <= self.x + self.width
            && py >= self.y
            && py <= self.y + self.height
    }
}

/// 一棵语义树中的一个节点。
#[derive(Clone, Debug)]
pub struct SemanticsNode {
    pub id: SemanticsNodeId,
    /// 对应的视觉节点（用于 perform 动作与焦点）。
    pub source: Option<NodeId>,
    pub config: SemanticsConfig,
    pub bounds: SemanticsBounds,
    pub children: Vec<SemanticsNodeId>,
    pub parent: Option<SemanticsNodeId>,
}

impl SemanticsNode {
    /// 该节点的可读标签（contentDescription 优先，其次 text / testTag）。
    pub fn label(&self) -> Option<String> {
        self.config
            .get(CONTENT_DESCRIPTION)
            .or_else(|| self.config.get(TEXT))
            .or_else(|| self.config.get(TEST_TAG))
    }

    /// 是否可交互（有任一动作）。
    pub fn is_actionable(&self) -> bool {
        !self.config.actions().is_empty()
    }
}

/// 构建好的语义树（arena 存储 + 根列表），供无障碍桥接 / 测试查询。
#[derive(Default)]
pub struct SemanticsTree {
    nodes: Vec<SemanticsNode>,
    roots: Vec<SemanticsNodeId>,
    focused: Option<SemanticsNodeId>,
}

impl SemanticsTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加节点（构建期使用）。
    pub(crate) fn push(&mut self, node: SemanticsNode) -> SemanticsNodeId {
        let id = node.id;
        if self.nodes.len() <= id.0 as usize {
            self.nodes.resize(id.0 as usize + 1, placeholder_node(id));
        }
        self.nodes[id.0 as usize] = node;
        id
    }

    pub(crate) fn set_roots(&mut self, roots: Vec<SemanticsNodeId>) {
        self.roots = roots;
    }

    pub fn roots(&self) -> &[SemanticsNodeId] {
        &self.roots
    }

    pub fn get(&self, id: SemanticsNodeId) -> Option<&SemanticsNode> {
        self.nodes.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.source.is_some() || !n.children.is_empty()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 按 testTag 查找（对齐 Compose `onNodeWithTestTag`）。
    pub fn find_by_test_tag(&self, tag: &str) -> Option<SemanticsNodeId> {
        self.nodes.iter().find_map(|n| {
            if n.source.is_some() && n.config.get(TEST_TAG).as_deref() == Some(tag) {
                Some(n.id)
            } else {
                None
            }
        })
    }

    /// 按 contentDescription / text 查找。
    pub fn find_by_label(&self, label: &str) -> Option<SemanticsNodeId> {
        self.nodes.iter().find_map(|n| {
            if n.source.is_some() && n.label().as_deref() == Some(label) {
                Some(n.id)
            } else {
                None
            }
        })
    }

    /// 坐标 → 最深命中语义节点（对齐 Compose `SemanticsNode.hitTest`）。
    pub fn hit_test(&self, px: f32, py: f32) -> Option<SemanticsNodeId> {
        fn dfs(
            tree: &SemanticsTree,
            id: SemanticsNodeId,
            px: f32,
            py: f32,
        ) -> Option<SemanticsNodeId> {
            let n = tree.get(id)?;
            if !n.bounds.contains(px, py) {
                return None;
            }
            for c in &n.children {
                if let Some(hit) = dfs(tree, *c, px, py) {
                    return Some(hit);
                }
            }
            Some(id)
        }
        self.roots
            .iter()
            .find_map(|r| dfs(self, *r, px, py))
    }

    /// 深度优先遍历所有节点。
    pub fn visit(&self, mut f: impl FnMut(&SemanticsNode)) {
        fn dfs(tree: &SemanticsTree, id: SemanticsNodeId, f: &mut dyn FnMut(&SemanticsNode)) {
            if let Some(n) = tree.get(id) {
                f(n);
                for c in &n.children {
                    dfs(tree, *c, f);
                }
            }
        }
        for r in &self.roots {
            dfs(self, *r, &mut f);
        }
    }

    /// 无障碍焦点管理（语义节点级，对齐 Compose `Focusable` semantics）。
    pub fn set_focus(&mut self, id: SemanticsNodeId) {
        self.focused = Some(id);
    }
    pub fn focused(&self) -> Option<SemanticsNodeId> {
        self.focused
    }
    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    /// 按焦点顺序移动（DFS 顺序，对齐无障碍线性导航）。
    pub fn focus_next(&mut self) -> Option<SemanticsNodeId> {
        let order = self.linear_order();
        let next = match self.focused {
            Some(cur) => order
                .iter()
                .position(|id| *id == cur)
                .map(|i| (i + 1) % order.len()),
            None => Some(0),
        }?;
        let id = order[next];
        self.focused = Some(id);
        Some(id)
    }

    /// 线性焦点顺序（仅可交互节点）。
    pub fn linear_order(&self) -> Vec<SemanticsNodeId> {
        let mut out = Vec::new();
        self.visit(|n| {
            if n.is_actionable() {
                out.push(n.id);
            }
        });
        out
    }

    /// 执行语义动作（对齐 Compose `performClick`）：调用源的视觉节点 handler。
    pub fn perform(
        &self,
        id: SemanticsNodeId,
        action: SemanticsAction,
    ) -> Result<(), SemanticsError> {
        let node = self.get(id).ok_or(SemanticsError::NotFound)?;
        if !node.config.actions().contains(&action) {
            return Err(SemanticsError::ActionNotAvailable);
        }
        match action {
            SemanticsAction::Click => {
                if let Some(src) = node.source {
                    crate::layout::dispatch_click(src);
                    return Ok(());
                }
                Err(SemanticsError::NoSource)
            }
            SemanticsAction::Focus => Ok(()),
            _ => Err(SemanticsError::ActionNotAvailable),
        }
    }

    /// 导出 JSON 快照（供平台无障碍桥接消费，对齐 Compose SemanticsNode 序列化）。
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"roots\":[");
        for (i, r) in self.roots.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            self.write_node_json(&mut out, *r);
        }
        out.push_str("]}");
        out
    }

    fn write_node_json(&self, out: &mut String, id: SemanticsNodeId) {
        let Some(n) = self.get(id) else { return };
        out.push_str("{\"id\":");
        out.push_str(&n.id.0.to_string());
        out.push_str(",\"bounds\":[");
        out.push_str(&n.bounds.x.to_string());
        out.push(',');
        out.push_str(&n.bounds.y.to_string());
        out.push(',');
        out.push_str(&n.bounds.width.to_string());
        out.push(',');
        out.push_str(&n.bounds.height.to_string());
        out.push_str("],\"props\":{");
        let mut first = true;
        for name in ["contentDescription", "text", "testTag", "role", "selected", "checked", "disabled", "progress", "value"] {
            if let Some(v) = n.config.prop_debug(name) {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push('"');
                out.push_str(name);
                out.push_str("\":\"");
                out.push_str(&escape_json(&v));
                out.push('"');
            }
        }
        out.push_str("},\"actions\":[");
        for (i, a) in n.config.actions().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(action_name(*a));
            out.push('"');
        }
        out.push_str("],\"children\":[");
        for (i, c) in n.children.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            self.write_node_json(out, *c);
        }
        out.push_str("]}");
    }
}

fn action_name(a: SemanticsAction) -> &'static str {
    match a {
        SemanticsAction::Click => "click",
        SemanticsAction::LongClick => "longClick",
        SemanticsAction::Focus => "focus",
        SemanticsAction::Scroll => "scroll",
        SemanticsAction::SetText => "setText",
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn placeholder_node(id: SemanticsNodeId) -> SemanticsNode {
    SemanticsNode {
        id,
        source: None,
        config: SemanticsConfig::new(),
        bounds: SemanticsBounds::default(),
        children: Vec::new(),
        parent: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticsError {
    NotFound,
    ActionNotAvailable,
    NoSource,
}

/// 从视觉节点树构建语义树（对齐 Compose `SemanticsNode` 树的生成）。
///
/// 属性来源优先级：显式 `Modifier.semantics` > 派生（clickable/scrollable/editable）> 绘制文本。
/// 合并规则：`mergeDescendants` 把后代文本并入父 TEXT；`clearAndSetSemantics` 切断子树；
/// `hidden` 节点及其子树不进入语义树。
pub fn build_tree(root: NodeId) -> SemanticsTree {
    // 先收集绘制文本（按 owner 节点归属），作为语义文本兜底来源。
    let mut texts: HashMap<u32, String> = HashMap::new();
    let (cmds, owners) = crate::layout::draw_tree_tagged(root);
    for (cmd, owner) in cmds.iter().zip(owners.iter()) {
        if let crate::layout::DrawCmd::Text { text, .. } = cmd {
            texts.entry(*owner).or_default().push_str(text);
        }
    }

    let mut tree = SemanticsTree::new();
    let root_id = build_node(root, &texts, &mut tree, None);
    tree.set_roots(root_id.into_iter().collect());
    // 写入线程局部，供 `with_tree` / 平台桥接读取（对齐 Compose SemanticsOwner 持有当前树）。
    TREE.with(|t| *t.borrow_mut() = tree);
    with_tree(|t| clone_tree(t))
}

/// 克隆语义树（供 build_tree 返回；SemanticsTree 本身含 Box<dyn Any> 需逐节点克隆）。
fn clone_tree(src: &SemanticsTree) -> SemanticsTree {
    let mut dst = SemanticsTree::new();
    for n in &src.nodes {
        if n.source.is_some() || !n.children.is_empty() {
            dst.push(n.clone());
        }
    }
    dst.set_roots(src.roots().to_vec());
    dst.focused = src.focused;
    dst
}

/// 递归构建一个语义节点，返回其 id（None 表示该节点被隐藏）。
fn build_node(
    id: NodeId,
    texts: &HashMap<u32, String>,
    tree: &mut SemanticsTree,
    parent: Option<SemanticsNodeId>,
) -> Option<SemanticsNodeId> {
    let sid = SemanticsNodeId(tree.nodes.len() as u32);
    // 先占槽（递归 children 时 id 稳定）。
    tree.push(placeholder_node(sid));

    // 读视觉节点字段（组合期已完成布局）。
    let (mod_elems, has_click, has_press, has_scroll, has_text_value, children_ids) =
        crate::runtime::with_rt_pub(|rt| {
            let n = &rt.nodes[id.index()];
            (
                n.modifier.elems.clone(),
                n.on_click.is_some(),
                n.on_press.is_some(),
                n.scroll.is_some(),
                n.text_value.is_some() || n.char_handler.is_some(),
                n.children.clone(),
            )
        });
    // 绝对边界单独取（node_absolute_bounds 内部也借 RUNTIME，不能嵌套）。
    let bounds = crate::layout::node_absolute_bounds(id);

    let mut config = SemanticsConfig::new();
    let mut hidden = false;
    let mut clear = false;
    let mut merge = false;
    for e in &mod_elems {
        match e {
            crate::layout::ModElem::Semantics(c) => {
                hidden |= c.hidden;
                clear |= c.clear_descendants;
                merge |= c.merge_descendants;
                // 显式属性覆盖派生值（先存起来，后面覆盖）。
                for (k, v) in &c.props {
                    config.props.insert(k, clone_any(v));
                }
                for a in c.actions() {
                    config.add_action(*a);
                }
            }
            crate::layout::ModElem::Scroll { .. } => {
                // 滚动标记（实际 ScrollInfo 在节点字段，上面已派生 has_scroll）。
            }
            _ => {}
        }
    }
    if hidden {
        return None;
    }

    // 派生语义（显式属性优先：已设置的不再覆盖）。
    if has_click && !config.has("role") {
        config.set(ROLE, Role::Button);
    }
    if has_click {
        config.add_action(SemanticsAction::Click);
    }
    if has_press {
        config.add_action(SemanticsAction::LongClick);
    }
    if has_scroll {
        config.add_action(SemanticsAction::Scroll);
    }
    if has_text_value && !config.has("role") {
        config.set(ROLE, Role::TextField);
        config.set(EDITABLE, true);
        config.add_action(SemanticsAction::SetText);
    }
    // 绘制文本兜底（显式 contentDescription/text 优先）。
    if !config.has("contentDescription") && !config.has("text") {
        if let Some(t) = texts.get(&(id.index() as u32)) {
            if !t.is_empty() {
                config.set(TEXT, t.clone());
            }
        }
    }

    // 子节点递归（clearAndSetSemantics 切断子树语义）。
    let mut children = Vec::new();
    if !clear {
        for c in &children_ids {
            if let Some(cid) = build_node(*c, texts, tree, Some(sid)) {
                children.push(cid);
            }
        }
    }

    // mergeDescendants：把后代文本合并进父 TEXT（Compose 语义合并规则）。
    if merge {
        let mut merged = config.get(TEXT).unwrap_or_default();
        for c in &children {
            if let Some(child) = tree.get(*c) {
                if let Some(t) = child.config.get(TEXT) {
                    if !merged.is_empty() {
                        merged.push(' ');
                    }
                    merged.push_str(&t);
                }
            }
        }
        if !merged.is_empty() {
            config.set(TEXT, merged);
        }
        // 合并后后代不再单独出现在语义树（对齐 Compose merge）：清掉其 source 与子树。
        for c in &children {
            if let Some(child) = tree.nodes.get_mut(c.0 as usize) {
                child.source = None;
                child.children.clear();
            }
        }
        tree.nodes[sid.0 as usize].children.clear();
        children.clear();
    }

    // 写回节点。
    let node = SemanticsNode {
        id: sid,
        source: Some(id),
        config,
        bounds,
        children,
        parent,
    };
    tree.push(node);
    Some(sid)
}

/// 当前语义树（线程局部，构建后供平台桥接读取）。
thread_local! {
    static TREE: RefCell<SemanticsTree> = RefCell::new(SemanticsTree::new());
}

pub fn with_tree<R>(f: impl FnOnce(&SemanticsTree) -> R) -> R {
    TREE.with(|t| f(&t.borrow()))
}

pub fn with_tree_mut<R>(f: impl FnOnce(&mut SemanticsTree) -> R) -> R {
    TREE.with(|t| f(&mut t.borrow_mut()))
}
