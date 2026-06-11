# 2026-10-30

## RustCompose 初始化（Jetpack Compose 架构 GUI 库）
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
