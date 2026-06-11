# 2026-10-30

## compose-rs 初始化（Jetpack Compose 架构 GUI 库）
- 用户明确：基础库 winit 0.30 + valo 0.18 + wgpu 30（valo 非 vello，类 Impeller 引擎；实际解析为 rsproxy 的 valo-dl-0.13.1）；架构严格按 Jetpack Compose；目标全平台（第一阶段 desktop + 代表性控件）。
- 硬约束：零 Rc/Arc/unsafe → 节点/状态/作用域/任务全 arena 化（u32 句柄），回调用 take-put 防重入。
- 产出：crates/compose（runtime 槽位重组/remember/derivedStateOf/CompositionLocal/effects/动画 + layout 三阶段 + material 组件）+ crates/compose-desktop（valo 渲染管线）+ counter 示例。
- 测试 5 项全绿：定向重组、remember 身份、派生状态、命中分发、绘制命令管线。
- 关键坑：rustc 1.99 match 绑定语义（顶层 Copy 字段按值、嵌套 Some 按引用）；RefCell 双借用的四处修复（remember init、observer、click handler、disposal queue 全部改借用外执行）。
- 已知限制记录在 README.md（单 Composition、物理像素坐标、TextField 精简版等）。
- counter 改为 gallery：重写 `examples/gallery.rs`（story 布局：顶部菜单栏 + 左 sidebar(180px, primary_container 高亮) + 右 content_area，`page.set(idx)` 定向重组切页），7 页：Text/Buttons/Selection/Input/Layout/Animation/Canvas；删除 counter.rs，Cargo.toml 移除入口。
- 验证：`cargo test --workspace` 全绿（4 语义测试 + pipeline 测试）；gallery 二进制构建成功。README 运行命令已更新为 gallery。
- 待办：menu_bar 真下拉菜单（File/View/Theme/Help 可点 + 主题切换接运行时而非 --dark）；更多控件页对齐 135 控件。
- menu_bar 真下拉菜单 + 运行时主题切换完成：重写 gallery.rs（发现磁盘上是旧版无 sidebar，一并重建 shell + 7 页）。菜单 File(Open/Save/Exit)/View(切页)/Theme(Light/Dark)/Help(About)，浮层 = scrim(全屏 clickable 关闭) + 定位 Box(offset 对齐菜单标签)，条件组合放作用域末尾保槽位稳定。主题切换：dark 状态在根作用域读取 → 根重跑 → material_theme 重提供 → 全树重跑（验证：run_scope 级联 + locals 在 emit 时先更新）；新增无头测试 theme_switch_propagates_to_subtree（5 测试全绿）。
- 核心库改进：material::text 未显式着色（默认 BLACK）自动继承主题 on_surface（暗色可读）。
- 闭包坑：move+Fn 闭包不能 move 出捕获（`let cl = colors;` 报 E0507）；ColorScheme 非 Copy，多个 move 闭包各需独立 clone；&str 参数入 'static 闭包需 &'static str/String。
- 修复运行时崩溃 "CompositionLocal not provided"（runtime.rs run_scope）：定向重组单个作用域时祖先链不在 scope_stack 上，theme_colors 查找失败 panic。修复：run_scope 从节点父链重建祖先作用域链（owner_scope）临时压栈（emit 路径已在栈上的跳过），运行后弹出。新增回归测试 targeted_recomposition_reads_composition_local（此前 sidebar item 点击后 text() 读主题即触发）。7 项测试全绿。
- 修复"菜单栏看不见"三连 bug（无头探针 dump DrawCmd 定位）：① CrossAxisAlignment::Stretch 测量未实现（死代码 && false）→ Row/Col stretch 收紧交叉轴（注意：measure_row_col 不能回读节点 policy——measure_policy take-put 期间是 Leaf，cross 需从调用方传入）；② display_list 文本原点错：valo draw_paragraph 原点=段落左上角（首行基线=原点+line.baseline），此前传 y-baseline 使字形上移一个 ascent——y=0 的菜单标签整个出窗；改传 (x, y)；③ Box/Row/Col place 未按容器自身 padding 偏移孩子（错用了孩子 padding）→ 下拉首项 "Open" 落在 (0,36)。探针验证：菜单标签 (21.7, 8.9) 居中、高亮 72×36 全高、Open(12,44)。7 测试全绿。

## Sidebar 升级（分组侧栏）
- 目标：参考成熟 sidebar 实现（三态宽度 220/56/8、分组折叠+徽标+箭头旋转、键盘 ↑↓/PageUp/Down、type-ahead 逐字搜索、悬停、左右停靠、聚焦外框）。本阶段覆盖高价值子集：分组 + 整栏收起/展开 + 分组折叠动画 + 键盘导航；悬停/逐字搜索/停靠/聚焦外框/最小化窄条列为后续。
- 新增焦点系统（核心库最小侵入）：`RuntimeData.focused_scope: Option<ScopeId>`；`set_focus/focused_scope/clear_focus`；`NavKey{Up,Down,PageUp,PageDown}`（核心库定义，不依赖 winit）；`layout::focusable(c,modifier,on_click,on_key,content)->ScopeId`（建 Box 节点挂 on_click + key_handler，返回 content scope id）；`layout::dispatch_key(key)->bool`（focused_scope 找节点、take key_handler、借用外执行、h(key)）。`emit_scope/emit_scope_into/emit_scope_impl` 改为返回 `ScopeId`（emit_scope_impl 末尾 `run_scope(scope); scope`）。
- 焦点模型：点击向上冒泡时记录最近带 `key_handler` 的祖先 `focus_sid`，消费 on_click 后 `rt.focused_scope=Some(focus_sid)`；点内部条目/折叠按钮不抢焦点 → 键盘导航始终生效。`run.rs` `WE::KeyboardInput` Pressed 分支翻译 ArrowUp/Down→NavKey、PageUp/Down→NavKey 后调 `dispatch_key`；take-put 防重入（同既有点击分发）。
- 图标系统：`material::IconName`（Folder/File/Play/Settings/Mail/Grid/Type/Brush/Chart/Layout/Star/ChevronRight/ChevronDown/Menu/Sun/Moon）+ `draw_icon(b,name,x,y,s,color)`（纯 DrawBuilder 基元矢量绘制；Moon 用弯月多边形）+ `draw_poly` + `icon(c,name,size,color,modifier)`。`ColorScheme` 改 `#[derive(Clone,Copy,Debug)]`（修复非 Copy 被多 move 闭包捕获 E0507）。
- gallery 重写：`PAGES`/`PAGE_ICONS`/`GROUPS`(Basics[0..3]/Layout[4]/Motion[5]/Graphics[6])；`SIDEBAR_W=200/SIDEBAR_W_RAIL=56/GROUP_H=28/ITEM_H=32`；`sidebar_header`(clickable 切整栏、ChevronRight/Menu + 展开时 "Gallery")；`sidebar_group`(frac 折叠动画 + 箭头旋转 + 数量徽标 + 条目块 box_(.height(frac*est_h).clip(0)))；`sidebar_item`(选中 primary_container 高亮、展开 CenterStart / 收起 Center)；`sidebar_key_nav`(按 GROUPS+折叠态算可见条目，↑↓±1 / PageUp/Down±5 移动 page)。`width = SIDEBAR_W_RAIL + t*(SIDEBAR_W-SIDEBAR_W_RAIL)` 由 `animate_float_as_state` 驱动，`expanded=t>0.5`。
- 闭包坑补：嵌套 `move |c|` 不能把 Vec 所有权 move 给内层 `Fn`（E0507）→ 外层 `let members2 = members.clone();`，内层 `for &m in &members2`（借引用，Vec 不消费）。`'static` 切片 `members` 入长期闭包需 `to_vec()` 拥有化。
- 验证：`cargo build --example gallery -p compose-desktop` 通过；`cargo test --workspace` 7 项全绿（无回归）。README 已补 sidebar 分组/折叠/键盘导航/图标系统说明。
- 后续待办（差距）：悬停高亮、逐字搜索(type-ahead)、左右停靠、聚焦外框、最小化窄条三态(8px)。

## Sidebar 功能完善 + gallery 独立 examples crate
- 核心库新增交互基础设施（compose）：`Node.hover_handler/char_handler`、`hoverable`、`focusable_full`（可选 char 处理器）、`dispatch_hover_at/dispatch_hover_left/dispatch_char`；`RuntimeData.hovered_node` 跟踪悬停节点。`set_focus/clear_focus` 改为「变化才调度 + 失效新旧作用域」→ 聚焦外框等焦点读者可重组。
- gallery 补齐 Sidebar 五项：悬停高亮（on_surface 8% 叠加）、逐字搜索（1s 超时 type-ahead，字符路由 TextField 优先 → dispatch_char）、三态宽度 200/56/8（View 菜单 Minimize + 悬停 reveal bridge + 点窄条恢复）、左右停靠（Box 覆盖布局，组合顺序固定保槽位稳定）、聚焦外框（canvas overlay 描边 + SB_SCOPE thread_local 对比 focused_scope）。
- **布局教训**：Row 双子树条件换序会互串 remember 槽位（downcast 失败重置状态）；改用 Box 覆盖（内容铺满 + sidebar 覆盖其上切 Alignment），组合顺序固定零风险。
- **gallery 移出 compose-desktop** → 顶层 `examples/` 独立 crate（`cargo run --example gallery -p examples`）；compose-desktop 回归纯薄壳。
- compose-material 补 `input_focused()`（字符路由判据）。全量测试 7+1 全绿。

## 修复 RefCell 双借 panic（TextField 输入字符崩溃）
- 现象：`gallery` 运行后向 TextField 输入字符即 panic `RefCell already borrowed`（runtime.rs:142 `with_rt` 的 `borrow_mut`）。
- 根因：`input_char`/`input_backspace`（compose-material/src/widgets.rs）在 `with_rt_pub(|rt| {...})` 闭包内直接调用 `st.update(...)`，而 `MutableState::update` 内部再次 `with_rt`→`borrow_mut`，形成同一线程的重复可变借（RefCell 拒绝嵌套可变借）。栈：Kernel::char → input_char → with_rt_pub → update → with_rt。
- 修复：先把 `rt.nodes[id].text_value`（`Option<MutableState<String>>`，句柄 Copy）**取出（拷贝）出 runtime 借用**，关闭借用后再 `st.update(...)`。两函数均改为此模式；并加注释警示。
- 同文件 slider 的 `on_click` 内也有嵌套 `with_rt_pub`（取 width），但 `dispatch_click_at` 先 `.take()` handler 再在运行时借用外执行 `f()`，故非同时嵌套，安全。
- 验证：`cargo check` 四 crate 通过；`cargo test -p compose -p compose-material` 7 项全绿（无回归）。

## 渲染层收口 + compose-platform 合并（跨平台分层落地）
- 分层最终形态（`src/platform` 布局）：
  - `crates/compose`（runtime/layout/material）、`crates/compose-material`（控件）不变。
  - `crates/compose-render` = **平台层之上**共享层：`display_list`/`gpu`/`kernel`/`host`（桌面/Web 专属 winit 循环，按 `cfg` 门控）/`fonts`。渲染收口于此，不再于 `compose-platform` 内建 `rendering/`。
  - `crates/compose-platform` = 跨平台宿主粘合：按 target 条件编译 `platform/{desktop,web,android,ios,ohos,services}`（原 `compose-desktop` 已并入，workspace members 现为 compose/compose-material/compose-render/compose-platform/examples）。
- `crates/compose-render/src/fonts.rs`：把 `default_font_collection` 由「双 cfg 同名定义」合并为**单一函数 + 内部 `cfg!` 分支**（否则跨 crate 的 `pub use` 重导出在某些缓存场景下对外不可见，且 `scan_font_dir` 去 cfg 化保证所有 target 编译期可见）。桌面分支扫描 `/usr/share/fonts` 等加 `sans-serif`/`monospace` 别名；非桌面返回 `FontCollection::default()`（TODO 嵌入 CJK）。
- Android 后端（`platform/android.rs`）**完全改写**：放弃 winit 0.30 Android 后端（API 漂移：`EventLoopBuilder::with_android_app` 关联函数消失、`raw_window_handle`→`as_raw`、`ndk::NativeWindow::from_ptr` 签名变更、`#[non_exhaustive]` main_event 需兜底），改用 `android-activity` 的 `AndroidApp::poll_events` 主循环，从 `android_app.native_window()` 取 `ANativeWindow` 直接喂 `GpuRenderer::create_surface_from_native_window`。事件循环在 compose-platform 内自驱，`compose-render` 仅保留 ndk 类型依赖供 gpu.rs。`compose-platform` Android 依赖最终 = `android-activity = { version="0.6", features=["native-activity"] }`。
- `Kernel` 输入 API：`pointer_down/move/up(x,y)`、`scroll_at(x,y,dy)`、`char(ch)`、`backspace()`、`key(NavKey)`（`NavKey::{Up,Down,PageUp,PageDown}`）；`kernel.key` **返回 `bool`**（表明是否需要续帧/消费）——注意 host.rs 的键盘 match 臂需 `kernel.key(...);` 丢弃返回值，否则与 `()` 臂类型冲突报 E0308。
- 修复被缓存掩盖的 **compose-render host.rs 真错误**：`WE::KeyboardInput` 内 `kernel.key(NavKey::*)` 四个臂未丢弃 `bool` 返回值，与 `Key::Character` 的 `()` 臂类型不匹配 → E0308。此前 `cargo check` 一直用陈旧缓存「Finished」假绿，clean rebuild 后才暴露。已补齐 `;` 修复，`workspace` 全量编译通过。
- `examples/gallery.rs` 四端入口收敛：`#[cfg(not(android/wasm))] run_app(GalleryApp)`、wasm 同 `run_app`、android `#[no_mangle] fn android_main(app: compose_platform::AndroidApp) → compose_platform::android_entry(|| Box::new(GalleryApp{..}), app)`。
- iOS/OHOS（`platform/ios.rs` / `platform/ohos.rs`）沿用宿主驱动 C ABI 模型（`compose_ios_*` / `compose_ohos_*` 符号），修正 `valo::FontCollection::default()` → `default_font_collection()`（valo 非 compose-platform 直接依赖）。因无 iOS SDK / OHOS target，本环境用临时 `verify-ios/verify-ohos` feature 绕过 target cfg 在宿主做类型检查，确认 API 一致后已回退临时改动。
- 验证状态：① `cargo check -p compose-render` 通过；② `cargo check -p compose-platform`（host）通过；③ `cargo check -p compose-platform --target aarch64-linux-android` 通过（修复 `E0004` 非穷尽 `PollEvent` 缺 `_ => {}` 兜底臂）；④ `cargo check --workspace` 通过；⑤ `cargo check --example gallery` 通过。Web(wasm32) 因 crates-io 受限无法拉 `wasm-bindgen` 未实跑；iOS/OHOS 受 SDK/target 缺项限制未实编译。
- 后续待办：services（camera/bluetooth）仍 Stub；wasm/iOS/OHOS 需各自 SDK/网络实跑；全量 `cargo test --workspace` 与真机 GUI 验证。

## 修复 sidebar 显示/点击失效 + 下拉菜单不对齐（三处引擎级 bug）
- 用户截图确认三症状：①侧栏条目图标与文字重叠；②条目点击无法切页；③下拉浮层永远贴最左不对齐菜单标签。
- 根因与修复（均在 `compose/src/layout.rs`）：
  1. **`Modifier.offset` 完全不参与落位**（测量/落位都没读 Offset elem）→ 落位 `place_node` 读 `modifier.offset_of()` 平移（含子树；不影响测量，对齐 Compose）。下拉对齐即由此修复（gallery 的 offset(idx*MENU_W+4, 2) 生效）。
  2. **铺满父节点的装饰性 canvas（聚焦外框）先命中**（Box 命中按 children.rev()，外框是最后组合的孩子且 fill_max_size）→ 点击全被 focusable 的空 on_click 吃掉。新增 `ModElem::NoHit` + `Modifier::no_hit()`：`hit_node` 直接跳过该子树。gallery 聚焦外框 canvas 加 `.no_hit()`。
  3. **点击焦点容器内部子项会丢焦点**：`dispatch_click_at` 的焦点查找与 on_click 查找混在同一条走一半的循环里——on_click 在内层被消费后就停了，永远走不到上层的 focusable key_handler → clear_focus()。修复：两条独立链各走到底（先整链找最近 key_handler 祖先，再找最近 on_click）。
- gallery 修复：`sidebar_item` 条目内容由 `box_`（Box 子节点重叠语义，图标+间隙+文字全叠在一处）改为 `row`（Arrangement Start/Center 按 expanded 切换，CrossAxisAlignment::Center，padding_hv(12,0)）。
- 新增回归测试 `crates/compose/tests/sidebar_layout.rs`（7 项全绿）：offset 落位/不影响测量、no_hit 放行点击 + 无 no_hit 对照组确实拦截、点 focusable 内部子项保焦点、Row 依次排布 vs Box 重叠语义。
- 端到端验证：xdotool 不可用；改为实跑 gallery + gnome-screenshot 截屏确认侧栏渲染正常（图标+文字并排、分组/徽标/高亮全对）。桌面截图验证 GUI 时注意：`pgrep -f <pattern>` 会匹配到自身 shell 命令行造成"进程仍在运行"假象，用 `ps aux | grep "pattern[y]"`。
- 教训：**布局引擎新增 ModElem 时必须同时接线测量（apply_measure_mods）、落位（place_node）、命中（hit_node）三处**，漏一处即成静默 bug（offset 漏了两处）。

## 动画性能优化：静止即停驱 + 增量脏区绘制（空闲 CPU 45% → 0%）
- 两条用户请求：①按 Jetpack Compose 动画算法消除「动不动就 CPU 45%」；②做增量脏区绘制（去掉每帧无条件全量开销）。
- ①`crates/compose/src/animation.rs`：`AnimFloat`/`AnimColor` 增加 `duration_frames` + `running` 字段与 `tick()`；帧驱动改为 `if st.peek_mut(|s| s.running) { schedule_scope(sid) }` —— 对齐 Compose `AnimationState`「只在 isRunning 时留在 Recomposer 动画集合」；删除旧 `sample/sample_values/sample_color` 自由函数；`remember` 的 init 返回 `MutableState` 句柄（Copy）供驱动长期捕获。`remember_infinite_transition` 保持每帧调度（无限动画本就不该停驱）。公开 API 签名不变。新增 `pub fn animations_active()` 语义确认：thread_local 标志**读取即清除**，宿主每帧读一次决定续帧——无头测试必须逐帧读（`frames_probe`），否则上一帧标志误报。
- ②`crates/compose/src/layout.rs`：`perform_layout` 增加「约束未变 + 根不脏 ⇒ 直接返回既有尺寸」早返回（跳过 O(N) 测量/落位/update_scroll_content）；新增 `thread_local! { SCENE_CHANGED }`，`perform_layout`/`draw_tree_tagged` 首行复位，`draw_node` 真正改变输出时置位；导出 `scene_changed()` + `scene_is_clean(root)`（根不脏且有缓存 ⇔ 全树无脏区，依赖 mark_dirty_path 的聚合不变式）。`crates/compose-render/src/kernel.rs`：`last_cmds: Option<Vec<DrawCmd>>`（每帧 O(M) 深比较）换成 `has_rendered: bool` + 三层短路（快路径①`scene_is_clean` 跳整树遍历；快路径②`!scene_changed()` 跳 DisplayList 重建与 GPU 上传；否则才重建）；`partition_static_dynamic` 移入 retained 分支。
- **新约定（务必遵守）**：时间相关绘制（光标闪烁、进度条）**禁止**直接 `rt.nodes[id].draw_invalid = true`——只标自身不标祖先会被根缓存冻结；必须走 `compose::layout::invalidate_draw_node(id)`（= 子树失效 `mark_draw_invalid_subtree` + 祖先失效 `mark_draw_invalid_path`，不动 `dirty`、不强制重布局）。帧驱动**禁止捕获 NodeId 字面值**：`remember` 只注册一次，重组后节点是新 arena 下标，旧下标失效即静默停摆；用 `compose_material::widgets::input::keep_repainted_while(id, active)`（内部把 id 存进 `Rc<Cell<Option<NodeId>>>`，每次重组 `slot.set(Some(id))` 刷新）。调用点：BareTextField/TextField/PasswordField/OtpInput/fade_mask/RichTextEditor。
- 由此挖出两处既有真 bug：①控件直置 draw_invalid 不传播 → 增量快路径把光标/进度冻在旧画面；②帧驱动持陈旧 NodeId → TextField/RichTextEditor 焦点光标在任意重组后永久冻结（此前被「每帧全量重建」的浪费掩盖，优化后才暴露）。OTP 边框画在各格子**子节点**上，故失效必须覆盖子树。
- 测试：新增 `crates/compose/tests/dirty_region.rs`（稳定画面不重跑绘制闭包 / 布局早返回回报正确尺寸 / 重组脏区传到根 / 单节点失效传到根）+ `crates/compose/tests/animation_idle.rs`（静止 30 帧零重组、到位后收敛且停止请求续帧、新目标重新起驱）。`settle()` 需两轮 layout+draw：首帧 `last_geo=None` ⇒ place_node 把全树判几何变化重标脏。`input_caret_click`/`otp_advance` 原为轮询 `draw_tree` 但不泵帧的 flaky，已在循环内补 `advance_frame(1/60)` + `Recomposer::recompose_pending()` + `perform_layout`（与生产帧循环同路）。
- 验证：`cargo check --workspace` 通过；`cargo test --workspace --tests` **93 用例全绿**（注意：`cargo test --workspace` 会卡在 compose-platform 的 rustdoc doctest，本机日常用 `--tests` 跳过）。实测 `target/debug/examples/story`：空闲 30s `/proc/<pid>/stat` ticks 增量 = 0（`/proc/<pid>/schedstat` run_time 亦 0.00ms/2s），对比改造前约 45%；窗口 resize（真实全量脏区）后 CPU 唤醒约 20ms 随即再次归零，证明「按需干活、无事休眠」。
- 环境坑：①CLion 会把会话内的源码编辑**自动提交进 HEAD**，导致 git stash/checkout 无法取得「优化前基线」，对照实验前必须先 `git log` 确认；②GUI 交互注入在本机不可用——无 xdotool，`Xlib.protocol.event.ButtonPress` + XSendEvent 合成点击 winit/XCB 不处理（run_ms 恒 0），XTEST 真指针点击又因 story 窗口位于 (-70,-184) 且被全屏窗口遮挡而可能误点其它应用，故交互验证退化为「无头断言 + 进程 CPU/schedstat 实测」。

# 2026-11-02

## 动画子系统对齐 `androidx.compose.animation.core`（墙钟帧时钟 + Spec 体系 + 中央注册表）
- 上一轮做到「静止即停驱 + 增量脏区」，本轮补当时留下的结构性缺口：①计时按「帧数 ÷ 假定 60fps」→ 120Hz 上 0.3s 动效只跑 0.15s；②缺 `SpringSpec`/`KeyframesSpec`/`SnapSpec`/`Animatable`；③每个 `animateXAsState` 各挂一条**永久** `add_frame_driver` 且从不注销（每帧成本 O(创建过的动画数) + 驱动泄漏）；④material 动画控件用 `ensure_frames()` 整作用域每帧重组。
- `crates/compose/src/animation.rs` 重写（~860 行）：`AnimVector=[f32;4]` + `AnimatableValue`（`T ⇄ 向量`，一套仿真同时服务 f32/Dp/Offset/Color = Compose `VectorConverter`）；`AnimSpec::{Tween,Spring,Snap,Keyframes,Infinite}` + `impl Into<AnimSpec>` 让所有旧 `Tween::new(0.22)` 调用点零改动兼容；`Spring::simulate` 是阻尼振子**闭式解**（欠/临界/过阻尼三支，含 `finite_exp` 溢出兜底），`StiffnessLow/Medium/High = 1500/4000/10000`；`Animatable<T>`（`remember_animatable`/`animate_to`/`snap_to`/`velocity`，中断续接**保留速度**）；`animate_progress_on(key, spec)` = key 变化即从 0 重播的入场进度（对齐 `AnimatedVisibility`/`AnimatedContent`）。
- **中央注册表**（对齐 `Recomposer` 动画集合）：`thread_local RUNNING: Vec<Reg{id,scope,last_frame}>`，`runtime::advance_frame` 里 `tick_animations()` 一次遍历完成「剪枝 + 定向调度」；到位 `unregister` 自删；`STALE_FRAMES=2` 处理「组合不再采样」的孤儿（控件被条件移除时动画悬挂）。新增 `animations_active()` 判据 = 注册表非空 **或** `FRAME_REQUEST`（读取即清除，兼容控件自挂驱动）。
- 关键设计取舍（勿在后续会话中误改）：
  - **剪枝为什么用帧计数器而不是作用域 generation**：先前试过「remember 槽位记 scope_run_generation，不等即剪」——点击事件里（组合外）调 `animate_to` 时起驱后作用域**先重组再 tick**，generation 永远滞后一拍 → 新动画被误剪。帧号方案最坏只浪费一次重组。
  - **`visibility_threshold` 取相对语义**：Compose 的阈值是绝对值（`Float.VisibilityThreshold=1.5f`）因为其动画值自带单位；本库 `f32` 既表示 0..1 alpha 又表示像素，绝对阈值无法通用 → 实际阈值 = `threshold·|target−from|` 再取 `max(ABS_EPSILON=1e-4)`。
  - **`FRAME_NOW` 线程局部快照**：`advance()` 跑在 `MutableState::peek_mut` 的 RUNTIME 可变借用内，再 `with_rt` 读帧号必然 RefCell 重入；`tick_animations` 先把帧号写进 `FRAME_NOW`，`touch/unregister` 只读它。
  - **`touch` 顺带刷新归属 ScopeId + `animate_value_as_state`/`value()` 每次采样写回 `a.scope = current_scope_id()`**：`remember` 槽位跨作用域换代复用时，注册表若持旧句柄会调度死作用域 → 动画静默停摆（与既有「驱动禁止捕获字面 NodeId」同坑）。
- 单调时钟落在 runtime：`RuntimeData.frame_time_nanos`（`advance_frame(dt)` 累加 `dt·1e9`），`frame_time_nanos()`/`frame_delta()`/`scope_alive()` 公开；`advance_frame` 顺序 = 置帧号 → `recompose_pending` → `tick_animations` → `run_frame_drivers` → `recompose_pending` → `drain_tasks`。另加 `frame_driver_count()` 供测试断言「驱动只注册一次且卸载即注销」。
- material 侧迁移（`widgets/animation.rs` 等）：**删除 `ensure_frames`**（永久 `schedule_scope(整页)`）与 `progress_since(frame_count)`；`SharedAxis`/`FadeScale` → `animate_progress_on`；`fade_mask` → `clock_nanos` + `progress_since_nanos` 且 `disposable_effect(0).on_dispose(remove_frame_driver)`，进度只在真的变化时 `invalidate_draw_node`（不再无条件标脏）；`Pulse` → 纯 `remember_infinite_transition`（注册表自驱）；`PullToRefresh` → **删掉重复的第一条驱动**（它调度父作用域，下拉期间整棵 PullToRefresh 含滚动容器每帧重布局），保留指示器作用域一条，收改用核心 `Spring::critically_damped(STIFFNESS_MEDIUM).simulate(...)`（目标变化即从当前显示位重播种、保留速度），旋转相位用 `loop_phase(clock_nanos(), 0, 0.9, Reverse)`，`spinner_sid` 存 `Rc<Cell<ScopeId>>` 每次组合刷新。
- **顺带挖出的真 bug（对齐「静止即停驱」之后的新约定违反者）**：`PageView(auto_play_ms)` 的驱动只 `schedule_scope`、**从不调 `request_anim_frames()`** → 宿主空闲休眠后轮播静默停摆（且按 `current_frame() % period` 计帧率、驱动永不注销）。修为墙钟累计（`clock_nanos` 比较）+ 主动请求续帧 + dispose 注销。同类泄漏在 `keep_repainted_while`（焦点光标）与 `fade_mask` 一并补上 dispose。
- 测试：新增 `crates/compose/tests/animation_specs.rs` 8 项（**30fps vs 120fps 同一累计时刻取值一致**、Snap 当帧落位、Keyframes 命中站点、Spring 到位退集且停止请求续帧、`Spring::simulate` 物理（临界不回弹 / bouncy 回弹后收敛 / t=0 导数=初速度）、`animate_progress_on` 重播、孤儿动画被剪枝、无限相位按墙钟回绕）+ `crates/compose-material/tests/animation_idle.rs` 3 项（SharedAxis 入场后 60 帧零重组、Crossfade 驱动只注册一次且子作用域销毁即注销、PullToRefresh 未越阈值收回后停止请求续帧）。`cargo check --workspace --tests` 干净（仅历史告警）；`cargo test --workspace --tests` **104 用例全绿**（上轮 93 + 本轮 11）。
- 验证坑（别再踩）：多线程 winit 进程 `/proc/<pid>/stat` 的 utime+stime **只统计主线程且不含未退出线程**，本例连主线程都读出差（ticks=0 而 `COMPOSE_TRACE_FRAMES` 明显示 60fps、busy≈95%）——判定「是否真在空转」应以 `[trace] path fast1/fast2/full` 为准：页面静止时 10s 内**完全不打印 trace 行**（不足 60 帧）即为真·空闲；story 页 0/4/10 均如此，页 2（Animation 演示，含自动循环 tween）与页 3/6 等含常驻无限动画的页面按设计连续 60fps。
- 待办：`examples/story.rs` 动画页 5 个演示仍是 app 级 `std::time::Instant` + 手写 `add_frame_driver`（可迁 `remember_infinite_progress` / `animate_float_as_state(target, Spring::bouncy())` 作展示）；`layout.rs:754/760` `fw`/`fh` 死赋值（疑似 `CrossAxisAlignment::Stretch` 潜在 bug）与 `advanced.rs` `ripple_burst` 偶发并行 flaky 仍未查。

# 2026-11-03

## 动画 API 补全（Repeatable / Decay / updateTransition）+ 定点唤醒调度（CPU）
- 对齐 `androidx.compose.animation.core` 剩余三项：①`RepeatableSpec` → `repeatable(iterations, animation, LoopMode)`（Restart / Reverse，Reverse 奇数次迭代反向，结束时停在起点）；②`ExponentialDecaySpec` → `Decay`/`exponential_decay(friction, threshold)` + `Animatable::animate_decay`（v(t)=v₀e^(−kt)，k=4·friction，位移取积分；无目标值，收敛点即当前位置，结束不跳回起点）；③`updateTransition` → `Transition<S>`（`animate_float/animate_dp/animate_color` 从同一目标态取多条动画，全部走既有注册表）。
- **定点唤醒（timed wake）**：新增 `request_anim_frame_after(delay)` / `take_wake_delay()`（线程局部，取最小值、读取即清除），区分「连续帧」与「延时一帧」。宿主 `Handler.next_wake` + `new_events(StartCause::ResumeTimeReached)` + `about_to_wait` 映射到 winit `ControlFlow::WaitUntil/Wait`。用途：光标闪烁按 530ms 相位翻转唤醒、PageView 轮播按周期唤醒——焦点输入框与自动轮播不再让应用常驻 60fps。
- 每帧分配清零：`advance` 改 `match &self.spec`（不再 `spec.clone()` 深拷贝 Keyframes 站点 Vec）；`tick_animations` 用 `mem::take` + `retain` 代替每帧克隆注册表；文本测量器从 `Kernel::frame()` 移到 `Kernel::new()`（每帧一次 Box 分配）。
- **回归修复（真实 bug）**：光标改为相位定点重绘后，OTP 焦点格（非响应式 `Rc<Cell>`，无重组）移动不再可见 → `OTP_INPUT` 登记项扩为 `(焦点索引, 格数, 行节点)`，`InputChar`/`InputBackspace`/`InputKey`/格子点击改焦点后 `invalidate_draw_node(行)`；`blink_caret_while` 补「失去焦点当帧失效一次」。教训：**任何非响应式 draw 闭包读者，改值处必须自己失效绘制**，不能依赖「每帧重绘」兜底。
- 验证：`cargo test -p compose -p compose-material --tests` **113 用例全绿**（新增 repeatable 两向 / decay 收敛 / timed-wake 一次性 / updateTransition 多值 5 项）；`cargo check --workspace --tests` 与 `cargo check -p examples --examples` 通过。共享目录上增量编译会因 UNC 锁失败（os error 87/2147024809），本机需 `CARGO_INCREMENTAL=0`。
- 待办不变：story 动画演示迁移与 `layout.rs` 死赋值。

## 动画性能二轮：draw-phase 迁移 + 每帧分配清零
- **注册表 / 驱动容器 O(N) 化**：`animation::RUNNING` 由 `Vec` 改 `HashMap<u64, Reg>`（`touch`/`unregister` 每帧每动画都发生，线性扫描是 O(N²)/帧）；`runtime::frame_drivers` 改 `HashMap<u64, FrameDriver>`，`run_frame_drivers` 用复用的 `DRIVER_IDS` 缓冲按 id 定位（不再每帧克隆整表 + 位置扫描）。`AnimData::advance` 增加「同一时间戳重复采样」短路（只续期注册表，不重跑弹簧闭式解）；`Spring::simulate_vec` 四轴共享 exp/三角系数并跳过惰性轴（标量动画只算 1 轴）；`animate_value_as_state`/`Animatable::value` 的采样合并为一次 `peek_mut` + `runtime::frame_time_and_scope()`。
- **`compose_material::widgets::animation::draw_phase(node, progress, period, mode)`**（`pub(crate)`）：纯绘制型无限相位（不确定进度 / Spinner / Skeleton / AnimatedImage 播放点）。帧驱动写 `Rc<Cell>` 后只 `invalidate_draw_node(该节点)`，**绝不 `schedule_scope`**——调用方作用域不参与每帧重组；`Some(v)`（确定态）时驱动空转、不请求帧。已迁移：`LinearProgress`/`CircularProgress`/`Skeleton`/`Spinner`/`AnimatedImage` 脉冲点。
- **PullToRefresh 指示器也改 draw-phase**：`pull` 从 `MutableState` 改 `Rc<Cell>`（每帧 `set` 不再触发观察者）；指示器作用域只在挂载 / `refreshing`/`armed` 变化时组合一次，位移、是否旋转、旋转相位由驱动写入 `ind_dh/ind_spin/ind_phase` 并只失效指示器节点（`pull_indicator_cells` 仅在值变化时失效）。刷新可持续数秒，旧实现整段期间每帧重组并重建 Canvas 节点/闭包。注意：指示器组合期仍读 `refreshing/armed` 写入初值，保证「首帧即 `refreshing=true`」时立刻可见（`compose/tests/pull_refresh.rs`）。
- **绘制遍历分配清零**：`draw_node` 的孩子表改 take-put（旧实现每帧每个节点 `children.clone()` 一次堆分配，动画期上百节点即上百次/帧）；绘制闭包改 `DrawBuilder::with_buffer(cmds)` 直接写进图层前缀缓冲（省掉临时 Vec 分配 + 逐命令复制）；`mark_draw_invalid_subtree` 对叶子走无分配快路径，栈遍历不再 clone 孩子 Vec。旋转弧/液面路径改用定长栈数组（`arc_points<const N>`），`CircularProgress` 轨道与进度弧只采样一次画两遍。
- **重组批次缓冲复用**：`Recomposer::recompose_pending` 每轮 `mem::take` 的 `pending` Vec 处理完还回 runtime（旧实现每轮丢弃，动画期间每帧重新分配撑容量）；轮内新调度的作用域接回同一块缓冲。
- 测试：`crates/compose-material/tests/animation_idle.rs` 新增 3 项——Spinner/Skeleton 相位逐帧落到绘制命令且调用方作用域零重组、确定态进度不请求续帧、PullToRefresh 刷新态旋转弧逐帧变化并在收回后彻底静默。`cargo test -p compose -p compose-material --tests` **118 用例全绿**；`cargo check --workspace --tests`、`cargo check -p examples --examples` 通过（仍用 `CARGO_INCREMENTAL=0`）。
- 待办不变：story 动画演示（app 级 `Instant` + 手写驱动）可迁到 draw-phase API；`layout.rs` `fw`/`fh` 死赋值；`advanced.rs` `ripple_burst` 偶发 flaky。

## 动画性能三轮：帧内零分配（保留模式计划缓冲 + 指纹集轻量哈希）
- **`FrameDraw` 跨帧复用**：新增 `layout::draw_tree_retained_into(root, state, &mut f, force)` + `FrameDraw::clear()`（清内容、留容量）；`Kernel` 新增 `frame_draw` 字段逐帧复用（首帧/快路径②/完整路径三条出口都放回），`RetainedState` 新增私有 `is_static`/`blks` 临时缓冲。动画期每帧原来要新建 `segments`/`plan`/`dynamic_cmds` 三个 Vec + `vec![false; n]` + `Vec<Blk>`；现在只剩现场块命令的自然写入。旧入口 `draw_tree_retained` 保留为薄包装（测试语义不变）。
- **`next_runs`/`last_runs` 双缓冲交换**：本帧指纹集合与上一帧 `swap` 后 `clear` 复用（旧实现 `std::mem::take` 后整表丢弃 → 每帧重新扩容 + 重哈希几百条段）。`RetainedState::last_runs`/`sig` 改为私有字段，内部容器不外露成 API。
- **指纹集换轻量哈希**：`FingerprintSet = HashSet<u64, FingerprintHashBuilder>`，splitmix64 收尾。键是内容哈希 / 自增 id（分布已均匀），默认 SipHash13 的抗碰撞强度无用；保留模式每帧按段一次 `contains` + 一次 `insert` 是纯算术。
- **驱动快照带归属**：`DRIVER_IDS: Vec<(u64, Option<ScopeId>)>`。`owner` 注册后不变，快照即权威 → 每驱动每帧少一次 `HashMap::get`。
- **`run_scope` 祖先链缓冲复用**：`SCOPE_CHAIN` thread-local 承接 leaf→root 链，压栈时 `.iter().rev()`（旧实现每次定向重组新建 Vec + reverse）。同时删掉 `content` 上多余的 `mut`（`ScopeData::content` 是 `Box<dyn Fn()>`，编译器告警）。
- **`Kernel::build_chunked_frame`** 的「本帧用到的静态块键」`HashSet` 改字段复用（`used_chunks`），不再每帧新建。
- 测试：`retained_layers.rs` 新增 2 项——`reused_frame_draw_equals_flat_stream_across_steps`（同一 `FrameDraw` 连续复用 6 帧，跨重组/换 id，每帧拼回命令流必须与平铺遍历一致，且稳定帧现场命令为空）、`frame_draw_clear_resets_previous_plan`。`cargo test -p compose -p compose-material --tests` **120 用例全绿**；`cargo check --workspace --tests`、`cargo check -p examples --examples` 通过。
- 考虑过但**回退**：`invalidate_draw_node` 去掉「祖先链置脏」（理论上逐节点图层不嵌子树命令，父层回放仍会递归孩子）。改动理论成立，但 `frame_cost` 基准在本 VM 上噪声 2–3 倍，无法证明收益 → 保持既有行为，避免无测量的行为变更。
- 环境备忘：新 shell 里 `cargo` 不在 PATH，需 `export PATH="/c/Users/Administrator/.rustup/toolchains/stable-x86_64-pc-windows-gnu/bin:$PATH"`；compose-render **测试二进制**链接还缺 `dlltool.exe`，加 `/d/local/msys64/mingw64/bin` 即可（MSVC toolchain 不可用：无 `link.exe`）。`frame_cost` 用 `cargo test -p compose-render --test frame_cost -- --ignored --nocapture`，单次结果不可比，需多次取中位数。
- 待办不变：story 动画演示迁移、`layout.rs` `fw`/`fh` 死赋值、`ripple_burst` flaky；下一批可做：按节点缓存子树段区间以跳过未变子树的规划遍历（Compose layer 语义的完整形态）。

## 2026-11-03（续）全控件对齐 Material 3 + 文本折行测量

- **令牌化收口**：`widgets/{feedback,data,input,animation,glass,advanced,grid_sheet,chart,basic,mod}.rs` 里所有裸数字（字号/圆角/间距/控件尺寸）改从 `crate::tokens` 取；`tokens.rs` 补 `TABLE_*`/`PROGRESS_TRACK`/`SPINNER_STROKE`/`EMPTY_STATE_ICON`/`PLACEHOLDER_ICON`/`SNACKBAR_*`/`COMMAND_PALETTE_W`/`DIALOG_*`/`CALENDAR_CELL`/`CODE_*`/`CHART_LEGEND_*`/`FIELD_LABEL_TOP`/`FIELD_VALUE_TOP`/`FIELD_VALUE_H`。新增约定：**控件文件里不该再出现 `TextStyle::new()`**（除 `field_style`/`run_style` 这类本地包装），排版一律走 `tokens::角色(Color)`。
- **文本折行（核心引擎改动，此前卡片/列表正文横向溢出被裁）**：
  - `DrawBuilder` 增 `probe_max_w`（`with_buffer`/`probe` 默认 `f32::INFINITY`）；`draw_text` 的 probe 分支取 `mw = if max_w>0 { max_w.min(probe_max_w) } else { probe_max_w }` 再 `measure_text_at(text, style, mw)`。
  - `leaf_intrinsic(rt, id, avail_w)` 新增第三参，由 `Policy::Leaf` 分支传 `constraints.max_width`；probe 仍按 `0,0,0,0` 调闭包（其它 Leaf 闭包行为不变）。
  - `TextMeasurer` 加 **默认方法** `measure_at(text, style, max_w)`（退回 `measure`），故第三方实现不破；`measure_text_at` 为 `pub(crate)`，无度量器时走新公开的 `heuristic_text_at`（超宽按行叠高）。删掉死代码 `measure_text_probe`/`layout_probe_text`。
  - `Text` 控件绘制侧同步：闭包改用收到的 `w` 作 `max_w`（原来传 `f32::INFINITY` ⇒ 永不折行）。**测量侧与绘制侧必须给同一个宽度**，否则量到单行、画成多行。
  - valo 侧：`TextContext::measure_at`（`get_paragraph(text, style, layout_width(max_w))` → `(longest_line, height)`）；`ValoTextMeasurer::measure_at` 覆写；**段落缓存 key 必须含折行宽**（`(max_w*2.0) as i64`）——同一句话在标题/正文里宽度不同，旧 key 不含宽度会命中错段落；`layout_width()` 把 `INFINITY`/`<=0` 归一到 `f32::MAX`，测量与绘制共用。
  - 回归测试 `crates/compose/tests/text_wrap.rs`（2 项）：200px 盒内 100 字符必须报 >2 行高且不超宽；1000px 盒内保持单行。
- **M3 输入框标签上浮**：`TextField`/`OutlinedTextField`/`PasswordField` 有内容时标签浮到顶行（`label_small` @ `FIELD_LABEL_TOP`），正文落在 `FIELD_VALUE_TOP..+FIELD_VALUE_H`；空且未聚焦时标签居中当占位。`draw_field` 加 `float_label: bool`（`BareTextField` 传 `false`）。**`PasswordField` 签名改为 `(value, label, modifier)`**（破坏性，调用点 2 处已同步）。
- **视觉修正**：`NavigationBar` 删掉矩形 `shadow`（M3 用色调高度，投影会在整条栏四周糊出灰块）；`Slider` 未激活轨道 `surface_container → surface_variant`（前者在 surface 上几乎不可见）；`SearchBar` 内边距改对称 `padding_ltrb(SPACE_MD,0,SPACE_MD,0)`（原右侧 `SPACE_XS` 让清空按钮贴到窗边）；`CircularProgress` 轨道与进度弧分开采样（旧实现共用一段部分弧 ⇒ 灰色轨道从不显示）；`caret_index_at` 改用 `field_style`（原按 14px 量、字段按 16px 画 ⇒ 点击定位偏左）。
- **测试期望必须跟着令牌走，别写死像素**：`FIELD_PAD_H` 12→16 连带 `textfield_caret`(13→17)、`input_caret_click`(点击 x 26→34) 需要更新；`otp_advance` 的「38 宽 + 8 间距 ⇒ 格距 46」算术在 `CONTROL_HEIGHT+8` 下失配，已重写为**从 `draw_tree` 里取所有 `StrokeRRect` 按 x 排序、主色那一格的序号即光标格**（尺寸/间距再变也不误判）。
- 验证：`cargo check --workspace --examples --tests` 无 error；`cargo test -p compose -p compose-material -p compose-render --tests`（逐 target 跑）**111 用例全绿**。
- **环境坑（本轮踩到，务必记住）**：
  1. `cargo run -p compose-render --example widgets_shot` 的 `target/shots/*.png` 是**相对当前 shell 目录**写的。若在 `crates/compose-render/examples` 下执行会写到那里的 `target/shots/`，而 Read 工具按路径缓存图像 → 反复看到旧图、误判改动无效。**永远在工作区根目录跑**，且每次渲染后 `cp` 成新文件名再 Read。
  2. UNC 共享上单个文件可能被 VM 侧句柄独占，报 `EPERM`/`permission denied when opening file`（本轮 `crates/compose/tests/pipeline.rs` 全程不可读，`cargo test --workspace` 因此编不过）；与代码无关，改用逐 `--test <name>` 跑其余 target 即可，别去删那个文件。
- 待办：`ColorScheme` 缺 `surface_container_low/_highest`（Elevated Card、Slider 轨道只能近似取色）；story 动画演示迁移、`layout.rs` `fw`/`fh` 死赋值、`ripple_burst` flaky 不变。

## 2026-11-03（续）修复下拉菜单 `RefCell already borrowed` panic

- 用户报运行时崩溃 `crates/compose/src/runtime.rs:170: RefCell already borrowed`。根因：`compose-material/src/widgets/input.rs` 的 `menu_panel` 把每个菜单项建成 `Policy::Leaf` 节点，其**绘制闭包内调用 `hovered.get()`（MutableState）**；`perform_layout` 全程持 `RUNTIME.borrow_mut()`，而 `leaf_intrinsic` 正是在这个借用里**执行 Leaf 闭包**（探测固有尺寸）→ 嵌套可变借必 panic。M3 悬停状态层改造新引入该闭包，故此前不曾触发。
- 修法（与既有 `Switch` 同模式）：把依赖登记上移到组合期——`let is_hover = hovered.get(); let label = it.clone();` 放在 `with_rt_pub` 之前，绘制闭包只读快照布尔值；`hover_handler` 仍 `hovered.set(enter)`（处理器由 `dispatch_hover_*` 在借用外 take-put 执行），set 触发项作用域重组 → 新闭包取到新快照。
- 教训/约定再确认：**任何 Leaf 绘制闭包（含 `canvas`/`Canvas`）内禁止读 `MutableState`、禁止 `theme_colors()`/`add_node`/`remember`/`invalidate_draw_node`/`schedule_scope`**。要在绘制里反映状态，要么组合期取快照（响应式，值变即重组），要么用 `Rc<Cell<..>>`（非响应式，配 `invalidate_draw_node`，见 `draw_phase`）。
- 全仓扫描（awk 跟踪 `with_rt_pub(...)` / `.draw = Some(...)` / `Canvas(...)` 块体内的 `get/set/update/read_clone/theme_colors/remember/add_node/invalidate_draw_node/schedule_scope`）：其余命中点**全部**是 `Rc<Cell>`（`draw_phase` 的相位、OTP `focus`、RichText `scroll`、animation/advanced/navigation 的偏移单元）或 `on_click`/`hover_handler` 块（借用外执行），确认无同类隐患。
- 新回归测试 `crates/compose-material/tests/menu_hover.rs`（2 项）：①反复 `perform_layout` 菜单浮层不得重入运行时借用 + 未悬停零状态层；②悬停画出 8% `FillRect` 状态层、离开后撤除。**已验证该测试能抓住原缺陷**：临时把闭包改回 `hovered.get()` 后两个用例均在 `runtime.rs:170` 复现 panic，改回快照后转绿。
  - 测试坑：重组会重建节点并在 `layout.rs:754` 顺带清掉 `rt.hovered_node`，故「移开」断言前要先按当前节点 `dispatch_hover_at` 重新登记，否则 `dispatch_hover_left()` 返回 false。
- 验证：逐 target 跑 `cargo test -p compose -p compose-material -p compose-render --tests` 全绿（新增 2 项；`compose/pipeline` 仍被本会话前述 UNC `EPERM` 文件锁挡住，与代码无关）；`cargo check --workspace --tests`、`cargo check -p examples --examples` 无 error；`widgets_shot` 从工作区根重绘，命令数 213、图像与修前一致（无悬停时行为不变）；`story.exe` 实跑 6s 存活无 panic（本会话 Windows 宿主，`target/debug/examples/story.exe` 构建成功）。
- 注：`crates/compose-render/examples/pngenc.rs` 是共享库模块（被 `widgets_shot` 以 `#[path]` 引入），`cargo check --examples` 会把它当独立 example 报 E0601「no main」；日常用 `cargo check --workspace --tests` 规避，不必改代码。

## 2026-11-03（续）M3 色彩角色补全 + 引擎级按压指示（波纹 / 抬起结算点击）

### 色彩角色（对齐 Flutter `ColorScheme`）
- `crates/compose/src/material.rs`：`ColorScheme` 补齐 M3 全角色 —— `surface_container_{lowest,low,high,highest}`（`surface_container` 即基准档）、`surface_dim`/`surface_bright`、反色三件套 `inverse_surface`/`inverse_on_surface`/`inverse_primary`、`outline_variant`、`scrim`、`shadow`；light/dark 均取 M3 baseline hex（light `FFFFFF / F7F2FA / F3EDF7 / ECE6F0 / E6E0E9`，dark `0F0D13 / 1D1B20 / 211F26 / 2B2930 / 36343B`）。
- **`scrim` 两套主题统一 32% 黑**：M3 barrier 语义如此，暗色下再加深会让被遮内容彻底不可见。
- 消费点：`outline_variant` 取代原先过重的 `outline`（Divider / Card / OutlinedCard / GroupBox / 表格与图表网格线 / 输入框边框 / 弹层描边）；`surface_container` 用于 Card / ElevatedCard / SearchBar / Banner；BottomSheet 与 ModalDrawer 文档标注 `surface_container_low`。
- **仍待迁移**：`Tooltip` 目前用 `on_surface` 当底、`Surface` 当字（应改 `inverse_surface` / `inverse_on_surface`），`surface_container_lowest/high/highest` 与 `surface_dim` 尚无消费者 —— 归到动效那一轮一起做。

### 引擎级按压指示（Flutter `InkWell` / M3 ripple）
- 目标：**每个** clickable 节点自动获得波纹与状态层，不需要逐个控件接线；点击在**抬起**时结算。
- `crates/compose/src/layout.rs` 新增「按压」一节：`ModElem::NoIndication`（+ `Modifier::no_indication()` / `Modifier::ripples_off()`）、`Press{click_node, ripple_node, gx, gy}`、`Ripple{node, gx, gy, down_ns, up_ns, max_r}`（`up_ns == f64::MAX` 表示仍按住）、`dispatch_press_down` / `dispatch_press_up` / `press_active` / `tick_press_state`；参数 `RIPPLE_SPEED=900px/s`、`RIPPLE_FADE_SECS=0.35`、`RIPPLE_ALPHA=0.10`、`TAP_SLOP=8`、`RIPPLE_MAX=6`。
  - **默认开启、显式关闭**：统计下来 compose-material 有 50 处 `on_click`，而需要关的只有 6 处全屏遮罩（Dropdown/Dialog/Snackbar 等 scrim），故取「opt-out」而非 `indication()` opt-in。波纹半径由节点自身 `Background`/`Clip` 圆角推出，无需按控件传形状。
  - 时钟用 `PRESS_EPOCH: thread_local Instant`（**墙钟**），不能用 `frame_time_nanos`：帧钟在宿主休眠时冻结，空闲很久后的一次按下会「一出生就长满」。
  - `push_ripples` 挂在 `draw_node` 前缀段末尾 ⇒ 墨画在节点自身背景之上、孩子之下（Flutter ink 层语义），用 `ClipPush/ClipPop` 裁到形状。
  - **剪枝必须补一次失效**：`tick_press_state` 里 `retain` 丢掉淡完/无主的波纹后，若不对其归属节点再走一遍 `invalidate_self_and_ancestors`，该节点缓存的 `DrawLayer` 前缀会永久残留一个淡到一半的圆。
  - 按住已长满的 splash ⇒ `ripple_animating` 转假，不再标脏、不再 `request_anim_frames` ——「静止即停驱」不变式保住（抬手事件本身会唤醒宿主，淡出照常）。
- `crates/compose/src/runtime.rs`：`RuntimeData` 增 `press` / `ripples` 字段；`advance_frame` 在 `tick_animations()` 之后、`run_frame_drivers()` 之前调 `layout::tick_press_state()`（推进绘制状态，**不进组合**，不触发重组）。
- 新增 `Node.on_press`：按下即触发的手势。这是抬起结算带来的功能回归修复 —— splitter 抓取与 Slider 拖动原本挂在 `on_click` 上。
- `crates/compose-render/src/kernel.rs`：`pointer_down` 只 `dispatch_press_down`（`dispatch_click_at` 调用点删除）；`pointer_up` 先 `drag_end_global || pull_end_global || scrollbar_end_global` 结算手势，再 `dispatch_press_up(hit, x, y)`。`host.rs` 补 `WE::MouseInput { state: Released, button: Left }` 臂（此前只有 Pressed）。
- 拖拽会话泛化：`begin_drag_session(..., lo, hi)` 增加钳制区间（splitter 0.05..0.95、Slider 0..1），`DragSession` 增 `lo/hi` 字段；compose-material 侧 `split_grab()` 在组合期读 `last_pointer_global()` + `node_abs_rect(id)`；`Slider` 用 `on_press` 起会话并 `.no_indication()`。
- `crates/compose-material/src/tokens.rs` 删除 `RIPPLE_SPEED` / `RIPPLE_FADE_DURATION` / `TAP_SLOP`：**波纹参数归引擎所有**，控件层不再重复定义。
- 测试：新增 `crates/compose/tests/press_ripple.rs` 5 项（按下不触发点击 / 越 slop 取消 / 落到别处取消 / 波纹画在按压点且淡出后被剪掉 / `no_indication` 抑制波纹但点击照常）+ `drag_session.rs` 4 项补 `lo/hi` 实参。
  - 写这类测试的坑：`Composition::set_content` 要求 `impl Fn()`（可重复调用），所以 `move` 闭包不能消费捕获；嵌套 `move` 闭包共享 `Rc<Cell<_>>` 还会 E0507 → 计数改用 `thread_local! { static HITS: Cell<usize> }` + 无捕获的 `fn bump()`。`Constraints::new` 非 `const fn`，窗口约束写成 `fn win()`。
  - 淡出窗口用墙钟，测试只能按真实时间轮询：`while t0.elapsed() < 420ms { advance_frame(1/60); sleep(5ms) }`。
- 验证：逐 target `cargo test` 全绿 —— `compose` lib 1 + 19 个集成 target（含 press_ripple 5）、`compose-material` 10 个 target 19 项、`compose-render` 5 passed/4 ignored；`cargo check -p compose-platform -p examples --examples` 无 error。`crates/compose/tests/pipeline.rs` 仍是 UNC `EPERM` 文件锁，与代码无关。
- 下一轮待办（动效对齐）：Dialog/菜单 fade+scale 入场、Tab indicator 滑动、NavigationBar 胶囊、Switch/Checkbox 插值、chevron 旋转、Tooltip 延时与 `inverse_*` 取色；旧待办不变：story 动画演示迁移、`layout.rs` `fw`/`fh` 死赋值、`ripple_burst` 偶发 flaky。

## 2026-11-04 crate 合并：compose-render + compose-platform 并入 compose（workspace 5→3）

- 目标：workspace 由 5 crate 收敛为 3（`compose` / `compose-material` / `examples`），删除 `crates/compose-render`、`crates/compose-platform` 目录。保留命名空间：`compose::render::*`（display_list/gpu/kernel/host/fonts）、`compose::platform::*`（desktop/web/android/ios/ohos/services）。
- **循环依赖反转**：合并后 `compose::render::{kernel,host}` 仍会调用 compose-material 的输入路由（原 14 处 `Input*`/`SetShiftState`），而 material 依赖 compose → 成环。新建 `crates/compose/src/input_router.rs`：`struct Handlers`（13 个函数指针字段）+ `thread_local!` 注册表 + `install(&'static Handlers)` + 13 个转发函数（未注册时安全 no-op，`focused()` 返回 false）。compose 核心对 compose-material **仅保留 dev-dependency**，无编译期依赖。
- compose-material 侧新增 `pub fn install_router()`：用本 crate 13 个无状态自由函数绑定 `INPUT_HANDLERS: static Handlers`，调用 `compose::input_router::install(&INPUT_HANDLERS)`。因 MaterialTheme 在核心无法自动挂载，入口示例（story/gallery）在 `run_app_with(...)` 前显式调用一次。
- 迁入代码路径改写：`extern crate self as compose;` 兜住海量 `use compose::…` 自引用（edition 2024）；render 内互引 `use crate::{display_list,gpu,kernel,fonts}`→`crate::render::{…}`；`compose_render::`→`crate::render::`（render/platform 内）或 `compose::render::`（集成测试）；`kernel.rs`/`host.rs` 的 14 处输入调用改走 `crate::input_router::{…}`。
- Cargo：`crates/compose/Cargo.toml` 追加 render+platform 依赖（log/valo 0.13/wgpu 30/pollster 1/winit 0.30/raw-window-handle 0.6）+ 保留 target-cfg 段（wasm32/android/desktop）+ `[lib] crate-type=["rlib","cdylib","staticlib"]`（承接 Android/iOS/OHOS 产物）；根 workspace members 删两 crate；examples 删 compose-platform 依赖。
- `compose/src/lib.rs` 根级 re-export（App/Kernel/GpuRenderer/WindowConfig/run_app/run_app_with + android 入口），业务侧 `use compose::{App, WindowConfig, run_app_with}` 保持可用。
- 构建脚本：`examples/scripts/build_ios.sh` `--lib`→`-p compose --lib`、产物 `libcompose_platform.a`→`libcompose.a`；其余 build_android/apk/web/ohos 注释措辞 compose-platform→compose。
- 验证：`cargo check -p compose`/`-p compose-material`/`-p examples --examples` 全绿（仅历史 style 告警）；`cargo test --workspace --tests` 全 `test result: ok`、0 failed；`cargo check -p compose --target aarch64-linux-android` 通过。

## 2026-11-08 滚动条 gutter 引擎化 + Sidebar folder 视觉区分 + ColorPicker HSV 取色器

### 滚动条贴窗口右缘 + 布局期为滚动条预留 gutter
- 症状：内容区滚动条离窗口右缘太远，且浮在内容之上遮挡控件。此前在 story 手工塞 padding 是治标。
- 引擎级修复（`crates/compose/src/layout.rs`）：新增常量 `SCROLLBAR_THICKNESS=8.0`/`SCROLLBAR_MARGIN=2.0`/`SCROLLBAR_GUTTER=12.0` + 辅助 `scrollbar_gutter_for(rt, id, is_row)`（该 Row/Col 若自身是交叉轴方向上的滚动容器则返回 12，移动端 `is_mobile()` 返回 0）。`measure_row_col` 用 `cross_max_c = cross_max - reserve` 收紧孩子交叉约束，`place_node` 的 `cross_size` 同样扣 `reserve` ⇒ Center/End 对齐内容不落进滚动条区。`scrollbar_geometry` 复用这三个常量。滚动条画在滚动节点外框右缘（与节点内部 padding 无关），story 侧回退手工 padding：内容 Row 右内边距 14→0（滚动条贴窗口右缘），VerticalScroll 恢复 `padding_hv(4,2)`。

### Sidebar folder（分组标题）与 item（条目）视觉区分
- `crates/compose-material/src/widgets/navigation.rs`：folder 标题 `InkWell` 底色 `colors.primary.with_alpha(0.35)`（圆角 8）色带，与中性色条目明显区分（历经 surface_variant 灰底→不明显→primary alpha 0.22→0.35）；folder 文字 `label_large(...).weight(700)` 加粗；箭头/徽标文字保持 `on_surface_variant`。
- **组间间隔**：folder `InkWell` 前插入 `if si > 0 { Spacer(height = SPACE_SM = 8px) }`，避免相邻 folder 色带紧贴（首个分组上方已有栏头故不加）。

### ColorPicker 重写为 HSV 取色器 + 引擎二维拖拽
- `crates/compose-material/src/widgets/input.rs`：`ColorPicker(color: MutableState<Color>, modifier)` 从色板圆点网格改为标准 HSV —— SV 方块（15×10 离散色块 + 选中圆环）+ 竖向色相条（60 段 rainbow + 白色横条指示）+ 底部预览条。内部 `h_st/s_st/v_st` 走 `remember`（首次从传入 color 播种，低饱和时 hue 不丢），每次改动同步写回 `color`；新增 `hsv_to_rgb`/`rgb_to_hsv`（无现成 helper）。
- 引擎新增通用二维拖拽（`layout.rs`）：`thread_local DRAG_2D: RefCell<Option<Box<dyn FnMut(f32,f32)>>>` + `begin_drag_2d(on_move)`/`drag_2d_move_global(x,y)`/`drag_2d_end_global()`；`kernel.rs` 的 `pointer_move` 加 `drag_2d_move_global` 分支、`pointer_up` 的 gesture 链加 `drag_2d_end_global`。
- **命中链真 bug（用户「鼠标无法取色」）**：`dispatch_press_down` 靠 `nearest_click_idx`（沿祖先链**只认 `on_click`**）判定命中，找不到即 `hit=false` 提前返回，`on_press` 永不执行。ColorPicker 的 SV/hue 节点只设了 `on_press` 漏了 `on_click` → 取色静默失效。修复：给两节点各补空 `on_click`（取色已在按下瞬间完成）。**引擎级约定：任何用 `on_press` 做按下手势的控件必须成对配 `on_click` 放行命中链**（Slider/Splitter 早已如此）。
- story 页分发坑（勿把行号当页下标）：`PAGES` 下标 0..102，badge 显示 1-based；ColorPicker 真实页下标 = 75（非源码行号 127），分发 `36 => d_color(), 75 => d_color_picker()`。
- 验证：`cargo build --example story` 通过；`cargo test -p compose -p compose-material --tests` 全绿（无回归）；实跑截图确认侧栏色带间隔与滚动条贴右缘正常。GUI 鼠标注入本机不可用，ColorPicker 交互以逻辑约定 + 无头断言为准。

### Color 页色板不显示颜色（Card 自带 surface 盖住自定义底色）
- 症状（用户「各个 Color 框没显示颜色」）：story `d_color()`（页下标 36、badge 37）色板框只在圆角错位处露一点色边，主体是浅灰。
- 根因：`Card(modifier, content)` 内部走 `GlassOrBg(modifier, g, colors.surface_container, SHAPE_MEDIUM)`，玻璃关闭时执行 `m.background(surface_container, …)`。调用方传入的 `.background(col)` 是**外层**、Card 的 surface 是**内层**，绘制时内层盖在外层之上 → 色板色被遮。`GlassOrBg` 语义即「容器控件总铺自己的主题底」。
- 修复：色板改用无自带底色层的 `Box(Modifier.size(48,48).background(col, 6.0).border(1.0, outline_variant, 6.0), TopStart, ||{})`，直接铺 `col`。
- **可复用教训**：`Card`/`FilledCard`/`Surface` 这类容器控件总会铺自己的主题底色，不能靠外部 `.background()` 覆盖来展示自定义颜色；纯色块/色板应使用无底色层的 `Box`。

### 文档全面校正（README.md 对齐当前代码）
- README 长期滞后于代码，本轮按现状逐项校正：①渲染管线 valo 0.9→**0.18**（Cargo.toml 实值）；②顶部示例旧 API `column(c,…)`/`remember(c,…)` → 现代 PascalCase 无 `&mut Composer` 首参（`Column(…)/Text(…)/Button(…)/remember(||…)`），架构表 `@Composable` 行同步；③示例入口 **gallery→story**（全文：运行/跨平台构建/build_apk/已知限制），移除已废弃的 `--dark` 启动参数（现由 Theme 菜单运行时切），补 `STORY_PAGE=<下标|页名>`；④gallery 7 页描述 → story 103 页 / 7 分组（Demos/Basics/Data Display/Layout/Navigation/Feedback/Advanced）；⑤侧栏 `GROUPS` 名与 story 对齐；⑥液态玻璃函数 snake_case→PascalCase（`ToggleLiquidGlass`/`SetLiquidGlass`/`glass_spec()` 无参/`LiquidGlassWallpaper(colors)`）；⑦浮层宿主 `OverlayHost/Overlay/RememberOverlayKey` 去 `c` 参；⑧控件模块表补 `chart.rs`/`grid_sheet.rs`/`rich_text_editor.rs`（实际 13 模块）；⑨菜单栏 Theme 补种子色板 `THEME_PRESETS`+`ColorScheme::from_seed`。AGENTS.md 为历史日志，保留旧记录原样，仅追加本会话新条目。

## 2026-11-09 examples/story.rs 拆分为多文件模块

### 拆分结构
- 动机：`examples/story.rs` 单文件 5710 行 / 156 个顶层项，改任一页都要在 5.7k 行里定位，维护成本高。
- 入口与模块声明留在 `examples/story/main.rs`（151 行，含内联的无头探针 `probe_state` / `probe`），实现拆到同目录：
  - `story/prelude.rs`（35 行）——原文件顶部 `use` 列表整体搬入并改写为 `pub use`，各模块经 `use crate::prelude::*;` 引入（免逐模块重复、保持「改一处即全应用」）；
  - `story/shell.rs`（534 行）——应用外壳：主题 / 标题栏 / 菜单栏 / 侧栏 / 状态栏 / 页面容器；
  - `story/common.rs`（77 行）——跨分组共用：`section` / `card_section` / `nav_items` / `nav_sections` / `thread_local`（`MENU_ANCHORS` + `SNACK_HOST`）；
  - `story/pages/mod.rs`（303 行）——`PAGES` 注册表（标题/图标/说明/渲染函数唯一真源）+ `build_page_body` / `build_page_title` + 7 个分组子模块声明；
  - `story/pages/{demos,basics,data,layout,navigation,feedback,advanced}.rs`——按 `PAGES` 的 7 分组归置（demos 2497 行含动画 helper，data 730 行，其余 210~430 行）。
- `examples/Cargo.toml`：example 入口改 `path = "story/main.rs"` + 加 `autoexamples = false`（example 位于子目录，关闭自动发现避免重复目标）。原 `examples/story.rs` 已删除。

### 拆分方法与踩坑（可复用经验）
- **不要手工搬运 5.7k 行**：用一次性脚本按「顶层项」切片重组——定位第 0 列声明行（fn/struct/impl/const/mod/use/thread_local!）与 `#[...]` 属性行，块 = [块起点, 下一块起点-1]；属性行并入所属项后回溯截断上一块结束行，保证每行恰好归属一块（不重不漏）。
- **坑① 函数体内缩进的 `#[cfg(test)]`**（`story_shell` 内的探针块）会被误判为顶层属性 → 属性检测必须限定第 0 列，否则会凭空多出「声明项」并切坏函数体。
- **坑② 注释上吞会与上一块重叠** → 上吞 doc 注释后必须把上一块结束行截断到本块起点-1；否则同一段注释在相邻两模块各出现一次（曾致 prelude.rs 尾部悬空 doc 注释 → `expected item after doc comment`）。
- **坑③ 批量提可见性时正则吞类型** —— 字段提 `pub(crate)` 用 `re.sub(r'^(    )(\w+):', r'\1pub(crate) \2:', l)` 只替换前缀；直接重写整行会把 `&'static str` 等类型删掉。
- **坑④ 跨模块引用须显式导入** —— `probe_state` 是 crate 根的 `#[cfg(test)]` 模块，shell 侧需 `#[cfg(test)] use crate::probe_state;`；`nav_items` / `nav_sections` 被外壳侧栏与 NavigationRail 页共用 → 下沉 `common.rs`；`common.rs` 又需 `use crate::pages::{GROUPS, PAGES};`。
- prelude 的 `use` 必须写 `pub use`，否则 `use crate::prelude::*` 拿不到任何名字（表现为大批 `cannot find type`）。
- 模块间循环引用合法：`pages/mod.rs` → `use crate::shell::page_group;`，shell → `use crate::pages::{GROUPS, PAGES, build_page_body};`。

### 验证
- `cargo build --example story` 通过（0 error；拆分未引入新警告，唯一 `Tabs` 命名风格警告为既有）。
- `cargo test --example story` 通过：`probe::menu_dropdown_flow ... ok`。
- 实跑 `target/debug/examples/story` 正常：GPU 适配器就绪、64 字体加载、无 panic。
- 脚本自检：切分 5705/5711 行，未覆盖的 6 行即原模块文档头（已在 main.rs 复现）。

## 2026-11-18 Docking 对齐 VS2022（指南拖放 / 标签插入 / 自动隐藏 / 浮动 / 上下文菜单）

- 拖放判定从「矩形五区盲判」改为 **VS 指南命中**：`DockGuide::{Inner{target,dir},Outer{dir},Tab{lead,index}}` + `dock_inner_guides/dock_outer_guides/dock_guide_hit`（±DOCK_GUIDE_PAD）。只有南针/外圈/标签条是落点；无指南=取消、窗外=浮动。旧 `dock_direction_in_rect/dock_preview_rect` 保留（测试仍用）。
- 标签条：文档井顶部 2px 主色 accent、悬停才显关闭、pin/▼/× 按钮组、超宽折行省略（`measure_text_with_measurer`）、溢出 chevrons 走引擎新增 `scroll_by/scroll_metrics`（`HorizontalScroll/VerticalScroll` 改返回 NodeId）。拖到别组标签条显**插入光标**（竖线+上下三角）+ 条高亮。
- 自动隐藏：竖排标题边条；点击滑出**贴边整条 peek**（侧边=全高×clamp(宽*0.30,240,420)，顶底=全宽×clamp(高*0.32,180,360)），`animate_progress_on(id,Tween(0.18))` 滑入；peek 标题竖放图钉=钉回。
- 浮动窗：两层软阴影 + 八向缩放手柄 + 双击标题回 dock + 拖标题回主窗口指南重新停靠。**阴影坑**：全屏 `Canvas(fill_max_size)` 绝对坐标画阴影在浮层重组时残留旧位（窗上方灰块）；改为与窗框同 offset 落位路径的 `background(BLACK alpha)` 节点（`no_hit`）即逐帧同位。教训：浮层投影别用全屏绝对坐标 Canvas，用与主体同落位路径的底色节点。
- 上下文菜单 VS 项：关闭/关闭其他/全部关闭（组>1 才显）/分隔/浮动为窗口/自动隐藏/最大化；宽 190、行 32、分隔 9。不可关闭面板（解决方案资源管理器）菜单无「关闭」——非缺陷。
- 引擎/导出：`lib.rs` 根 re-export 是**选择性列表**，新增 pub API 必须同步加进去（否则 examples E0425）；本轮补 DockGuide/dock_*_guides/dock_guide_hit/DOCK_GUIDE_*/DOCK_TAB_MAX_W。
- 测试修正：①玻璃菜单宽 180→190；②peek 改贴边整条后旧「两个全尺寸玻璃面」断言过时，改为泵帧（advance_frame+layout+draw ×24）等滑入到位再断言边条尺寸（首帧根未测量 dock_layout_rect=0 属正常）；③`list_custom_item.rs` 的 `rows()` 误把 `focusable_full` 焦点容器（空 on_click+key_handler）计为行、且按 arena 下标序（重组会乱）→ 过滤 `key_handler.is_none()` + 按 abs y 排序。属焦点系统遗留测试缺陷，非本轮回归。
- GUI 验证工具（D:/rc-target，不进仓库）：`capture.ps1`(PrintWindow 2)、`drag.ps1`(左键拖)、新增 `drag_preview.ps1`(按住不放供截屏)、`rclick.ps1`(右键)。**截屏必须在同一 bash 命令内「启动拖拽&→sleep→capture→wait」**，跨工具调用的间隔延迟会让 hold 提前结束、截到释放后画面。慢拖（steps40×100ms）才能截到移动中的指南/光标/幽灵。
- 验证：docking/dock_layout/dock_contract 54 项全绿；compose、compose-material 全量 --tests 全绿；examples check 无 error；实跑截屏确认 浅色/暗色 VS 工作区、罗盘+外圈+幽灵+预览、标签插入光标、自动隐藏边条+滑出 peek、上下文菜单、浮动窗+正确阴影、拖浮窗回指南（脱靶=取消正确）。

## 2026-11-18（续）修复分割条拖拽「串层」：拖下方分割条却改右侧比例
- 用户报：拖下方 Dock 分隔条，变动的却是右边 Dock 的分隔大小。
- 根因（`docking.rs` `set_ratio_by_path`）**path 语义 off-by-one**：render 侧 `splitter_path = 分割节点自身 path`（根=`[]`、右=`[1]`、下=`[1,0]`），但旧 `set_ratio_by_path` 在 `path.len()==1` 时就写**当前**节点比例（只导航 len-1 步）→ 下`[1,0]`导航到右分割 B 后写 B（串层）；右`[1]`写根（改左栏宽）；根`[]`被 `is_empty return` 直接忽略（根分割条拖不动）。
- 修复：改为「沿整条 path 走到目标分割节点再写比例；空 path=当前节点」。即 `if path.is_empty(){*r=ratio;return;} let child=if path[0]==0{first}else{second}; recurse(child,&path[1..])`。三个层级（根/右/下）各自隔离。
- 测试：`docking.rs::set_split_ratio_only_touches_addressed_level` 旧断言用 `[1,1]` 寻址内层（对着 buggy 实现写的多余尾元素）→ 改为节点自身 path `[1]`，并补「拖根 `[]` 只改外层、内层保持」双向隔离断言。
- GUI 实测（drag.ps1 + capture.ps1）：下分割条 (500,620)→(500,700) 只改文档/输出高度、右边界 x≈890 不动；右分割条 (890,400)→(800,400) 只改中/右宽、下分割 y 不动；根分割条 (231,400)→(330,400) 只改左栏宽（中+右按 B 比例同步缩）、下分割 y 不动。**根分割条仅 4px（DOCK_SPLITTER_PX），x=228 会落在侧栏滚动条 gutter 上拖不动，须 x≈231 命中**。
- 验证：docking/dock_layout/dock_contract 全绿；compose-material 全量 --tests 全绿。

# 2026-11-24 Jetpack Compose 架构对齐专题（本会话）

## 用户约束
- **后续只验证 desktop**（不再逐端跑 wasm/android）。本会话所有验证均为 `cargo test --workspace --tests` + `cargo check --example story -p examples`。
- 主题：逐项对齐 Jetpack Compose 核心架构，并在 story 中提供可交互演示。

## 本轮新增核心模块（compose）
- `slot_table.rs` — SlotTable gap-buffer：线性 `Vec<Slot>`（Group/Data/Gap）+ 可搬移 gap（原地 `rotate`，不 Clone `Box<dyn Any>`）+ group 头/Anchor + `truncate_data` 回收；`ScopeData.slots: Vec` → `SlotTable`，`remember/keyed_remember/launched_effect/disposable_effect` 全部改 `get_data/set_data`。`run_scope` 未启用 truncate（保持位置身份兼容）。
- `applier.rs` — `NodeApplier` trait（对齐 Compose `Applier<N>`）+ `ArenaApplier` + 线程局部可替换实现；`add_node` / `run_scope` 摘子树 / `dispose_scope_rt` / `sweep_nodes` 四处树变更收口；新增 `move_node`。**关键坑**：`clear/remove` 必须保留子节点 `parent` 字段（GC 可达性安全网），初版置 None 导致 `list_custom_item` 回归。
- `snapshot.rs` — 快照栈 + 隔离写集合 + 读缓存 + read/write observer + `SemanticsTree` 无关；`MutableState::{get, read_clone, read_unregistered, set, update}` 接入快照读/写；`apply_snapshot` 在 runtime 提交并失效。嵌套快照读缓存语义：外层已缓存的值不受内层 apply 影响。
- `modifier_node.rs` — `ModifierNodeImpl` + arena 节点 + `ModElem::Node` + `Modifier::{with_node, node_ids}` + 公开 `then/then_if`（原私有 `then(ModElem)` 改名 `push_elem`）；measure/draw 两阶段按链序调用。`CompositionLocal` 读取依赖确认不注册（Compose 语义正确）。
- `gesture.rs` — 统一 `PointerEvent{type,x,y,dx,dy,time_ns}` + `PointerInput` 组合函数（按 NodeId 注册 handler，沿祖先链派发）+ `detect_tap_gestures`（tap/double/long，8px slop）+ `detect_drag_gestures`（touch slop）；`dispatch_press_down/up` 派发 Press/Release，新增 `dispatch_pointer_move`。
- `semantics.rs` — 类型化 `SemanticsPropertyKey<T>` + 标准属性 + `Role`/`SemanticsAction` + `SemanticsConfig` + arena `SemanticsNode`/`SemanticsTree`（查询/命中/焦点/perform/JSON）+ `build_tree`（显式 > 派生 > DrawCmd::Text，merge/clear/hidden）；`Modifier::{semantics, content_description, test_tag, role, merge_descendants, clear_and_set_semantics, hide_from_accessibility}`。**坑**：`build_tree` 必须写入线程局部 TREE 供 `with_tree` 读；`node_absolute_bounds` 与 `with_rt_pub` 不能嵌套（RefCell 重入）。
- `subcompose.rs` — measure 期请求队列 + `MeasureCtx::subcompose(slot, content)`；`perform_layout` 改两趟（measure 收集 → 无借用期 `subcompose_scope` keyed 组合 → 重测反馈），上限 8 轮。`runtime::subcompose_scope` 临时压入父 scope/node 栈复用 `emit_keyed_scope`。
- Effects 完善：`TaskSlot.owner` + 作用域销毁取消协程（`spawn_task` 需在借用外取 owner，否则嵌套借 panic）；真正 `SideEffect`（post-recompose 队列，`recompose_pending` 每批后 + `set_content` 末尾 drain）；`rememberUpdatedState` / `snapshotFlow`（derivedStateOf 复用）/ `producedState`。
- GraphicsLayer：`DrawCmd::{SaveLayer, Transform, Restore}` + Hash 22/23/24 + `is_clip` 归状态栈（不进静态块）+ `Modifier::{graphics_layer, alpha, rotate, scale}` + valo `save_layer` 映射（Transform 暂降级 no-op，valo 0.13 无 builder 级变换）。

## compose-material 新模块
- `transition.rs` — `AnimatedVisibilityEx`（Enter/Exit 生命周期，exit 进度归零后下一轮重组 dispose 内容）+ `AnimatedContent`（`animate_progress_on(key)` 重播入场）+ `VisibilityTransition`/`ContentTransform`；走 `Modifier.graphics_layer`。**坑**：不能用 `animations_active()` 判断收敛（读即清 FRAME_REQUEST）；不能嵌套 `move` 闭包传 content（E0507），直接把 content 传给 LayoutBox。
- 虚拟列表 `lazy_list.rs` 加 `selected: MutableState<usize>`，点击整行 `secondary_container` 高亮（行节点挂 on_click，点击 padding 亦归属该行）。
- RichTextEditor 文本拖选：`on_press` → `click_to_caret` 建 anchor + `begin_drag_2d` → `drag_to_caret` 保留 anchor 扩选；`on_click` 留空避免重复定位清锚点。
- Docking：拖拽入口收窄到 tab / tab 条同一行，移除面板内容区 `on_press`（内容拖动不再触发 DnD 提示）。

## 平台 / 构建 / 网络
- Android：入口下沉引擎层 `compose::platform::android::android_main` + 应用层 `compose_create_app_root` 注入，最终 `.so` 由 examples `story_android` cdylib 链接（对齐 cui `cui_core`/`examples` 分层）；Manifest 须 `android:hasCode="false"` + .so STORED 16KB 对齐；字体扫 `/system/fonts`（Roboto/DroidSansFallback）。
- `platform/services/network.rs`：`TcpStream` HTTP/1.1（http:// GET/POST），移动端经 channel；Android Manifest 加 INTERNET 权限。
- wasm32 `Instant` 用 `web_time` cfg 别名（winit `ControlFlow::WaitUntil` 类型匹配）。
- 构建脚本：`build_cross.sh` / `build_web.sh` / `build_android.sh` / `build_apk.sh` 全部从已删除的 gallery 改 story。

## story 演示（现 112 页 / 8 分组）
- 新增 `pages/compose_alignment.rs` 8 页：SlotTable / Applier / Snapshot / Effects / Modifier.Node & GraphicsLayer / Subcompose & Semantics / pointerInput 手势 / AnimatedTransition。
- `PAGES` + `GROUPS`（Compose 对齐 104..112）+ 探针 `compose_alignment_pages_render_without_panic` 逐页渲染。
- **坑**：Effects 页的 SideEffect 不能写会触发自身重组的 state（无限循环）→ 用 `Rc<Cell>` 计数（示例层允许 Rc）。

## 验证
- 每步 desktop 全量测试无失败；新增单测/集成测试：slot_table 7、applier 6、snapshot 4+5、effects 5、gesture 4+4、semantics 7、graphics_layer 3、modifier_node 3+2、subcompose 3、transition 集成 4。
- 文档：README.md 架构表、模块清单、对齐边界、页数、验证策略全部同步；本文件追加本条。

# 2026-11-24 视频播放（参考 /work/study/cui/rui_packages/src/video.rs）

## 用户约束
- 仍只验证 desktop；不逐端跑 wasm/android。
- 要求参考 cui 的 `video.rs` 实现视频播放（来源探测 / 后端表 / FFmpeg 解码 / 状态机 / 控制器 / 控件族）。

## 实现要点
- `crates/compose-material/src/widgets/video.rs`：数据层（`VideoSource/VideoInfo/VideoFrame/AudioChunk`）+ 后端 trait/目录/偏好/能力 + `VideoPlayback` 状态机（媒体时钟、seek、循环、倍速、音量）+ `VideoPlayerController` + Composable（`Video`/`VideoControls`/`VideoPlayer`）+ `MockVideoBackend`。
- 帧管线：解码线程按 PTS 与墙钟对齐产 RGBA → **单槽 `Arc<Mutex<Option<VideoFrame>>>` 邮箱**（新帧覆盖旧帧，天然有界）→ 主线程帧驱动 `on_frame()` 逐帧 `next_image_id()` + `image_cache::submit_decoded` + `drop_image(prev)` → `Canvas` 闭包 emit `DrawCmd::Image`。视频控件自身帧驱动随作用域销毁自动注销，播放时 `request_anim_frames()` 续帧。
- 控制器是 `Clone` 共享句柄；`Drop` 必须用 `Rc::strong_count(&self.worker)==1` 判断是否最后一个句柄，否则任一临时 clone 被丢弃就 dispose 会话并清空帧槽（曾导致集成测试看不到 `DrawCmd::Image`）。
- `VideoPlayerValue` 用 `MutableState`（Copy 句柄）承载，seek/initialize/volume 乐观写 `cmds`，不等解码线程下一拍；`VideoProgress` draw 闭包用 `peek_mut` 无注册读，避免绘制期注册依赖。
- 音频：FFmpeg 后端能探测音轨（`has_audio_stream`），但工程无音频输出后端，播放静音（与参考的 FFmpeg 后端边界一致）。

## 构建与 FFmpeg
- `crates/compose-material/build.rs` + `csrc/ffmpeg_shim.c`：探测 vcpkg（`FFMPEG_DIR`/`VCPKG_*`，优先 `x64-linux-dynamic`）→ pkg-config 兜底，编译 C shim 并输出 `cfg(compose_ffmpeg)` 与链接参数；非桌面直接跳过。本机已探测到 vcpkg FFmpeg 7（libavcodec 63），真解码可用。
- C shim AVIO 所有权：自定义 AVIO 在 `avformat_open_input` 失败后不能再释放 `io/io_buffer`（FFmpeg 可能已回收），用 `io_owned` 标记区分；未修复前用 PNG 内存源失败路径会 `double free` SIGABRT。
- 失败路径测试：`ffmpeg_invalid_memory_source_fails_cleanly`；真解码用内存/文件 GIF（`ffmpeg_backend_decodes_{memory_bytes,real_bytes}`），PNG 单帧在 FFmpeg 7 的 `png_pipe` 探测下无法定尺寸，不作为测试用例。

## 验证（desktop）
- `cargo test --workspace --tests` 全绿；`cargo check --example story -p examples` 无 error。
- story 新增 113 页 / 9 分组：`Video` 页默认 Mock 播放，可原生对话框打开本地视频；`main.rs` 探针按页名加入 "Video"；`prelude.rs` 注释同步 113 页。
- 测试：`video.rs` 单测 6 + 集成 5（含 FFmpeg 真解码 3 项，均 gated `cfg(compose_ffmpeg)`）。

## 2026-11-24 修复 release story 找不到 libavformat.so.63
- 现象：直接运行 `target/release/examples/story` 报 `libavformat.so.63: cannot open shared object file`；debug 单测正常。
- 根因：`compose-material/build.rs` 的 `cargo:rustc-link-arg=-Wl,-rpath,...` **不会跨包传播**到 `examples` 包的 example 目标；`cargo check` 不链接所以此前未暴露。release 二进制只有 `NEEDED libav*.so`，没有 `RPATH/RUNPATH`。
- 修复：新增 `examples/build.rs`，按 `FFMPEG_DIR` / `VCPKG_INSTALLED_DIR` / `VCPKG_ROOT/installed/*` / pkg-config 探测含 `libavformat.so*` 的目录，对该包目标发 `cargo:rustc-link-arg=-Wl,-rpath,<dir>`；Linux 额外加 `--disable-new-dtags` 生成 `DT_RPATH`（间接依赖也覆盖），macOS 跳过该开关；非 linux/macos 直接返回，不影响 wasm/android。
- 验证：`cargo build --release --example story -p examples` 后 `readelf -d` 显示 `RPATH [/tools/dev-cache/vcpkg/installed/x64-linux-dynamic/lib]`，`ldd` 全部解析到该目录，启动无加载错误。

## 2026-11-24 修复 VideoProgress measure 期 RefCell 重入
- 现象：运行 story 进视频页 panic `runtime.rs:200 RefCell already borrowed`，栈为 `VideoProgress::{closure#0}::{closure#0}` → `measure_node` → `measure_row_col`。
- 根因：`Canvas` 的 draw 闭包不仅绘制时调用，`leaf_intrinsic` 在 measure 期会用 `DrawBuilder::probe()` 调一次；此时 `measure_node` 已持有 `RUNTIME.borrow_mut()`。进度条旧闭包用 `c_draw.value.peek_mut(...)`，内部 `with_rt` 再次 `borrow_mut()` 即 panic。
- 修复：进度条 draw 闭包改为直接读控制器共享的 `cmds.position_us`（`AtomicI64`）和 `info.duration_us`（`Mutex<VideoInfo>`），不碰 `MutableState` / runtime；既不重入也不注册组合依赖。
- 回归：集成测试 `decoded_frame_is_drawn_as_image` 由只组合 `Video` 改为组合完整 `VideoPlayer`（含 `VideoControls` → `VideoProgress`），覆盖 measure 期预探测；`cargo test -p examples --example story` 的 `compose_alignment_pages_render_without_panic`（含 "Video" 页）通过。

## 2026-11-24 修复 story「打开文件」后不播放
- 现象：选择本地文件后画面/状态无变化。
- 根因链：`open_file_dialog` 的回调**已在主线程**（`file_dialog.rs` 内部先 `post_to_main`）；旧 story 页又在回调里二次 `post_to_main` 投递路径，并且**提前**把 `wait` 原子标志清成 false。`Kernel::frame` 里 `advance_frame`（帧驱动）在 `drain_main_tasks` **之前**：帧 N 的驱动看到 wait=false 不再 `request_anim_frames()`，回调随后才把路径投递出去；此时宿主无待渲染帧，路径永远停在 channel 里。
- 修复：回调里直接 `tx.send(path)`（同线程）并补一次 `request_anim_frames()`；`wait` 只在帧驱动**真正消费并处理完路径**后清除，取消对话框时才在回调里清除。另外 Mock 演示源改为创建后立即 `play()`（此前文案说默认播放但实际未播放）。
- 可观测性：新增 `VideoPlayerController::open_with_error(VideoSource) -> Result<Self, String>`（`open()` 仍返回 Option），story 状态栏回显「打开 xxx 失败：原因」与解码错误；文件过滤器补 ts/flv/m4v 与「所有文件」。
- 验证：`video.rs` 集成测试增 `open_with_error_reports_reason`（6 项全过）；workspace 全量测试 + release 构建均通过。

## 2026-11-24 修复选文件后「不停闪」且仍不播放（真正根因）
- 现象：上一条修复后仍不播放，页面不停闪。
- **真正根因**：`page_video()` 是组合体，视频页每帧因时钟/进度状态重组。旧代码在组合体里直接 `let (tx, rx) = channel()` 且 `let rx = remember(...)`：remember 留住**首次** Receiver，而每次重组拿到的 `tx` 都是**新建 channel** 的 Sender —— 新 Sender 发往已被丢弃的 Receiver，`send()` 静默失败，路径永远收不到。同时每次重组都直接 `add_frame_driver(...)`，帧驱动不断累积且都看到 `wait=true`，每帧反复 `request_anim_frames()` → 看起来就是「不停闪」。上一条只修了 wait 清除时机，没修身份不稳定。
- 修复：把 `(Sender, Receiver)` 作为一个整体放进 `remember`（`Rc<(Sender, RefCell<Receiver>)>`），帧驱动注册也放进 `remember`，两处身份在重组间稳定；按钮闭包从同一个 `channel.0.clone()` 取 Sender。
- 回归：`examples/story` 新增 `video_page_recomposition_does_not_grow_frame_drivers`：进入 Video 页后连跑 12 帧，`frame_driver_count()` 必须与首帧相等（旧实现会持续增长）。现 14 项 story 探针全过。
- **教训**：本工程 `remember` 是「位置即身份」，但只保留**首次求值的返回值**；任何在组合期创建并需跨重组存活的资源（channel、frame driver、dialog handle）都要整体包进 `remember`，不能「对象每次新建、只把一半 remembered」。

## 2026-11-24 修复点击按钮后文本要移开鼠标才刷新
- 现象：SlotTable 演示页点「追加 slot」，上方统计文字不动；鼠标离开按钮后才更新。
- 根因：演示把表放在 `Rc<RefCell>`，另设 `rev = mutableStateOf(0u32)`，按钮里 `add_r.update(...)`；但**组合期没有任何作用域读 rev**。Compose 定向重组只会失效「读到该 state 的作用域」，无读者 ⇒ 不重组。宿主在 mouse-up 时虽 `request_redraw()`，但该帧无 pending 重组，Text 闭包仍是旧字符串；鼠标移动改变 hover 状态后才顺带重组刷新。
- 修复：在读取表格数据的 Column 作用域里 `let _ = r.get();` 注册依赖。按钮 `update` 立即失效该作用域并重组。
- 回归：`examples/story` 新增 `slot_table_button_click_recomposes_without_pointer_move`：找到「追加 slot」文字坐标→`click`→`recomp`（**不调用 pointer_move**）→断言统计文本变化。临时删掉修复行后该测试确实失败（`left == right`），恢复后通过。
- **通用规则**：演示里用 `Rc<Cell/RefCell>` 做非响应式探针时，必须配一个**在组合期被 `.get()` 读取**的 rev state，否则写它不会重组；`let _ = r;` 只是移动句柄，不算读取。已审计其余 compose-alignment 页均有对应 `.get()`。

# 2026-11-24 C++20 协程版 Jetpack Compose（cpp/）

## 调研结论
- `/work/study/cui/cui_core` 是 Flutter 风格三树引擎（Widget/Element/RenderObject + BuildOwner + PipelineOwner），**不是** Compose：无 SlotTable/Applier/Snapshot/Compose Modifier，可复用的是分层骨架与脏调度思想。`/work/study/compose-rs/cpp` 原先只有两个空目录。

## 实现
- 新建 CMake 工程 `cpp/CMakeLists.txt`（C++20，静态库 `compose_runtime` + `compose_tests`）；头文件在 `cpp/compose/include/compose/*.h`，实现在 `cpp/compose/src/*.cpp`（slot_table/state/applier/coroutine/composer），测试 `cpp/examples`。模板/内联留在头文件，非模板实现全部迁到 src。
- `slot_table.h`：gap buffer（逻辑→物理映射必须走 `gap_end_`，删除圈回 gap、插入复用）。
- `state.h`：`State/MutableState`、`mutableStateOf`、读注册依赖、写变化才失效、`Snapshot` 隔离写 + `apply()`。
- `applier.h`：`Applier/ArenaApplier` + 线程局部 `current_applier` + 祖先链 `dispatch_click`；后续已去掉 `NodeArena`，改为 `LayoutNode` 对象树 + `cui::SharedPtr`。
- `composer.h`：`Scope/SlotTable slot_cursor/Recomposer`、`Composition`、`remember`、`LaunchedEffect(key, factory)`。
- `coroutine.h`：`Task` promise_type（初始挂起，由 `EffectScope::add` 设置归属后恢复）、单线程虚拟时钟 `Scheduler`、`Delay/NextFrame`、`EffectScope::cancel()` 通过 `await_resume` 抛 `Cancelled` 收敛。
- `modifier.h` 值链 + `widgets.h` 的 `Text/Button/Row/Column`（节点身份由 `remember<NodeId>` 稳定，重组只更新 payload）。
- `cpp/examples`：gap 复用、有/无读者的重组调度、Snapshot、协程 effect 取消重启、applier 拦截，6 项全过。
- `cpp/README.md` 记录构建、对齐表与边界（无渲染/平台层；无编译器插件）。

## 验证
- `cmake -S cpp -B cpp/build -DCMAKE_BUILD_TYPE=Release && cmake --build cpp/build -j4 && ./cpp/build/compose_tests` 通过。

## 2026-11-24 续：Skia + X11 可用界面 + 严格对齐布局
- 用户补充：底层实现可参考 `/work/study/cui`，架构严格对齐 Jetpack Compose。
- **Skia 来源修正**：本机 Skia 在 `/tools/dev-cache/vcpkg/installed/x64-linux-dynamic`（不是 /work/study/vcpkg）；CMake 从 `$VCPKG_ROOT/installed/x64-linux-dynamic` 追加 `CMAKE_PREFIX_PATH`，`find_package(unofficial-skia CONFIG)` + `unofficial::skia::skia`。
- 新增 `cpp/skia_app/`：`X11Window`（XCB 建窗/事件/WM_DELETE/`xcb_put_image` blit，参考 cui `x11_window.cpp` + `app_window.cpp`）、`SkiaSurface`（`SkSurfaces::Raster(N32Premul)` + `peekPixels`）、`ComposePainter`（fontconfig + `SkFontMgr_New_FontConfig`，参考 cui `Painter`）、`demo/main.cpp`。
- **运行坑**：vcpkg `libskia.so` 把 webp/jpeg/png/harfbuzz/icu/freetype/z 留作外部符号；须 `find_library` 后用 `-Wl,--no-as-needed ... -Wl,--as-needed` 保活，否则运行报 `undefined symbol: WebPPictureImportRGBX` / `jpeg_resync_to_restart`。RPATH 指向 vcpkg lib 目录。
- **协程坑**：`LaunchedEffect` 的 factory 若用 `[&]` 捕获状态，临时 lambda 销毁后引用悬空（gdb: `State<int>::read_unregistered(this=0x0)`）；改为 `[tick]() mutable` 按值捕获可 Clone 的状态句柄。
- 架构对齐：新增 `compose/include/compose/layout.h` + `src/layout.cpp`：`Constraints`（loose/tight/constrain）、`MeasurePolicy`、`Placeable`、`MeasureResult`、两阶段 `measure_node`、`set_text_measurer`（渲染层注入字体度量）。`Node` 增加 `layout_data`。`Row`/`Column` 策略含 padding；`Composition` 根节点默认 `measure_column`。`ComposePainter` 不再自行布局，改为 `measure_node(root, Constraints)` → 递归收集 `Placeable` → 绘制/命中；hit_test 按布局顺序反向确保 z 序。
- 测试：新增 `test_constraints_and_measure_policy`；7 项 C++ 测试全过；`compose_skia_demo` 在 `DISPLAY=:0` 下运行 3 秒无崩溃（timeout 124 正常终止）。
- 后续：控件层拆到 `cpp/compose-material/`（`include/compose_material/widgets.h` + `src/widgets.cpp`，静态库 `compose_material`），新增 `Card/Checkbox/Switch/Slider/TextField` 及 Skia 绘制/点击；`Slider` 用 `onClickPos(local_x, node_width)`。
- 用户要求去掉 `NodeArena` 与 `std::shared_ptr`：改用 `LayoutNode` 对象树；所有权用 `cui::SharedPtr`（复制到 `cpp/compose/include/compose/shared_ptr.h`，非原子计数，单线程），父链用裸指针（树所有权自上而下）；`Scope/EffectScope` 也用 `cui::SharedPtr`，状态读者与 Recomposer dirty 用裸 `Scope*`；`cui::make_shared` 替代 `std::make_shared`。外部 include 路径依赖已移除。全量编译/7 项测试/Skia 演示均通过。
- 边界：`Modifier.Node` 尚未进入 measure/draw 链；Subcompose/Semantics/手势/动画未移植；Skia 层未接 cui 完整 RenderObject/PipelineOwner。

## 2026-11-24 续：C++ examples/story 对齐 Rust 版
- 用户要求「增加 examples 示例，对齐 rust 版」。`cpp/examples/` 原为空目录。
- 先摸清 Rust 真源 `examples/story/pages/mod.rs`：`PageInfo { name, icon, blurb, builder: fn() }` + `PAGES`（113 页）+ `GROUPS`（9 组）；`build_page_body` 用 `emit_keyed_scope(idx as u64)` 隔离各页 remember 槽；`main.rs` 用 `STORY_PAGE=<序号|页名>` 直达。
- C++ 新增 `cpp/examples/story/`：`include/story/pages.h`（PageInfo/IconName/constexpr page()/PAGES/GROUPS + 99 个 builder 声明）、`src/pages.cpp`（113 项注册表逐项对齐 Rust 的 name/icon/blurb/builder，含复用 builder）、`src/shell.cpp`（StoryApp：上一页/下一页/上一组/下一组 + 页头 + build_page_body）、`src/pages_{demos,basics,data,layout,navigation,feedback,advanced,alignment,video}.cpp`、`runner/main.cpp`、`tests/story_test.cpp`。
- **翻页关键决策**：点击回调 `request_page()` 只登记 pending，不立即改树（否则 `dispatch_click` 的 take-put 会对已销毁节点写回 → UAF）；主循环在事件处理后 `StoryApp::rebuild()` → `Composition::set_content()` 新建 Scope/SlotTable，等价 Rust `emit_keyed_scope(idx)` 的 key 隔离。`set_content` 末尾 `Recomposer::clear()` 清掉旧 Scope* dirty。页内状态用 `remember` 创建，每页重建时自然获得全新槽位。
- **Modifier 补齐**：`modifier.h` 原 `then()` 只接 `std::function<void(Visual&)>`，无法组合两条链；新增 `then(Modifier)` 重载（`make_move_iterator` 拼接 elements_），使 `padding().then(background())` 可用。否则 `background`/`size` 只能单独用。
- **测试坑**：`story_test.cpp` 第 3 项先用 `find_node(nav.root(),"下一页")` 找到按钮 → `dispatch_click` → 断言 pending=1/page=0 → `rebuild()` → 断言 page=1 与正文含 "Chat"；第 4 项在 Reactive 页点 `+1` → `recompose()` → 断言 `count = 1`；第 5 项在 pointerInput 页点「点击区域」→ 断言点击次数=1。逐页渲染探针遍历全部 113 页。
- C++ 控件层只有 9 种基础控件，未覆盖的 Rust 页统一用 `detail::placeholder(title, detail)` 明确标注（不用空壳冒充完整实现）；已落地约 28 个可交互页，覆盖 Reactive/Async/CrossFade/Buttons/Toggles/Slider/TextField/Cards/List/Table/Color/Row-Column/Modifier/SizedBox/Tabs/Stepper/Pagination/Rating/Settings/SlotTable/Snapshot/Effects/pointerInput 等。
- CMake 新增 `compose_story` 静态库 + `story_tests` + `compose_story_runner`；Skia 第三方库保活/RPATH 逻辑抽成 `foreach(skia_exe compose_skia_demo compose_story_runner)` 共用。`ctest` 2/2 通过；`compose_story_runner` 在 `DISPLAY=:0` 下 `timeout 4` 返回 124（正常存活无崩溃）。
- 文档：`cpp/README.md` 新增 Story 章节与运行命令，根 `README.md` 的 cpp 条目同步。

## 2026-11-24 续：C++ 控件层扩容（DrawOp + 18 个控件）
- 用户要求「继续实现各种控件」。先读 `cpp/skia_app/src/compose_painter.cpp`：原 `paint_node` 按 `WidgetData::Kind` 硬编码 switch（Text/Button/Card/Checkbox/Switch/Slider/TextField），新增一种控件就要同时改 Kind 枚举与 switch。Rust 侧真源是 `DrawCmd`（定义在 `crates/compose/src/layout.rs:1833`，18 个变体，弧线靠采样路径），故 C++ 对齐为平台无关 `DrawOp`。
- 新增 `cpp/compose/include/compose/draw.h`：`DrawOp { Rect, RoundRect, Circle, Line, Arc, Text }` + 工厂函数；`fill_width`/`fill_height`/`width_ratio` 解决「Divider 撑满父宽」「进度条按比例宽」。核心不依赖 Skia。
- `WidgetData`/`WidgetConfig` 增加 `preferred_width/height` 与 `ops`；`measure_leaf` 优先用显式尺寸（否则回退文本测量）；新增 `Kind::Custom`。`ComposePainter::draw_ops()` 解释执行原语（相对节点矩形坐标 + 可选字号）。
- **Skia 坑**：本机 vcpkg Skia 的 `SkPath` 已移除 `moveTo/lineTo`（迁到 `SkPathBuilder`）；直接用折线 `drawLine` 采样画弧，避免路径 API 版本差异。`PieChart` 是粗描边环扇（非实心扇形），已在 README 记录。
- 新增 `compose_material/widgets_extra.{h,cpp}` 共 18 个控件：Divider/Badge/Avatar/Kbd/Bubble/Skeleton/Chip/Radio/Link/Tab/RatingBar/LinearProgress/CircularProgress/Spinner/BarChart/LineChart/PieChart/EmptyState；`material.h` 统一入口包含它们，`CMakeLists` 把 `widgets_extra.cpp` 加进 `compose_material`。
- Story 对应页由 `placeholder` 换成真实控件：Icon/AvatarGroup/Kbd/Link/ContextMenu/MenuBar/Toggles(+Radio)/Value Input(+LinearProgress+Spinner)/Chips/Bubble/Chart/Tabs/Breadcrumb/Toolbar/Rating/EmptyState/ProgressCircle/Skeleton。Spinner 页（与 Value Input 共用 builder）用 `LaunchedEffect + Delay{60}` 推进相位。
- 测试：`story_test.cpp` 新增 Chip 点击两次（选择/取消）、RatingBar `dispatch_click_at` 按位置评分、Chart/Spinner 的 ops 非空断言。`ctest` 2/2 通过；`compose_story_runner` 在页 11/30/39/75/85/87/71 各跑 2 秒均返回 124（存活无崩溃）。
- 文档：`cpp/README.md` 已实现表补 DrawOp/扩展控件行，更新 Story 页面清单与对齐边界。

## 2026-11-24 续：CMakePresets.json + vcpkg.json 集成
- 用户要求「集成 CMakePresets.json vcpkg.json」。发现 `cpp/` 下已有两文件，但内容是从 cui 抄来的（`name=cui`、`CUI_LIBRARY_TYPE`/`CUI_SANITIZE`/`CUI_BUILD_TESTS` 等变量在本工程完全无效，还带 iOS/wasm overlay ports），属于未适配的历史残留，直接重写。
- `cpp/vcpkg.json` 改为 `name=compose-cpp`，依赖按平台收敛：Linux 才装 `skia` / `libxcb` / `fontconfig`。Skia 默认 feature 在 Linux 已含 fontconfig/freetype/harfbuzz/icu/jpeg/png/webp，不再像 cui 那样额外加 vulkan/graphite（本机 port 元数据确认过）。
- `cpp/CMakePresets.json` version 6：`base`(Ninja, binaryDir=`build-${presetName}`) → `vcpkg-linux/windows/macos` 隐藏基类（注入 `CMAKE_TOOLCHAIN_FILE=$env{VCPKG_ROOT}/scripts/buildsystems/vcpkg.cmake`、`VCPKG_TARGET_TRIPLET`、`VCPKG_INSTALLED_DIR=$env{VCPKG_ROOT}/installed`、`VCPKG_MANIFEST_INSTALL=OFF`）→ 各平台 debug/release；另有 `x64-linux-asan`（真加 `-fsanitize=address,undefined`，替换原来无效的 `CUI_SANITIZE`）与 `x64-linux-manifest`（`VCPKG_MANIFEST_INSTALL=ON`，安装到 build 目录）。build/test preset 一一对应。
- `cpp/CMakeLists.txt` 去掉硬编码 `$VCPKG_ROOT/installed/x64-linux-dynamic`：计算 `COMPOSE_VCPKG_LIB_DIR`（优先 `VCPKG_INSTALLED_DIR/${VCPKG_TARGET_TRIPLET}/lib`，再 `$VCPKG_ROOT/installed/...`），用于 Skia 第三方库 `find_library` 与 BUILD_RPATH；未用 toolchain 时才把其父目录加进 `CMAKE_PREFIX_PATH`。无 preset 老命令仍可用。
- **关键取舍**：默认 preset 用 `VCPKG_MANIFEST_INSTALL=OFF` 复用本机已装 Skia（避免 manifest install 重新下载/现编几十分钟）；`x64-linux-manifest` 留给全新机器。
- 验证：`cmake --list-presets` 识别 4 个 Linux configure preset；`cmake --preset x64-linux-release` 成功且 `compose_skia_demo / compose_story_runner: enabled`；`cmake --build --preset x64-linux-release -j4` + `ctest --preset` 2/2 通过；`build-x64-linux-release/compose_story_runner` 跑 3 秒返回 124（存活无崩溃，RPATH 正确）；老 `cmake -S cpp -B cpp/build` 仍能配置。
- 文档：`cpp/README.md` 构建章节改为 preset 优先 + manifest 说明 + 无 preset 回退，Skia/Story 运行命令同步到 `build-x64-linux-release`。

## 2026-11-24 续：skia_app 归入 compose
- 用户指出「skia_app 应当集成到 compose 的吧」。此前 Skia 适配层是 `cpp/skia_app/` 顶层目录（与 compose / compose-material 平级），但命名空间已是 `compose::skia_app`，目录与归属不一致。
- 物理迁移：`cpp/skia_app/include/skia_app/*.h` → `cpp/compose/include/compose/render`；`cpp/skia_app/src/*.cpp` → `cpp/compose/src/render`；`cpp/skia_app/demo/main.cpp` → `cpp/examples/skia_demo/main.cpp`；删掉空的 `cpp/skia_app/`。
- include 统一为 `<compose/skia_app/...>`（源码里用 `"compose/skia_app/..."`）：3 个适配层实现 + skia_demo + story/runner 共 5 处。命名空间保持 `compose::skia_app`。
- **仍保留独立 target `compose_skia`**（不放进 `compose_runtime`）：这样无头核心不依赖 Skia/XCB/fontconfig，找不到依赖时自动退化为纯核心构建。CMake 中源码路径改为 `compose/src/skia_app/`，PUBLIC include 改为 `compose/include`（runtime 也已 PUBLIC 暴露，此处显式写出），demo target 指向 `examples/skia_demo/main.cpp`。
- 验证：`cmake --preset x64-linux-release` 重配 + 增量构建 8 个目标成功；`ctest --preset` 2/2 通过；`compose_skia_demo` 与 `compose_story_runner` 各跑 2 秒均返回 124（存活无崩溃）。`cpp/README.md` 对齐表与路径已同步。

## 2026-11-24 续：platform 层 + 取消独立 compose_skia
- 用户要求「实现各个平台 /work/study/cui/cui_core/include/cui/platform」。调研确认 cui 的真实结构是 `PlatformEvent → PlatformWindow → Application/AppWindow + services/*`，没有 `platform.h`；工厂是编译期宏选择后端，服务通道是运行时注入。
- C++ 新增 `cpp/compose/include/compose/platform/`：`event.h`（PlatformEvent/EventType/KeyCode/滚轮 56px 单位）、`platform_window.h`（纯虚核心 + IME/剪贴板/自呈现默认虚接口 + NativeHandles void* 聚合）、`platform_factory.h`（`createPlatformWindow()` 按宏选后端，Wayland 默认不接入）、`platform_info.h`，以及 `services/{method_channel,event_channel,app_lifecycle,file_dialog}.h`。
- 实现 `cpp/compose/src/platform/`：`x11_window.cpp`（完整 XCB：事件翻译/键盘映射用 `xcb_get_keyboard_mapping` 免额外依赖/剪贴板进程内/presentPixels=xcb_put_image/合成 invalidate+wake 事件）；`win32_window.cpp`（WndProc/GDI StretchDIBits/系统剪贴板/DPI）；`cocoa_window.mm`（NSWindow+NSView/CALayer）；`android_window.cpp`（ANativeWindow_lock 整帧上屏 + AInputEvent）；`ios_window.mm`、`ohos_window.cpp`、`web_window.cpp`（宿主驱动队列 + 各自 presentPixels 契约）、`wayland_window.cpp`（接口骨架）。services 实现 4 个 cpp，Linux 文件对话框走 zenity→kdialog，其余可注入 backend。
- **用户中途纠正「无需独立 compose_skia，只作为 compose 库一部分」**：撤销此前保留独立库的决定。CMake 重排：`compose_runtime` 现在包含 core + services + 当前平台后端 + `skia_app/{skia_surface,compose_painter}.cpp`，`compose_material` 仍独立（PRIVATE include 供 painter 使用），彻底删除 `compose_skia` target 与可执行同名混淆——GUI demo 改名为 `compose_demo`。找不到 Skia/fontconfig/平台后端时 `COMPOSE_GUI_AVAILABLE=FALSE`，只构建库与测试。
- 删除旧 `compose/include/compose/skia_app/x11_window.h` 与 `compose/src/skia_app/x11_window.cpp`；两个示例改用 `createPlatformWindow()` + `PlatformEvent` + `window->presentPixels(...)`，不再直接 include XCB。
- 新增 `tests/platform_test.cpp`（工厂非空、事件默认、Method/EventChannel 回环、生命周期观察者、文件对话框注入），ctest 从 2 项变 3 项。
- **坑**：web_window.cpp 的 JS 桥接 extern "C" 不能访问匿名namespace 单例，改为命名空间的 `web_singleton()`；sed 批量替换曾把函数名误改成 `web_web_singleton`，已修正。`file_dialog.cpp` 用 `unique_ptr<FILE, decltype(&pclose)>` 触发 `-Wignored-attributes`，改成自定义 `PipeCloser` deleter。
- 验证：`cmake --preset x64-linux-release` 显示 `GUI targets: enabled (platform=linux, Skia/XCB/fontconfig)`；39 个构建目标全过；`ctest --preset` 3/3 通过；`compose_demo` 与 `compose_story_runner` 各跑 2 秒返回 124（存活无崩溃）。
- 文档：`cpp/README.md` 新增平台层/库结构章节，根 `README.md` 与 `vcpkg.json` 描述同步。

## 2026-11-25 续：cpp 液态玻璃观感修正（壁纸配色 / 同心环 / 裁切）
- 用户反馈「液态玻璃背景不好看」。**根因不在玻璃渲染**（`ComposePainter::paint_glass` 用 `SkImageFilters::Blur` 做真 backdrop blur 是对的），而在它背后那层：cpp `ColorScheme::light()` 的 secondary=0x625B71 / tertiary=0x7D5260 是 M3 低饱和灰调，被 Rust 版壁纸直接拿来铺底 ⇒ 模糊出来是一团灰紫雾，看不出任何色彩层次。
- 改法（`cpp/examples/story/src/shell.cpp::glass_wallpaper` + Rust 同源）：①固定高饱和桌面色板（紫罗兰 0x5B4BFF / 品红 0xFF4FA3 / 天青 0x18C2FF / 薄荷 0x2BE3A6），各自与 `cs.primary` 混 0.22 ⇒ 换 Ocean/Forest/Rose 色板时壁纸仍跟着变调；②每枚光斑由单个大实心圆改 `kGlassBlobRINGS = 12` 层**同心环**近似径向衰减（第 i 层增量 `a = (want − acc)/(1 − acc)`，`want = peak·(1 − smoothstep(f))` ⇒ 中心累计 α 恰等于峰值、边缘平滑归零），消掉 dock 缝隙里露出的硬边圆弧；③暗色主题 `gain 1.35` + 混白 0.28 提亮（玻璃 tint 本身 40% 深灰，壁纸再暗就糊成墨）；④`glass_spec()` tint 减淡一档（暗 0.50→0.40、浅 0.40→0.34），毛玻璃要「看得到背景」才像玻璃。
- **DrawOp 无渐变无裁切**：本机 vcpkg skia 快照不导出 `SkGradientShader`（cpp/README 已记录），Rust `DrawCmd` 也只有线性渐变 ⇒ 同心环是与两侧能力等价的唯一柔和衰减手段。裁切则必须绕：cpp `draw_ops` 完全不裁，而光斑半径 `max(w,h)·0.45` 远大于节点矩形，沿窗口四周 8px 留白画出一圈「彩色画框」；`paint_node` 的 `scrollable` 只 clip **孩子**（在自身 ops 之后），故 cpp 用「`Kind::Box + scrollable=true` 裁切壳包 Custom 绘制片」两层。Rust 有真通道 `Modifier::clip()` → `DrawCmd::ClipPush` → valo `clip_rrect`，直接在 Canvas 上加 `.clip(0.0)` 即可。
- 壁纸节点数仍恒定（关态 draw_fn 返回空 ops），不破坏 cpp 单 root scope 的「节点数恒定才能 bump 增量重组」不变式。
- 验证回路：`build-x64-linux-debug/zz_glass.cpp` 离屏出 4 张 PNG（明/暗 × 开/关）真目视。**坑**：cpp `SetLiquidGlass` 只改裸 bool、不置脏任何作用域（Rust 走 MutableState），构造 StoryApp 后单独 `recompose()` 是空转，必须 `app.do_menu_action(2, 5|6)` 经 `bump()` 才会重跑 `emit_shell` 重读材质。
- 测试：cpp `story_test.cpp` 9e (E) 块升级为「下降到裁切 Box + 4×11 环 + 半径单调递减 + src-over 累计 α 收敛到峰值 + 最外环半径 = 540·尺寸比·(1−1/12) + 圆心分数位」；Rust 新增 `crates/compose-material/tests/glass_wallpaper.rs`（3 项：柔和同心环+裁切、暗色提亮、tint 保持半透明）+ `SetLiquidGlass` 复位。**断言教训**：单盘增量并非越靠外越小（分母 `1 − acc` 在收缩，越靠内反而越大），可单调的是「累计不透明度」，拿单层增量做衰减断言会假失败。
- 验证：`ctest --preset x64-linux-debug` 3/3 全绿（compose_tests / platform_tests / story_tests）；`cargo test -p compose -p compose-material --tests` rc=0 全绿；`cargo check --workspace --tests` 仅历史告警。GUI 最终观感待用户在 CLion 目视验收。

## 2026-11-25 续：cpp 键盘焦点路由（List 页选中后方向键误切侧栏）
- 用户报 bug：「List 页面，选中 List item，然后上下箭头，无法在 list item 间切换，却是在 sidebar 上切换」。三层根因：① **cpp 引擎根本没有键盘通道**（`LayoutNode` 无 onKey、无焦点跟踪、无 `dispatch_key`）；② material 的 `ListItems` / story 的 `page_list()` 是无焦点容器（Rust 侧 `List/ListView/GridView/TreeView/Sidebar` 全走 `focusable_full`）；③ story 用 app 级裸 bool `sidebar_focus_` 门控 `handle_key`，只有点侧栏条目才置真 ⇒ 点页面内容后方向键永远在切侧栏。
- 引擎层（`compose/{applier.h,applier.cpp}`）：`LayoutNode` 加 `onKey` + `focusToken`，并继承 `EnableSharedFromThis`（焦点表要持弱引用）。焦点身份 = **uint64 token** 而非节点指针：thread_local `vector<FocusEntry{token, WeakPtr<LayoutNode>}>`，`register_focusable` 登记时顺带 `erase_if(expired)` 剪枝；聚焦 token 存**响应式** `MutableState<uint64_t>`（`focused_token()` 用 `get()` 注册读者 ⇒ 聚焦环自动定向重组，宿主不再需要 bool + rebuild_shell）。8 个 API：`next_focus_token/register_focusable/focused_token/is_focused/set_focus/focus_node/clear_focus/dispatch_key`。`dispatch_click` 改为 Rust 的**两条独立链各走到底**（先整链找最近 onKey 祖先定焦点，再整链找 onClick 消费），且**先定焦点再执行处理器**；点不到任何 onKey 容器时 `clear_focus()`（与 Rust 一致，故点普通内容后方向键确实不响应）。
- material 层：`WidgetConfig` 末尾追加 `onKey/focusToken`（位置式聚合初始化，新字段只能进尾部）；`create_widget` 接线；新增三个 helper `attach_key_nav(node,on_key,token)` / `nav_index(key,cur,len)` / `list_nav_on_key(sel,len,key)`，逐项对齐 Rust `mod.rs:140`（len==0→0；Up −1 saturating / Down +1 clamp / PageUp·Down ±8 / Home 0 / End last；`list_nav_on_key` 用 `read_unregistered` 取活值再 `set`，连按不丢步）。接入点：`ListItems`、`TreeView`、`GridView`（二维）、`Sidebar`（`SidebarSpec` 加 `on_key + focus_token`）。**未接** `LazyColumn`（Rust 侧同样不走 focusable_full）。
- story 层：删 `sidebar_focus_` bool，改 `std::uint64_t sidebar_token_ = compose::next_focus_token()`（声明必须早于 `dock_/composition_`），`sidebar_focus()` 由 `compose::is_focused(sidebar_token_)` 派生；侧栏是应用骨架，token 由宿主长期持有并每次组合传回 ⇒ **跨 `set_content` 整树换代保焦点**（对齐 Rust「焦点记 owner_scope 而非 NodeId」）；页内列表 token 由节点自分配 ⇒ 换页即失焦（符合预期）。`set_sidebar_focus` 走引擎且**不再 rebuild_shell**；runner 的 KeyDown 兜底由 `app.handle_key(key)` 改 `compose::dispatch_key(key)`。
- **关键陷阱（勿改回去）**：`create_widget` 在 `config.onKey` 为空时**绝不能清 `node->focusToken`**。「先发射容器、后 `attach_key_nav`」是本工程固定顺序，清身份 ⇒ 每轮组合换一次 token ⇒ 一次重组即失焦。`attach_key_nav` 传空 on_key 同理只解绑、保留身份。
- 验证：`ninja` 39/39 绿；`ctest --preset x64-linux-debug` 3/3 全绿。新增 story 测试 **12b** 三段钉住本次 bug：(a) List 页点行 → `!sidebar_focus()` + `dispatch_key(Down)` 返回真 + 行高亮从「项目文档」移到「稍后处理」且 `page()==31` 不变（高亮判据 = ListItem ops 里 `secondary_container` 填充 RoundRect）；(b) ListView 页用可见文本「选中 = N」逐项验 nav_index 语义（End/Home/PageUp·Down ±8/越界夹取）；(c) 点侧栏条目 → 焦点在侧栏 → `rebuild()` 换代后 `dispatch_key(Down)` 仍切页（token 保焦点）。既有测试 12（`set_sidebar_focus(true)` + `handle_key`）未改仍绿，证明新写法向后兼容。GUI 手感待用户在 CLion 目视验收。

## 2026-11-25 续：cpp SearchableList 内容越界（卡片须裁切 + 可滚）
- 用户报 bug：「SearchableList 内容越界了，应在 card 内，card 应可滚动」+ 截图（Spam/Trash/Starred 画到描边框外）。根因：`pages_data.cpp::page_searchable_list()` 用**普通 `Column` + `height(240)`** 装 8×56=448px 行内容 —— painter 只对 `layout.scrollable` 的节点 `save/clipRect/restore`（`render/painter.cpp:349`），普通 Column 的定高只收口容器**自身**输出尺寸（`apply_fixed_size`），孩子照画 ⇒ 越界，且滚轮找不到可滚祖先。Rust 同源是 `VerticalScroll(fill_max_width().height(220).border(1, outline_variant, 12))`，本来就该是滚动容器。
- 改法（`pages_data.cpp`）：外层 `Column` → **`ScrollColumn`**；发射行数由 `kRows=8` 提到 **`kAll.size()=12`**（否则过滤前 12 项只有 8 项可见、滚到底也看不到后 4 项）；每行外套 **`SizedClip(size_fn, ...)`** 把不匹配行的**自身高度收成 0** ⇒ 内容全长 = 命中行数 × 56，既不留空白行也不产生空白滚动区；视口高取组合期可算的 `min(240, 命中行数 × 56)`（命中 ≤4 行时框刚好包住内容不出条，命中多时 240 视口 + 真溢出，0 命中时自然高 0 —— 描边随 `r.isEmpty()` 一并免绘，见 painter 的「零面积⇒无盒子装饰」）。
- **两处 C++ 侧硬约束（决定上面这套写法，勿试图绕过）**：① `Modifier::height(0.0f)` 是**空操作** —— `apply_fixed_size` 用 `if (data->visual.height > 0.0f)` 守卫，只有 `size_fn`（判据 `>= 0.0f`，负值才是「沿用自然值」）能表达 0 高；② 单 root Scope 扁平槽位流 ⇒ 发射数量必须恒定，所以不能像 Rust 那样只发射 `hits`。CommandPalette 的「`preferred_height = live ? rowH : 0`」惯用法在本管线**并不真的归零**：`measure_leaf` 在 `preferred_height<=0` 时回落到 `measure_text("")` 的高（painter 测量器恒返 `size*1.55≈26`），故过滤列表要用 `SizedClip`+`size_fn`。
- 顺带修两处引擎/宿主缺口（本次写法的必要前提）：
  - `compose/src/layout.cpp measure_node`：scrollable 节点测完后把 `scroll_offset` 按**新布局**夹到 `[0, content-viewport]`（主轴跟 `scroll_horizontal`）。`create_widget` 故意跨重组保留偏移，过滤/折叠使内容变短后陈旧偏移会把可见项推出视口 = 空白卡片。
  - `examples/story/runner/main.cpp` 的 MouseWheel：由「取最近可滚祖先滚一次」改为**沿父链逐个试、第一个真能量得动的生效**。纯裁切壳（`SizedClip`）也带 `scrollable` 但无溢出，只看最近一个会让滚轮在外层列表里石沉大海（嵌套滚动普遍受影响）。
- 测试：story 块 34 新增 **34.e/34.f**（行壳 `scrollable && !scrollbar`、卡片 `scrollable && scrollbar`、视口 240 且内容全长 672、末行绝对矩形落在卡片底边之下（= 被裁切而非画框外）、`scroll_by` 夹到 `content-viewport`、过滤到 1 项后内容全长塌成 56 且陈旧偏移被夹回 0、无溢出不画条、`count_nodes` 恒定）。`ninja` 绿 + `ctest --preset x64-linux-debug` 3/3 全绿（compose_tests / platform_tests / story_tests）。GUI 观感待用户在 CLion 目视验收。
- 同类未改（非本次报障点，待确认后再动）：`pages_demos.cpp::page_search_bar` 同样固定 6 行但**无定高** ⇒ 不越界，只是过滤后仍占空白高度；`ListItem` 的选中底按组合期文本宽（`o.width` 未设）而非卡片宽，故高亮比框窄。

## 2026-11-26 续：video 底部叠放播放控件（自动隐藏 + 自定义控件），Rust / cpp 双端对齐
- 需求：「video 底部可内置播放控件（自动隐藏），也可设自定义的；请对齐 rust 版本」，规格真源 = `/work/study/cui/rust/examples`（`rui_extra/src/video.rs::VideoPlayerWidget` + `examples/src/pages/media.rs::page_video()`：底部 Positioned + auto_hide 2s + MouseRegion + AnimatedOpacity + IgnorePointer + `controls_builder`）。范围经用户确认 = Rust + cpp 双端都补。
- Rust（`crates/compose-material/src/widgets/video.rs`）：新增 `type VideoControlsBuilder = Rc<dyn Fn(bool)>`、`VideoControlsOverlay(controller, modifier)`（半透深底内置条）、`VideoPlayerWidget(controller, modifier, auto_hide, Option<VideoControlsBuilder>)`。`collapsed = remember(mutableStateOf(false))`，`show = !ready || !auto_hide || !collapsed`；`hoverable` 包舞台（进入 ⇒ seq+1 + collapsed=false；移出且 auto_hide ⇒ collapsed=true），底层 `Clickable(Video)`（点画面 toggle_play + 重新显形），上层 `Box(fill_max_size().alpha(animate_float_as_state(show?1.0,0.0, Tween(STAGE_FADE_SECS))) (+show?():.no_hit()), BottomStart)` 放 builder 或内置条；收起靠 `feedback::auto_hide_after(seq, STAGE_HIDE_DELAY_SECS=2.0, armed=show, …)`（**armed=show** ⇒ 只要处于显形态就挂着倒计时，不需先有鼠标事件）；`controller.session_id()` 变化 ⇒ collapsed 复位。常量 `STAGE_HIDE_DELAY_SECS=2.0 / STAGE_FADE_SECS=0.2`。story 页（`examples/story/pages/video.rs`）：舞台 `fill_max_width().height(280).background(黑,8)` + 「使用自定义控件 / 切回内置控件」开关（自定义 builder = 单条蓝底画布、13px 文本），页底独立的 播放/暂停·关闭 按钮与 Slider 删除。回归 `crates/compose-material/tests/video_player_overlay.rs` 5 项。
- cpp 库层：新建 `compose-material/include/compose_material/widgets_video.h` + `src/widgets_video.cpp`（已加入 `cpp/CMakeLists.txt` 的 compose_material 源列表）：`VideoControls(VideoStatePtr, Modifier={}, bool overlay=false)` / `VideoPlayer(VideoStatePtr, Modifier={}, bool auto_hide=true, std::function<void(bool)> controls_builder={})`。`VideoState` **末尾**追加 `atomic<float> volume` + `uint64_t session` 与 5 个动作 helper（`togglePlay/seekTo/setVolume/openSource/closeSource`，后两者顺带 `++session`）—— 控件只改这些字段 / 往 `cmds` 无锁队列发命令，解码与音频输出仍归宿主 worker；`pumpAudio` 在 `pa_simple_write` 前按 `volume` 做 16.8 定点缩放。
- cpp 实现要点（引擎能力差异，别当 bug 查）：
  - **节点拓扑恒定**：舞台(`video_stage`) / 画面 / 控制条容器(`video_bar`) 常驻发射，隐藏 = `layer_fn` 的 alpha 0 + 组合期 `visual.no_hit` ⇒ 点击穿透而非删节点，故可增量重组；而页面级「内置 ↔ 自定义」两条子树节点数不同 ⇒ 必须整体重建。为此新增 `story::request_shell_structural_rebuild()`（shell.h/.cpp，= `StoryApp::rebuild_shell()` = set_content），与既有 `request_shell_rebuild()`（bump，仅增量重组、不删旧节点）分工写死。
  - 计时取 `compose::Scheduler::instance().now_ms()` 逻辑钟（宿主每圈按真实耗时 advance ⇒ 生产仍是墙钟，但无头测试能直接推进）；`compose::animation::clock_nanos()` 是 steady_clock，测试推不动。
  - 收起倒计时挂在控制条的 `size_fn`（测量期）：到点 `collapsed.set(true)`，未到点 `request_anim_frame_after(剩余秒)` 定点唤醒；淡入淡出在 `layer_fn` 里按 easeOutCubic 求值，未播完 `request_anim_frames()`。
  - cpp 无子树 MaterialTheme ⇒ 叠放条用显式配色（`0x8C000000` 底 + 白字）；hover 只在命中节点变化时触发 ⇒「移动即续期」退化为「进入 / 点击时重新计时」。
  - `stage_cfg.box_align = BottomStart`：画面 `fill_max_size()` 铺满、控制条按**自然高**贴底边 —— 若让控制条容器也铺满舞台，它会吞掉画面上半部的点击。
- 测试：story 块 42 断言随页面改造更新（不再找「播放 / 暂停」「关闭」，改找 `video_stage`/`video_bar` + 内置条的「播放/暂停」「音量+」）；新增**块 57** 八段（叠放几何 280 + 贴底 + 画面铺满、未就绪常显、就绪后 2 秒自动收起且 `count_nodes` 恒定、`ComposePainter::hit_test` 真实命中链路验穿透、点画面重新显形、hover 移出即收起 / 移入即显形、自定义 builder 整体替换且自带交互、切页撤离）。
- **两个测试陷阱（本窗口踩到）**：① `rebuild_shell()` = set_content 会让 `create_widget` 里 `remember<NodePtr>` 的槽表换代 ⇒ **旧 NodePtr 指向已废弃的树**，读它的 `visual` 只会得到陈旧值（表现为「逻辑明明生效、断言却失败」），任何跨结构重建的断言都必须重新 `find_node` 定位；② `semantics::Tree::hit_test` 的 `no_hit` **不继承祖先**（`semantics.cpp:296` 只取本节点），要验「no_hit 波及整棵子树」必须走 `ComposePainter::hit_test`（`painter.cpp:87` 有 `blocked` 累加）。
- 验证：`cargo test -p compose -p compose-material --tests` 全绿；`cargo check -p examples --examples` 通过；`cmake --build --preset x64-linux-debug` 绿；`ctest --preset x64-linux-debug` 3/3 全绿。GUI 观感（悬停显隐、淡入淡出、点画面切播放、音量）待用户在 CLion 目视验收。

## 2026-11-26 续：鼠标指针卡在「青色十字」（X11 glyph cursor 形参错位）
- 用户报障：指针突然变成**青色十字**且**不自愈**。该形状不属于引擎 `Cursor` 枚举（Default/Grabbing/Move/ResizeEW/NS/NWSE/NESW），故排除业务层设错形状，直取 X11 后端。
- 根因（`compose/src/platform/x11_window.cpp::glyph_cursor`，随 `docking` 提交 f1b7d7c 引入）：`xcb_create_glyph_cursor` 的形参顺序是 `(cid, source_font, mask_font, source_char, mask_char, fore_rgb, back_rgb)`，而调用把字形号 `g / g+1` 写在了**最后两个槽**（back_green / back_blue），前两个 char 槽填了 `0, 0`。cursor 字体的 **0 号字形 = `XC_X_cursor`**（X server 内置十字/X 光标），同时前景色被挤成 `(0, 0xffff, 0xffff)` = **青色** ⇒ 任何 `Cursor` 取值都渲染成同一个青色十字，与形状枚举完全无关，所以「怎么动都不变、一直卡着」。修复 = 字形号归位到 `source_char/mask_char`，颜色保持前黑 `(0,0,0)` / 后白 `(0xffff,0xffff,0xffff)`（对齐 Xlib `XmuCreateCursorFromName`）。
- 触发时机与 video 页改动**无因果**：全仓 `set_cursor` 写入点只在 `compose-material/src/docking_view.cpp`（分栏条 hover 复位 L548/556/686、标签拖拽 Grabbing/Default L580/588/606、浮窗标题 Move L1820、浮窗边缘 `edge_cursor` L1677/1767/1719/1723/1852/1895），首次把指针划到 dock 分栏缝/标签/浮窗边才第一次真正 `xcb_change_window_attributes(XCB_CW_CURSOR)`（`applied_cursor_=0xFF` 使首调必发），此前窗口一直沿用 WM 主题光标，故观感上是「突然」变。`Cursor::Default` 走 `XC_left_ptr`（68），修好后即经典箭头；若想要主题箭头可让 Default 下发 `XCB_NONE` 继承父窗（未改，避免动行为）。
- 验证：`cmake --build --preset x64-linux-debug` 绿（x11_window.cpp 重编）、`ctest --preset x64-linux-debug` 3/3 全绿。光标视觉效果无法无头断言，待用户在 CLion 目视确认（划过分栏条应变左右双箭头、离开应恢复箭头）。







