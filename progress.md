# 进度日志

## 2026-10-19：目录导航与树展开分离
- 工作区干净，阶段 24–26 已提交。用户要求仅箭头展开左树，点击名称只导航右侧；已展开节点点击名称仍收起。
- 定位到 navigate 会展开目标及所有祖先，名称处理仅在当前目录时收起。将移除导航的展开副作用，并补充当前/非当前节点及异步请求回归。
- 已移除导航的目标/祖先自动展开；名称点击统一收起该节点，仅不同路径或错误状态时读取右侧。缓存失效后只补读已展开祖先，避免额外加载折叠目录。
- 前端构建、夹具类型检查及后端 19 项测试通过；25 组浏览器模拟回归全部通过，覆盖名称不展开、当前/非当前展开节点收起、仅箭头重开、深层路径/刷新不展开、延迟请求不重开以及原有文件操作。真实鼠标点击 sdcard 名称已确认右侧显示内容但左树仍折叠。
- 模拟设备截图保存为 visualization 目录 `device-files-name-navigation.png`；未读取或改动真实设备。独立 EXE 构建中。
- 独立 EXE 构建成功（release 1m23s），位于 `src-tauri/target/release/serial_terminal.exe`；仅 Windows 链接器已有中文提示 warning。diff 检查通过，本轮模拟页/服务器已关闭，阶段 27 完成，代码尚未提交。

## 2026-10-17：提交文件浏览器完善
- 用户确认提交阶段 24–26 的目录性能优化、设备端刷盘、主题菜单、隐藏目录、目录名称折叠及稳定列宽。
- 最新要求已移除 .，只保留 .. 返回上级；同步修正虚拟列表偏移、计数、使用说明与测试。父目录唯一导航行、5000 项尾部索引与导航不可修改的针对性测试通过，最新独立 EXE 已生成。
- 提交范围仅为本轮相关代码、隔离测试、只读性能诊断脚本及项目记录，不包含构建产物，也不推送远端。

## 阶段 26：点目录导航、名称折叠与稳定列宽
- 保留阶段 24/25 未提交修改。用户要求显示 . / ..、目录名称重复点击折叠，并修复大目录滚动时元数据列移动。
- find 本身已包括真实点开头目录，但不返回 . / ..；在前端增加仅用于导航的两行，继续禁止对其下载、重命名或删除，不放宽后端名称保护。
- 列抖动原因是 table-layout:auto 与虚拟 tbody 更替：可见文件名长度变化会重新计算列宽。改为 colgroup 固定元数据宽度和 fixed 布局，保留大目录虚拟化。
- 实现安全导航行、独立导航选中状态（根目录 . 与 .. 不混淆）、名称重复点击折叠和固定列宽；前端构建/测试类型检查与后端 19 项测试通过。准备使用含长中文文件名/不同大小的 5000 项模拟目录验证滚动列位置。
- 已补充真实隐藏目录树/列表显示、. / .. 导航与不可修改、根目录选中隔离、目录名称首次进入/再次收起/第三次展开，以及大目录顶部/中间/底部的表头坐标一致性测试。README 已解释导航和固定列宽行为；浏览器隔离回归进行中。
- 首次回归已有 6 组通过（含新导航/名称折叠），旧刷新等待条件在局部面板尚未更新时读取旧的 enabled 状态，随后名称点击因实际仍 loading 被忽略；测试改为同时等待面板 DOM 已更新和按钮可用，而非只看旧状态。
- 最终 24 组浏览器隔离回归通过。5000 项长短中文文件名/不同大小的目录在顶部、中间、底部和返回顶部时，各表头横坐标差异均小于 0.5px；虚拟列表总行高包含两行导航项，尾部操作正确。导航/隐藏目录/名称折叠/菜单与已有文件管理回归均通过，独立 EXE 构建中。
- 视觉复查后进一步约束放大箭头的 inline box 与导航提示的垂直对齐，确保文件夹行也严格保持 32px，避免混合文件/目录时虚拟滚动累计偏差；大目录夹具加入混合目录行并断言每行高度，继续回归与重打包。
- 混合行严格测试初次失败是首行 32.5px 的折叠表头边框半像素，而非所有行撑高；只读 DOM 测量确认其余目录/文件/导航行均为 32px。测试允许首行半像素边界，保留总滚动高度与列位置检查，不再修改生产实现。
- 最终夹具还有一处旧的“等待 30ms”在树中新建后不足以等待祖先目录重建，菜单目标节点暂不存在；改为等待提交后的面板更新、按钮恢复和目标树节点同时出现，不再依赖固定延迟。本轮生产代码已构建，测试修正不进入 EXE。
- 最后全套 24 组隔离回归通过，包含混合目录行高、隐藏目录、导航项安全、名称折叠以及顶部/中间/底部表头稳定性。实际鼠标验证目录名称首次打开/二次收起且右侧路径不变；最新截图位于 visualization 目录 `device-files-navigation-stable-columns.png`（模拟设备）。
- 最新独立 EXE 重新构建成功：4,842,496 字节，2026-10-17 18:19:13，位于 `src-tauri/target/release/serial_terminal.exe`；前端生产构建、夹具类型检查、后端 19 项测试、fmt 与 diff 检查均通过。最后修改仅为测试等待条件，生产产物已与源码一致。
- 本轮测试页面/服务器已关闭，真实设备和用户运行中的程序保持不变，代码尚未提交。阶段 26 完成。

## 阶段 25：文件浏览器图标与主题右键菜单
- 保留阶段 24 未提交修改，用户要求放大文件夹箭头并让文件菜单复用终端样式、包含刷新及所有顶部操作。
- 终端已有 terminal-context-menu/item/icon CSS，文件页尚未实现自绘菜单；采用 body 浮层避免终端面板 contain/overflow 裁剪，不因右键打开重建应用。
- 已将目录树/列表文件夹箭头放大至 20px；菜单包含全部 10 项顶部操作，复用现有操作分发和统一禁用判定。列表右键选中、空白取消选择，目录树上下文绑定右键目录；Escape/外部点击/滚动/失焦/缩放/重绘关闭浮层，支持键盘上下/Home/End。
- 前端生产构建、测试夹具类型检查及后端 19 项测试通过；模拟设备浏览器回归进行中，未使用真实设备或文件。
- 首轮 22 组浏览器隔离交互全部通过，进一步补充菜单 Home/End、原生目录选择取消、菜单上传覆盖与下载确认路由验证；最终 EXE 构建进行中。
- 补充用例后测试有一次在写操作使缓存失效、目录树尚未重建时查找目录按钮失败；明确等待目录树节点重新加载后再操作，避免把列表已更新误当作树也已更新。
- 最终 22 组隔离交互回归通过，已包含菜单 Home/End、目录选择取消、上传覆盖与下载确认、刷盘失败重试。实际鼠标右键文件夹能正确选中并显示完整菜单，截图为 visualization 目录 `device-files-context-menu.png`，共享终端 CSS 视觉复核通过。
- 独立 EXE 构建成功（4,841,984 字节，2026-10-17 18:03:42），产物位于 `src-tauri/target/release/serial_terminal.exe`。前端构建、测试类型检查、后端 19 项测试、fmt 与 diff 检查通过；本轮仅修改前端和记录，没有新后端功能变更。
- 测试页面已关闭、测试服务器已停止；保留用户现有程序和真实设备，代码未提交，阶段 25 完成。

## 阶段 24：目录浏览性能与设备刷盘
- 工作区开始时干净。用户要求优化打开目录、增加设备端刷盘，以及同排精简 ROOT/浏览按钮。
- 已确认 sync 的目标是 Android 设备缓存，不是电脑备份；仅计划只读性能检查，不在真实设备执行写入、删除或刷盘。
- 实机读取基准：旧脚本 /system/bin 448 项约 10.2 秒；改用原生 find + 批量 stat 后 134–163ms。第一次仅批量 stat 仍有逐文件 shell test，约 5.5 秒，已继续去除。
- 已实现局部文件区 rAF 合并重绘、虚拟列表、目录请求合并与 30 秒缓存，刷新/写操作/连接变化使缓存失效；刷盘新增后台 sync（120 秒独立窗口）。
- 中间前端编译因 cache 常量尚未接入暂报未使用，接入共享缓存后继续完整验证；一次缓存补丁未匹配 catch 行，已按实际代码位置修正。
- 补充 STFILES3 多批次/448 项/空目录/中文与符号/符号链接目录/CRLF/截断安全测试和独立命令超时测试，19 项后端测试通过。前端和测试夹具类型检查、cargo check 通过。
- 新增目录缓存/强制刷新/请求合并/保留侧栏 DOM/5000 项视口渲染/尾部操作/切目录重置滚动/同步重复点击、设备绑定、切标签、失败重试用例。旧测试等待旧按钮状态会误判操作完成，改为等待真实列表结果；最终 17 组浏览器隔离回归通过。
- 真实只读输出协议复核发现 Android 返回 CRLF，Rust lines 原本已支持，诊断脚本已修正并增加 CRLF 断言；复测 /system/bin 448 项 145–155ms，根目录 32 项 132–140ms，无真实设备写入。截图复查还发现旧 ROOT 独立外边距残留，已移除并补充同排对齐断言。
- 最终 18 组隔离交互回归通过，包含同排按钮几何对齐；ResizeObserver 首次绑定也校准虚拟视口。最终截图为 visualization 目录 `device-files-optimized.png`（模拟设备）。
- 最新代码重新构建独立 EXE 成功：`src-tauri/target/release/serial_terminal.exe`，4,841,472 字节，2026-10-17 17:53:19。后端 19 项测试、前端构建/夹具类型检查、cargo check、fmt 与 diff 检查均通过。Windows 链接器中文“正在创建库”提示仅 warning，不影响成功。
- 本轮测试标签已关闭，测试服务器已停止，未关闭用户原有程序，也未修改/刷盘真实设备。README 和只读性能诊断脚本已补全。阶段 24 完成，代码未提交。

## 2026-10-17：设备文件管理第一版
- 开始阶段 23；当前工作区干净，已有统一按钮按下效果提交。
- 复用现有后台 ADB 初始化，文件页与终端实例分离，使用主题内确认弹窗。
- 不访问或改动真实设备文件；后端安全边界与操作行为使用隔离测试验证。
- 原生目录选择首次编译缺 Shell_Common feature；本地 windows-sys 0.52 源码有该门禁而清单没有该 feature。改为声明两个稳定 shell32 ABI，保留依赖版本和缓存不变。
- 已接入独立文件标签、懒加载目录树/列表、路径导航、上传文件/目录、下载和主题确认弹窗。前端构建及 9 项后端测试通过。
- 复查修正：列表单击改为局部更新，防止 DOM 重建破坏双击进入目录；取消并关闭标签等待取消完成，避免过早丢失取消重试；下载允许 /sdcard 等目录而不复用写操作根目录保护。
- 使用 Tauri 官方 mockIPC 的本机浏览器隔离页面，9 组交互回归通过：导航、HTML 文件名转义、确认/取消、多个设备标签、固定设备目标、传输进度保持焦点、串口恢复。
- 复查发现防覆盖需要覆盖最终移动阶段，改为 mv -n/-T 和 Windows MoveFileExW；增加真实隔离子进程的取消/超时/错误输出测试，不使用真实 ADB。
- 一次多文件补丁未匹配格式化后的 Rust 测试位置，已按实际代码位置重做并核对前端修改状态。
- 最终 15 项后端测试通过，覆盖真实隔离子进程取消、超时、关闭中断、NUL 元数据、Unicode、根目录保护和原文件不覆盖；文件短命令也纳入 Job/关闭清理。
- 最终 12 组浏览器隔离回归通过，新增离线/恢复、权限失败保留实际路径、关闭标签取消传输；另用真实鼠标双击验证导航。
- 截图保存于本任务 visualization 目录，README 已说明能力、目录不合并覆盖、权限/临时目录边界及隔离测试方式。
- 检查时发现用户已有 `Desktop/tools/serial_terminal.exe` 实例，本轮未对该实例发出关闭/替换操作，也未操作其真实设备。原生对话框和真实 Android 权限/传输仍需实机验收。
- 最终 `npm run tauri -- build --no-bundle` 完成，独立 EXE 位于 `src-tauri/target/release/serial_terminal.exe`（4,824,064 字节），与最后代码一致；隔离测试页已关闭，本轮启动的测试服务器已停止。前端/测试夹具类型检查、Rust 格式和 diff 检查通过。代码尚未提交。

## 会话：2026-09-10

### 阶段 1：复盘问题与确定方向
- **状态：** complete
- 执行的操作：
  - 读取规划技能说明。
  - 确认 `serial_terminal` 子仓库当前无未提交改动。
  - 将缺陷根因归纳为自研简化终端不满足功能一致。
- 创建/修改的文件：
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 2：替换终端核心
- **状态：** complete
- 执行的操作：
  - 安装 `@xterm/xterm` 与 `@xterm/addon-fit`。
  - 删除自研终端渲染、光标、选区、虚拟列表和 ANSI 简化解析。
  - 使用 xterm.js 接收串口输出、处理输入、滚动、选择、IME 和 ANSI。
  - 使用 FitAddon 跟随右侧终端区域尺寸变化。
- 创建/修改的文件：
  - `package.json`
  - `package-lock.json`
  - `src/main.ts`
  - `src/styles.css`
  - `src/vite-env.d.ts`

### 阶段 3：补齐 UI 与交互
- **状态：** complete
- 执行的操作：
  - 保持深色顶部栏、侧栏、状态胶囊、串口设置和终端显示设置。
  - 连接后继续禁用端口、刷新、波特率、数据位、校验位、停止位。
  - 增加复制按钮、右键复制、Ctrl+C 有选区时复制/无选区时发送中断。
  - 串口参数和终端显示设置保存到 localStorage。
  - Rust 后端连接前先关闭旧串口句柄。
- 创建/修改的文件：
  - `src/main.ts`
  - `src/styles.css`
  - `src-tauri/src/lib.rs`

### 阶段 4：验证
- **状态：** complete
- 执行的操作：
  - 运行前端生产构建。
  - 运行 Rust 后端检查。
  - 运行 Tauri 环境检测。
  - 启动 `npm run tauri dev` 到真实 exe 运行态，确认无立即崩溃后停止。
- 创建/修改的文件：
  -

### 阶段 5：提交
- **状态：** complete
- 执行的操作：
  - 暂存全部迁移修复相关文件。
  - 提交到 `serial_terminal` 子仓库。
- 创建/修改的文件：
  - `task_plan.md`
  - `progress.md`

### 阶段 6：字体搜索与下拉滚动条
- **状态：** complete
- 执行的操作：
  - 新增 Rust `list_fonts` 命令，Windows 下从 HKLM/HKCU 字体注册表枚举字体族名。
  - 前端字体选择从原生 `select` 改为自绘 combobox。
  - 字体下拉支持搜索、Enter 选择首项、Esc 收起、点击外部收起。
  - 自绘下拉滚动条改为主题色。
  - 运行构建和 dev 启动验证。
- 创建/修改的文件：
  - `src-tauri/Cargo.toml`
  - `src-tauri/Cargo.lock`
  - `src-tauri/src/lib.rs`
  - `src/main.ts`
  - `src/styles.css`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 7：统一自绘下拉与输入性能
- **状态：** complete
- 执行的操作：
  - 将串口和波特率从原生 `select` 替换为与字体一致的自绘 picker。
  - picker 当时统一支持搜索、Enter 选择首项、Esc 收起、点击外部收起，后续阶段 8 已按用户要求移除搜索。
  - picker 使用 fixed 浮层，并按按钮位置/窗口剩余空间计算向上或向下展开，保证完整显示在窗口内。
  - 默认窗口尺寸改为 1500×960。
  - 终端输入改为短批量发送队列，减少每个按键一次 Tauri invoke。
  - Rust 串口写入去掉每次 `flush`，减少小包阻塞。
  - 运行构建、后端检查、Tauri info 和 dev 启动验证。
- 创建/修改的文件：
  - `src/main.ts`
  - `src/styles.css`
  - `src-tauri/src/lib.rs`
  - `src-tauri/tauri.conf.json`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 8：简化下拉与异步串口写入
- **状态：** complete
- 执行的操作：
  - 移除自绘 picker 的搜索框、查询状态、过滤函数、搜索占位符和搜索 CSS。
  - 保留自绘下拉的主题滚动条、fixed 浮层、向上/向下展开和窗口内夹取定位。
  - Rust 串口后端增加 writer channel 和后台 writer 线程。
  - `write_text` 命令改为只把字节入队并更新 TX 统计，不再直接同步 `write_all`。
  - writer 线程写入失败时继续通过 `serial-error` 通知前端。
  - 运行构建、后端检查、Tauri info 和 dev 启动验证。
- 创建/修改的文件：
  - `src/main.ts`
  - `src/styles.css`
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 9：主题右键菜单与真实输入链路优化
- **状态：** complete
- 执行的操作：
  - 读取最新需求，确认不能恢复此前被回滚的本地输入预览方向。
  - 使用 GPT-5.3-Codex-Spark 子 agent 完成第一版代码改动，并由主流程复查修正。
  - 终端右键改为主题自绘菜单，包含复制、粘贴、清空，并按选区/连接状态禁用不可用项。
  - 前端输入队列改为控制键抢占式 flush，普通输入短延迟合并，不做本地回显预览。
  - RX 输出和 RX/TX 统计更新改为 requestAnimationFrame 合并。
  - Rust writer 线程 drain channel 合并小包写入，reader buffer 增大，并移除重新引入的 `flush()`。
  - 运行 cargo fmt、前端构建、后端检查、Tauri info 和 Tauri dev 启动验证。
  - 准备提交到 `serial_terminal` 子仓库。
- 创建/修改的文件：
  - `src/main.ts`
  - `src/styles.css`
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 10：终端渲染器性能修复
- **状态：** complete
- 执行的操作：
  - 读取用户反馈，确认连续输入/删除仍卡，本轮重点转向终端渲染器。
  - 使用 GPT-5.3-Codex-Spark 子 agent 调查 xterm renderer、DOM 重挂载和 WebGL 插件现状。
  - 安装 `@xterm/addon-webgl`，将 xterm 渲染后端切换为 WebGL 优先。
  - 增加 WebGL context loss 回退逻辑，避免 WebGL2 不可用时终端不可用。
  - 将 RX 输出从应用层 `TextDecoder` + 字符串拼接改为 `Uint8Array` 合并后直接写入 xterm。
  - 调整终端性能选项：关闭透明、关闭右键选词、启用自定义字形/重叠字形缩放、取消平滑滚动，并将 scrollback 调整为 10000。
  - 给终端区域增加 layout/paint containment，减少终端重绘影响外层 UI。
  - 运行前端构建、Rust 格式化/检查、Tauri info 和 Tauri dev 启动验证。
  - 提交到 `serial_terminal` 子仓库。
- 创建/修改的文件：
  - `package.json`
  - `package-lock.json`
  - `src/main.ts`
  - `src/styles.css`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 11：替换终端库
- **状态：** complete
- 执行的操作：
  - 读取用户反馈，确认 WebGL xterm 方案仍卡，本轮不再继续局部优化 xterm。
  - 选择 `ghostty-web` 作为新的终端核心，使用 Ghostty WASM parser 和 canvas renderer。
  - 安装 `ghostty-web`，卸载 `@xterm/xterm`、`@xterm/addon-fit`、`@xterm/addon-webgl`。
  - 将前端终端导入切换到 `ghostty-web`，增加 `await init()`。
  - 删除旧 WebGL addon 激活、context loss 回退和 xterm 内部 CSS。
  - 改用持久化 `.terminal-surface` 承载 Ghostty canvas/textarea，避免 UI 重渲染时重复 `open()` 或嵌套旧 host。
  - 按 `ghostty-web` 键盘拦截语义修正 Ctrl+C 有选区复制逻辑。
  - 保留行距滑块，使用 Ghostty renderer metrics 兼容实现，避免留下失效控件。
  - 运行前端构建、Rust 格式化/检查、Tauri info 和 Tauri dev 启动验证。
  - 准备提交到 `serial_terminal` 子仓库。
- 创建/修改的文件：
  - `package.json`
  - `package-lock.json`
  - `src/main.ts`
  - `src/styles.css`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 12：证据驱动输入/删除卡顿定位
- **状态：** complete
- 执行的操作：
  - 启动 Tauri dev 并打开 WebView2 远程调试端口，用 CDP/Profiler 分层抓取性能数据。
  - 对 idle、合成终端输出、满 scrollback、真实 Tauri RX 事件、真实 COM5 输入和真实键盘事件分别压测。
  - 确认 Ghostty 渲染在这些测试中没有持续长任务，空闲 5 秒 profile 主要处于 idle。
  - 抓到真实键盘链路中 `write_text` invoke 曾出现 480ms 峰值；旧策略又让 Backspace/Delete 在真实按键间隔下接近一键一 invoke。
  - 移除临时后端 debug command 和前端 debug API。
  - 将普通串口输入合并窗口调为 12ms，Backspace/Delete 调为 40ms，并允许最多 4 个写入 invoke 并发消化积压。
  - 运行前端构建、Rust 格式化/检查、Tauri info，并在 Tauri dev 热重载后的最终代码上确认无新错误。
- 创建/修改的文件：
  - `src/main.ts`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 13：回车连续输出成段刷新定位
- **状态：** complete
- 执行的操作：
  - 读取用户最新反馈，确认现象是按住回车时输出成段刷新，而不是单纯键入/删除延迟。
  - 临时加入前端测量点并通过构建验证，随后因为 WebView2 调试端口返回 502，改用更直接的 COM5 串口读写测试先分离硬件/应用边界。
  - 停掉 Tauri dev 后，用 .NET SerialPort 直接打开 COM5，按 30ms 间隔发送 120 次回车并记录读事件。
  - 直接测试结果：120 次发送对应 120 次读事件，总 3000 字节，每次 25 字节且 1 个换行，平均间隔约 28.4ms。
  - 移除所有临时测量代码，没有保留 debug API。
  - 前端 RX 改为收到 `serial-data` 立即 `terminal.write(new Uint8Array(data))`，只保留统计更新的 rAF 节流。
  - Rust reader 从 32KB/50ms 改为 4KB/5ms，降低短交互输出在串口读取层被聚合的概率。
  - 运行前端构建、Rust 格式化/检查、Tauri info 和 Tauri dev 启动验证。
- 创建/修改的文件：
  - `src/main.ts`
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 14：删除与快速输入低延迟发送
- **状态：** complete
- 执行的操作：
  - 读取用户反馈，确认回车逐行刷新已解决，剩余问题是删除和快速输入仍成块。
  - 复查前端发送链路，确认 Backspace 仍使用 40ms debounce，普通输入使用 12ms debounce。
  - 复查 Rust writer，确认后台线程会 `try_recv` drain 小包并合并到一次 `write_all`。
  - 尝试直接打开 COM5 做删除测试，但当前 `serial_terminal.exe` 正在运行并占用 COM5；为避免打断用户实例，没有强行关闭。
  - 移除前端输入 debounce 和 timer，`queueSerialText` 收到数据后立即 `flushSerialText()`。
  - 写入 invoke 并发上限从 4 提升到 8，减少短时间输入积压。
  - 移除 Rust writer 小包合并，一个 channel 包对应一次后台 `write_all`。
  - 运行前端构建、Rust 格式化/检查和 Tauri info。
- 创建/修改的文件：
  - `src/main.ts`
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 15：分支回切普通 xterm.js
- **状态：** complete
- 执行的操作：
  - 按用户要求从当前稳定状态创建 `codex/xterm-js` 分支。
  - 卸载 `ghostty-web`，安装 `@xterm/xterm` 和 `@xterm/addon-fit`。
  - 将前端 import、终端初始化和样式切回普通 xterm.js。
  - 移除 Ghostty `init()`、持久化 surface 和 renderer metrics 行距兼容逻辑。
  - 使用 xterm 原生 `lineHeight` 支持行距滑块。
  - 修正 xterm `attachCustomKeyEventHandler` 语义：Ctrl+C 有选区复制后返回 `false` 阻止发送中断。
  - 保留阶段 13/14 的低延迟 RX/TX 串口链路。
  - 运行前端构建、Rust 检查、Tauri info 和 Tauri dev 启动验证。
- 创建/修改的文件：
  - `package.json`
  - `package-lock.json`
  - `src/main.ts`
  - `src/styles.css`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 16：修复窗口拉伸时终端黑边
- **状态：** complete
- 执行的操作：
  - 读取用户反馈，确认现象是窗口连续拉伸时终端边缘黑色区域周期性增大/消失。
  - 检查 xterm 默认 CSS，确认 `.xterm .xterm-viewport` 默认 `background-color: #000`。
  - 复查当前终端容器样式，确认外层为 `--terminal`，但 xterm 内部 viewport/screen/scrollbar 仍可能露出默认黑色。
  - 将 `.xterm`、`.xterm-viewport`、`.xterm-screen`、`.xterm-scroll-area`、`.xterm-scrollable-element` 背景统一覆盖为 `--terminal`。
  - 给 `.xterm-screen` 设置 `min-width/min-height: 100%`，让字符网格未覆盖的剩余区域也用终端背景填充。
  - 将 xterm viewport 滚动条轨道和 thumb border 改为终端背景色，去掉 resize 时的黑色轨道边缘。
  - 运行前端构建和 `git diff --check`。
- 创建/修改的文件：
  - `src/styles.css`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 17：退出时清理串口、ADB Shell 和 scrcpy
- **状态：** complete
- 执行的操作：
  - 恢复根 Tauri 项目规划状态，并确认本轮改动目标是根项目而非 `serial_terminal_eui`。
  - 盘点 `src-tauri/src/lib.rs` 中串口、scrcpy 和 ADB shell 的资源持有方式。
  - 发现 AndroidManager 已有 Drop 兜底，但串口 SerialManager 缺少 Drop；同时窗口关闭时没有主动调用统一清理。
  - 为 `SerialManager` 增加 Drop，退出时兜底调用 `close_locked()` 释放读写线程和串口句柄。
  - 抽出 `close_serial_manager()`、`close_android_manager()`、`cleanup_runtime_resources()`，统一关闭串口、scrcpy 子进程和 ADB shell 子进程。
  - 在 Tauri `WindowEvent::CloseRequested` 中主动执行 cleanup，确保点窗口关闭时立即释放所有后端资源。
  - 运行 Rust 格式化、`cargo check` 和 `git diff --check` 验证。
- 创建/修改的文件：
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 18：崩溃/强杀时清理子进程兜底
- **状态：** complete
- 执行的操作：
  - 明确非正常退出边界：主进程被任务管理器强杀或崩溃时，进程内 `Drop` / `CloseRequested` 不能保证执行。
  - 确认串口句柄属于主进程资源，崩溃/强杀后 Windows 会自动关闭 HANDLE 并释放串口占用。
  - 查询本地 `windows-sys 0.52.0`，确认可使用 Windows Job Object API。
  - 在 `src-tauri/Cargo.toml` 为 `windows-sys` 增加 `Win32_System_JobObjects` feature。
  - 新增 `ChildCleanupJob`，创建 Job Object 并设置 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`。
  - 在 AndroidManager 中持有长生命周期 Job Object，保证主进程存活期间 Job 句柄不关闭。
  - scrcpy 和交互式 ADB Shell 子进程启动后立即加入 Job Object；若加入失败，会终止刚启动的子进程并返回错误，避免留下无法兜底清理的进程。
  - 保持正常窗口关闭时的主动 cleanup 和 Drop 兜底不变。
  - 运行 Rust 格式化、`cargo check`、前端构建和 `git diff --check` 验证。
- 创建/修改的文件：
  - `src-tauri/Cargo.toml`
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

### 阶段 19：放宽 ADB 启动与命令超时
- **状态：** complete
- 执行的操作：
  - 读取用户反馈，确认当前 3 秒普通 ADB 命令超时会把 adb server 启动慢误判为失败。
  - 盘点 `ADB_CONNECT_TIMEOUT_MS`、`ADB_COMMAND_TIMEOUT_MS` 和 `run_adb_command()` 的调用点。
  - 新增 `ADB_SERVER_START_TIMEOUT_MS = 20_000`，在实际执行 ADB 命令前先确保 `adb start-server` 成功。
  - 用全局原子标志和互斥锁保护首次 adb server 启动，避免并发刷新/连接时重复启动。
  - 将普通 ADB 命令超时从 3 秒放宽到 8 秒，将远程连接超时从 4 秒放宽到 8 秒。
  - 保留命令级超时，避免远程设备不存在时无限等待。
  - 运行 Rust 格式化、`cargo check`、前端构建和 `git diff --check` 验证。
- 创建/修改的文件：
  - `src-tauri/src/lib.rs`
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

## 会话：2026-10-10

### 阶段 20：新增 ADB ROOT 按钮
- **状态：** complete
- 已读取项目规划和当前代码，工作区开始时没有未提交改动。
- 新按钮位于 scrcpy / ADB Shell 按钮行下方，使用当前选中的 ADB 设备；每次点击执行一次，不判断 root 是否成功。
- 受限 shell 初始化和 Node 内核失败后，经过自动审批使用非沙箱 shell 完成只读检查。
- 已新增主题按钮、事件处理和 `adb_root` 异步命令，捕获点击时设备 ID，忽略 root 返回结果；新按钮与上一行保留 7px 的缩放间距。
- `npm run build`、Rust 格式化、`cargo check --offline --manifest-path src-tauri\Cargo.toml` 和 `git diff --check` 均通过；仅有 Git 换行符提示。
- 修改文件：`src/main.ts`、`src/styles.css`、`src-tauri/src/lib.rs` 和三个项目规划文件。

### 阶段 21：ADB ROOT 点击后立即返回
- **状态：** complete
- 用户反馈点击后按钮变暗、需等待恢复；代码确认设备列表更新时会清空/切换消失的选择，且 root 命令返回仍等进程完成。
- 将 root 改为启动进程后立即返回，保留 root 目标的选择直到用户主动改选；定时设备查询移到后台线程。
- 已完成：前端发起调用后立即返回，后端仅等待进程启动；root 输出丢弃，后台线程只回收进程，不检查结果。
- 已完成：root 目标在定时刷新暂时消失时保持选中；主动选择其他 ADB 设备或连接新远程设备时解除保留。
- `npm run build`、`cargo fmt` 和 `cargo check --offline --manifest-path src-tauri\Cargo.toml` 均通过。
- 用模拟 IPC / 设备列表验证连续点击、临时空列表、其他设备仍在线、目标恢复、主动切换和未选设备六种情形；未对真实设备执行 root。

### 阶段 22：ADB 冷启动后台化
- **状态：** complete
- 先前 ROOT 改动已提交为 `d9a1b8b`；用户批准实施冷启动优化。
- 已确认远程连接/断开和 scrcpy/Shell 启停存在同步命令或锁等待；计划统一到后台执行。
- server 初始化增加共享协调器、存活探测和状态通知，软件启动时后台预热。
- 已实现统一后台任务封装、软件启动时预热、初始化状态通知与快照、server 响应探测及重新初始化。
- 已将远程连接/断开、scrcpy/Shell 启停和设备查询放入后台；ROOT 的直接启动逻辑保持不变。
- 验证通过：`npm run build`、Rust 格式检查、`cargo check --offline`，以及 5 项隔离后端测试（并发一次初始化、启动时读取状态不阻塞、停止/失败后恢复、协议探测、TCP 地址配置）。
- 前端回归通过：启动/失败/恢复显示，过期快照不覆盖新事件，启动中 ROOT 可重复点击，设备暂时消失时 ROOT 目标保留。
- 最终格式检查发现测试修正后的断言需重新排版，已运行 cargo fmt 并通过复核。
- 测试使用模拟启动和临时 loopback 服务，真实冷启动时延与窗口手感待实机确认。
- 修改：`src-tauri/src/adb_server.rs`、`src-tauri/src/lib.rs`、`src/main.ts` 和项目规划记录；未增加依赖。

## 测试结果
| 测试 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|------|---------|---------|------|
| `npm run build` | xterm 迁移后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `cargo check` | Rust 后端 | 编译检查通过 | 通过 | 通过 |
| `npm run tauri -- info` | npm wrapper | 能识别 cargo/rustc | 通过 | 通过 |
| `npm run tauri dev` | 实际启动 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止 | 通过 |
| `npm run build` | 字体搜索修复后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `cargo check` | `list_fonts` 后端命令 | 编译检查通过 | 通过 | 通过 |
| `npm run tauri -- info` | 字体搜索修复后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | 字体搜索修复后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止 | 通过 |
| `npm run build` | 统一自绘下拉和输入优化后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `cargo check` | 去掉写入 flush 后 | 编译检查通过 | 通过 | 通过 |
| `npm run tauri -- info` | 窗口 1500×960 后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | 统一 picker 后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止 | 通过 |
| `cargo fmt` | 异步 writer 改动后 | Rust 格式化完成 | 仅有路径 canonicalize 警告 | 通过 |
| `npm run build` | 移除 picker 搜索后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `cargo check` | writer channel 后端 | 编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | writer channel 后端 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | 异步写入和无搜索 picker 后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止 | 通过 |
| `cargo fmt` | 主题右键菜单和链路优化后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run build` | 主题右键菜单和链路优化后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `cargo check` | 主题右键菜单和链路优化后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | 主题右键菜单和链路优化后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | 主题右键菜单和链路优化后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止；仅有 MSVC linker stdout 警告 | 通过 |
| `npm run build` | WebGL 终端渲染器后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `cargo fmt` | WebGL 终端渲染器后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check` | WebGL 终端渲染器后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | WebGL 终端渲染器后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | WebGL 终端渲染器后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功并保持运行 10 秒，手动 Ctrl+C 停止；仅有 MSVC linker stdout 警告 | 通过 |
| `npm run build` | Ghostty 终端替换后 | TypeScript/Vite 构建通过 | 构建通过；Vite 提示 chunk 大小超过 500 kB | 通过 |
| `cargo fmt` | Ghostty 终端替换后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check` | Ghostty 终端替换后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | Ghostty 终端替换后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | Ghostty 终端替换后 | 启动到 Tauri exe 且无立即崩溃 | 两次启动均成功，最后一次保持运行 10 秒后停止；仅有 MSVC linker stdout 警告 | 通过 |
| CDP idle profile | Ghostty 空闲态 5 秒 | 无持续渲染或长任务 | 约 4.86s idle，无持续 render loop | 通过 |
| CDP 合成输出压测 | 1600 行、20000 行、满 scrollback | 渲染不产生长任务 | render 峰值低，未出现长任务 | 通过 |
| CDP Tauri RX 压测 | 大包和 20000 个小包 `serial-data` | Tauri 事件桥不造成长任务 | 未出现长任务 | 通过 |
| CDP 真实 COM5 输入 | 连续 ASCII/中文/删除 | 找到卡顿瓶颈 | 捕获到 `write_text` invoke 480ms 峰值 | 通过 |
| CDP 删除节流复测 | 240 次 Backspace 连续重复 | 降低 invoke 密度和峰值 | flush 次数约从 240 降为 120，max 约 2.1ms，无长任务 | 通过 |
| `npm run build` | 删除节流最终代码 | TypeScript/Vite 构建通过 | 构建通过；Vite 提示 Ghostty chunk 大小超过 500 kB | 通过 |
| `cargo fmt` | 删除节流最终代码 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check` | 删除节流最终代码 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | 删除节流最终代码 | Tauri 环境检测通过 | 通过 | 通过 |
| 直接 COM5 读写测试 | 120 次回车，30ms 间隔 | 确认硬件/驱动是否逐行返回 | 120 个读事件，总 3000 字节，每次 25 字节/1 换行，平均间隔约 28.4ms | 通过 |
| `npm run build` | 低延迟 RX 改动后 | TypeScript/Vite 构建通过 | 构建通过；Vite 提示 Ghostty chunk 大小超过 500 kB | 通过 |
| `cargo fmt` | 低延迟 RX 改动后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check` | 低延迟 RX 改动后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | 低延迟 RX 改动后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | 低延迟 RX 改动后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止；仅有 MSVC linker stdout 警告 | 通过 |
| 直接 COM5 删除测试 | 快速输入和 Backspace | 验证硬件是否逐键响应 | COM5 被正在运行的 `serial_terminal.exe` 占用，未强行关闭 | 跳过 |
| `npm run build` | 低延迟 TX 改动后 | TypeScript/Vite 构建通过 | 构建通过；Vite 提示 Ghostty chunk 大小超过 500 kB | 通过 |
| `cargo fmt` | 低延迟 TX 改动后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check` | 低延迟 TX 改动后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | 低延迟 TX 改动后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run build` | xterm.js 分支回切后 | TypeScript/Vite 构建通过 | 构建通过，JS chunk 约 368KB | 通过 |
| `cargo check` | xterm.js 分支回切后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run tauri -- info` | xterm.js 分支回切后 | Tauri 环境检测通过 | 通过 | 通过 |
| `npm run tauri dev` | xterm.js 分支回切后 | 启动到 Tauri exe 且无立即崩溃 | 启动成功，手动 Ctrl+C 停止 | 通过 |
| `npm run build` | xterm resize 黑边修复后 | TypeScript/Vite 构建通过 | 构建通过 | 通过 |
| `git diff --check` | xterm resize 黑边修复后 | 无空白错误 | 通过，仅有 CRLF 提示 | 通过 |
| `cargo fmt --manifest-path src-tauri\Cargo.toml` | 退出清理改动后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check --manifest-path src-tauri\Cargo.toml` | 退出清理改动后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `git diff --check` | 退出清理改动后 | 无空白错误 | 通过，仅有 CRLF 提示 | 通过 |
| `cargo fmt --manifest-path src-tauri\Cargo.toml` | 崩溃/强杀兜底改动后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check --manifest-path src-tauri\Cargo.toml` | 崩溃/强杀兜底改动后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run build` | 崩溃/强杀兜底改动后 | 前端构建仍通过 | 构建通过；仅有 npm 版本提示 | 通过 |
| `git diff --check` | 崩溃/强杀兜底改动后 | 无空白错误 | 通过，仅有 CRLF 提示 | 通过 |
| `cargo fmt --manifest-path src-tauri\Cargo.toml` | ADB 超时策略优化后 | Rust 格式化完成 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `cargo check --manifest-path src-tauri\Cargo.toml` | ADB 超时策略优化后 | Rust 后端编译检查通过 | 通过，仅有路径 canonicalize 警告 | 通过 |
| `npm run build` | ADB 超时策略优化后 | 前端构建仍通过 | 构建通过；仅有 npm 版本提示 | 通过 |
| `git diff --check` | ADB 超时策略优化后 | 无空白错误 | 通过，仅有 CRLF 提示 | 通过 |
| `npm run build` | ADB ROOT 按钮与样式 | TypeScript/Vite 构建通过 | 通过 | 通过 |
| `cargo check --offline --manifest-path src-tauri\Cargo.toml` | ADB ROOT 异步命令 | Rust 编译检查通过 | 通过 | 通过 |
| `git diff --check` | ADB ROOT 改动 | 无空白错误 | 通过，仅有 CRLF 提示 | 通过 |
| `npm run build` / `cargo check --offline` | ROOT 立即返回修复 | 前后端编译检查通过 | 通过 | 通过 |
| 模拟 ROOT 点击与设备刷新 | 连续点击、目标临时消失、其他设备在线、重连、手动切换、无选择 | 每次有效点击只发一次，目标不误切，空选择不执行 | 六种场景均通过 | 通过 |
| `cargo test --offline --lib adb_server::tests` | 初始化协调器和临时 TCP 服务 | 并发启动不重复、状态不阻塞、停止后恢复、协议与配置正确 | 5 项通过 | 通过 |
| 前端状态/ROOT 模拟回归 | 初始化中、过期快照、失败恢复、目标暂时掉线 | 标题更新正确，ROOT 不等待、不禁用、不误切 | 通过 | 通过 |
| `npm run build` / `cargo fmt --check` / `cargo check --offline` | ADB 冷启动后台化最终代码 | 前后端构建和格式检查通过 | 通过 | 通过 |

## 错误日志
| 时间戳 | 错误 | 尝试次数 | 解决方案 |
|--------|------|---------|---------|
| 2026-09-10 | 上一版自研 Tauri 终端功能不全 | 1 | 改用 xterm.js 作为终端核心 |
| 2026-09-10 | xterm CSS side-effect import 缺 Vite 类型声明 | 1 | 新增 `src/vite-env.d.ts` |
| 2026-09-10 | npm install 需要写用户 npm cache，被沙箱拒绝 | 1 | 使用授权后的 `npm install @xterm/xterm @xterm/addon-fit` |
| 2026-09-10 | 字体搜索范围太少且原生下拉滚动条不匹配主题 | 1 | 后端枚举系统字体，前端自绘搜索下拉和主题滚动条 |
| 2026-09-10 | 端口/波特率仍为原生下拉，且下拉可能超窗 | 1 | 统一 fixed 自绘 picker，按视口空间向上/向下定位 |
| 2026-09-10 | 终端输入和删除卡顿 | 1 | 前端批量合并发送，后端去掉每次写入 flush |
| 2026-09-10 | 删除仍比较卡，自绘下拉不需要搜索 | 1 | 移除 picker 搜索路径，后端同步写改为 channel + writer 线程 |
| 2026-09-10 | 终端右键菜单不符合主题，输入仍卡 | 1 | 改为主题自绘菜单，并继续优化真实串口输入/渲染链路 |
| 2026-09-10 | 连续输入/删除仍卡，怀疑终端本身 | 1 | 切换 xterm 官方 WebGL renderer，并将 RX 改为字节流直接写入 xterm |
| 2026-09-10 | WebGL xterm 仍卡，用户要求换终端 | 1 | 替换为 `ghostty-web` 并清理 xterm/WebGL 残留 |
| 2026-09-10 | Ghostty 后连续输入/删除仍有卡死反馈 | 1 | 用 CDP 和真实 COM5 分层定位，确认是 `write_text` invoke 抖动被过密 flush 放大；改为删除键专用合并和有限并发写入 |
| 2026-09-10 | 按住回车时输出成段刷新 | 1 | 直接 COM5 测试证明硬件逐行返回，改为低延迟 reader 和前端即时写终端 |
| 2026-09-10 | 删除和快速输入仍成块 | 1 | 移除前端输入 debounce 和后端 writer drain，改为按键级低延迟发送 |
| 2026-09-10 | 需要切回普通 xterm.js 验证 | 1 | 创建 `codex/xterm-js` 分支并回切 xterm.js，保留低延迟串口链路 |
| 2026-09-10 | 窗口拉伸时终端边缘出现周期性黑色区域 | 1 | 覆盖 xterm 内部默认黑色背景并让 screen 最小铺满 host |
| 2026-09-14 | 关闭软件时可能残留串口、ADB shell 或 scrcpy 进程 | 1 | 在后端窗口关闭事件中统一 cleanup，并给 SerialManager 增加 Drop 兜底 |
| 2026-09-14 | 主进程崩溃或被强杀时进程内清理逻辑不会可靠执行 | 1 | 使用 Windows Job Object 的 `KILL_ON_JOB_CLOSE` 托管 scrcpy 和 ADB Shell 子进程，串口由系统关闭 HANDLE |
| 2026-09-15 | ADB 首次启动慢时 3 秒超时容易误判 | 1 | 新增独立 `adb start-server` 20 秒启动窗口，并把实际 ADB 命令超时放宽到 8 秒 |
| 2026-10-10 | 新增状态读取测试多写一层 unwrap，测试目标编译失败 | 1 | 修正测试断言的 Result 解包层数，生产代码编译已通过 |

## 五问重启检查
| 问题 | 答案 |
|------|------|
| 我在哪里？ | 完成阶段 22：ADB 冷启动后台化 |
| 我要去哪里？ | 等待用户实机验证首次启动时界面仍可操作 |
| 目标是什么？ | Tauri 版达到可用的串口终端迁移状态 |
| 我学到了什么？ | 见 findings.md |
| 我做了什么？ | 后台执行 ADB / scrcpy 操作，预热并共享 server 初始化，显示状态、探测存活并恢复，ROOT 仍不等待 |

---
*每个阶段完成后或遇到错误时更新此文件*
