# 发现与决策

## 需求
- 用户明确反馈上一版 Tauri 迁移“功能不全，全是 bug”。
- 需要迁移到 Rust + Tauri，但 UI 设计与功能应与现有版本一致。
- 不能使用外层 eui-neo 仓库提交，`serial_terminal` 目录有自己的 git 仓库。
- 字体搜索需要覆盖更多系统字体；字体下拉滚动条颜色需要匹配主题。
- 所有应用内下拉框都需要统一成自绘版本；下拉框超出窗口时允许向上展开，并完整显示在窗口内。
- 终端打字输入和删除卡顿，需要优化跨进程发送路径。
- 用户进一步确认自绘下拉不需要搜索；连续删除仍比较卡，需要继续优化同步写入路径。
- 终端区域右键菜单需要匹配应用主题色，输入/删除仍需继续优化，但不能使用此前被回滚的本地输入预览方案。
- 连续输入和删除仍卡，用户判断是终端问题；允许在必要时换一种终端。
- WebGL xterm 方案实机仍卡，用户明确要求“换终端”。
- Ghostty 终端替换后仍有卡死/卡顿反馈；用户要求先调试抓证据，再基于证据修改。
- 按住回车时仍能看到输出成段刷新；期望是一行一行连续刷出。
- 回车刷新行已解决，但删除和快速输入仍表现为一块一块更新；期望按键级连续反馈。
- 用户要求利用 git 分支把终端核心切回普通 xterm.js 做验证。
- 用户反馈窗口拉伸时终端区域与窗口边框之间会出现黑色区域，拉伸变大后黑色区域周期性变大/消失。

## 研究发现
- 当前 Tauri 版前端终端是自研文本渲染，天然缺少完整 ANSI、IME、宽字符、滚动缓冲、选择和 resize 语义。
- Rust 后端串口桥接已能编译通过，问题主要集中在前端终端核心与交互完整度。
- xterm.js 已成功通过 TypeScript/Vite 构建并进入 Tauri dev 运行态。
- Windows 字体可从 `SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts` 的 HKLM/HKCU 注册表项枚举，值名里包含字体族名。
- 原生 HTML `select` 的下拉搜索与滚动条样式无法稳定按主题定制。
- 端口/波特率/字体原先混用了原生 select 与自绘 combobox；统一为 fixed 自绘浮层后可避免侧栏 overflow 裁剪。
- xterm 的 `onData` 会按输入事件频繁触发；如果每次都直接 `invoke("write_text")`，会产生大量 Tauri IPC 调用。短延迟合并发送能降低卡顿。
- Rust 后端原先每次 `write_all` 后都 `flush`，小包输入时可能阻塞 UI 感知链路；串口终端写入系统缓冲即可。
- 即使去掉 `flush`，`write_text` 仍在 Tauri command 内同步执行 `write_all`；当串口驱动或设备侧变慢时，连续 Backspace/Delete 仍会让 IPC 返回变慢。
- 自绘下拉搜索会给每个 picker 增加额外输入框、过滤状态和列表重绘；用户不需要搜索时应删除这条交互路径。
- 终端右键仍沿用浏览器默认菜单/旧的右键复制行为，不匹配应用深色主题，也缺少粘贴与清空的统一入口。
- 当前前端输入队列对 Enter/Ctrl+C 等立即发送，但 Backspace/Delete/ESC 没有抢占已存在的延迟 flush；连续删除时仍可能被 timer 和前一个 invoke 串行放大。
- RX 串口事件如果每包都直接 `terminal.write` 并立即更新统计 DOM，高波特率或终端文字满屏时会增加主线程绘制压力。
- Rust writer 线程虽然已后台化，但每次 `recv` 后仍只写一个 Vec；短小输入包可在 writer 线程内 drain 合并后再 `write_all`，减少系统调用和驱动压力。
- 本地 `@xterm/xterm` 主包默认使用 DOM renderer；源码注释明确 DOM renderer 是可靠 fallback，并不作为高性能路径。
- 本地此前没有 `@xterm/addon-webgl`；xterm README 将 WebGL addon 描述为 GPU 加速 renderer，插件需要单独安装。
- xterm public API 支持 `terminal.write(string | Uint8Array)`，因此串口 RX 不需要先在应用层 `TextDecoder` 成 JS 字符串再写入。
- WebGL renderer 插件有 `onContextLoss` 事件，适合做失败回退，避免 WebGL2 不可用或上下文丢失导致终端不可用。
- `ghostty-web` 提供基于 Ghostty WASM parser 的终端核心和 canvas renderer，并导出接近 xterm 的 `Terminal`、`FitAddon`、`init` API。
- `ghostty-web` 的 `Terminal.open(parent)` 会把传入的父元素保存为 `terminal.element` 并在里面追加 canvas/textarea；应用重渲染时不能把 `terminal.element` 当作子节点再挂进新 host，否则会嵌套旧 host。
- `ghostty-web` 的 `attachCustomKeyEventHandler` 返回值语义是 `true` 表示阻止默认处理；Ctrl+C 有选区时需要返回 `true`，普通输入返回 `false`。
- `ghostty-web` 构建产物会将 WASM 以内联 data URL 形式打进 Vite bundle，本轮不需要额外复制 `.wasm` 静态资源。
- WebView2 CDP idle profile 显示 5 秒内绝大部分时间处于 idle，Ghostty 空闲态没有持续渲染循环。
- 合成终端输出、满 scrollback、真实 Tauri `serial-data` 事件和批量 Backspace 压测均没有产生长任务；终端渲染本身不是本轮卡死根因。
- 真实 COM5 键盘链路中，连续按键触发的 `write_text` invoke 曾出现 480ms 单次抖动；调整并发后仍可出现 240ms 级别峰值，说明 IPC/后端写入返回时间会抖动。
- 旧策略的 2ms 合并窗口加 Backspace/Delete 立即 flush，在真实按键间隔下几乎变成每个删除键一次 `write_text`，会把 IPC 抖动放大成明显卡顿。
- 删除键专用 40ms 合并窗口后，连续 240 次 Backspace 的发送批次数从 240 次下降到约 120 次，串口 flush 总耗时和最大耗时明显下降，且无长任务。
- 直接用 .NET SerialPort 绕过应用打开 COM5，按 30ms 间隔发送 120 次回车时，得到 120 个读事件、总 3000 字节、每次 25 字节且每次 1 个换行；平均读事件间隔约 28.4ms。
- 直接 COM5 测试说明设备/驱动可以逐行返回；应用中“成段刷新”主要来自应用自己的读取和前端输出聚合。
- Rust 当前 reader 使用 32KB buffer 和 50ms timeout；在 Windows serialport 语义下，这会为了吞吐把短交互输出攒到超时或缓冲边界。
- 前端 `queueSerialOutput` 还会把同一动画帧内的多个 `serial-data` 合并成一次 `terminal.write`；这进一步增加“几行一起刷”的视觉感。
- 删除成块的直接原因之一是 `serialDeleteWriteDelayMs = 40`；这个策略虽然降低 invoke 数量，但会把连续 Backspace 合并成 40ms 一批。
- 快速输入成块的另一个来源是 Rust writer 线程在每次 `recv` 后继续 `try_recv` drain channel，直到 64KB 上限再 `write_all`，会把短时间内多个按键合成一次串口写入。
- 后端 `write_text` 命令现在只做 channel 入队和 TX 计数，实际串口写入在后台线程完成；因此前端不再需要用固定 debounce 保护 UI 线程。
- xterm.js v6 与 `@xterm/addon-fit` 可直接编译通过，生产构建 JS 从 Ghostty 分支约 673KB 降到约 368KB。
- xterm.js 的 `attachCustomKeyEventHandler` 返回 `false` 表示阻止 xterm 继续处理事件；这与此前 Ghostty 分支记录的语义相反，因此 Ctrl+C 有选区复制时必须返回 `false`。
- xterm.js 原生支持 `lineHeight` 选项，不需要 Ghostty 分支中直接改 renderer metrics 的兼容逻辑。
- xterm 默认 CSS 中 `.xterm .xterm-viewport` 使用 `background-color: #000`，composition view 和 scrollbar shadow 也有黑色默认值。
- xterm 的 screen/canvas 实际尺寸按字符宽高的整数列/行变化；窗口连续拉伸时，host 尺寸会先变化，而 terminal screen 会在下一次 fit 到新列/行时跳变，因此边缘剩余区域会周期性出现/消失。
- 如果 xterm 内部 viewport/screen/scrollbar track 的背景与外层终端背景不一致，resize 时这些剩余区域会表现为黑色闪动或黑边。
- 当前 Tauri 后端已保存 scrcpy 子进程和 ADB shell 子进程句柄；AndroidManager 的 Drop 会做兜底释放，但窗口关闭时最好主动清理一次，避免前端窗口已经消失后子进程继续短暂存在。
- 当前串口 `SerialManager` 原先没有 Drop 兜底；只靠前端在关闭前调用 `disconnect` 不可靠，因为窗口关闭时异步 IPC 可能来不及完成。
- 被任务管理器强杀、段错误崩溃等非正常退出时，Rust `Drop`、Tauri `CloseRequested` 和普通清理函数都不能保证执行；需要依赖操作系统资源所有权和子进程托管机制。
- Windows 会在进程结束时自动关闭该进程持有的串口 HANDLE，因此串口占用在崩溃/强杀后会由系统释放；scrcpy 和 `adb shell` 是独立子进程，需要放进 Job Object 才能跟随主进程消亡。
- `windows-sys 0.52.0` 已提供 `CreateJobObjectW`、`SetInformationJobObject`、`AssignProcessToJobObject`、`JOBOBJECT_EXTENDED_LIMIT_INFORMATION` 和 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，可直接用于本项目的 Windows 子进程兜底清理。
- ADB 首次调用可能隐式启动 adb server，原先 `ADB_COMMAND_TIMEOUT_MS = 3000`、`ADB_CONNECT_TIMEOUT_MS = 4000` 会把“server 启动慢”误判成命令失败。

## 技术决策
- 阶段 26：. / .. 是导航项而非可修改文件，显示在右侧列表顶部，不进入目录树以免出现循环。真实 .hidden 等目录沿用后端 find 的完整结果，两侧均显示。列宽抖动由自动表格布局随着虚拟行重新分配产生，采用固定元数据列 + 名称剩余宽度及稳定滚动条占位修复。
- 阶段 25：文件菜单直接共用 terminal-context-menu/item/icon 样式；右键行自动选中目标，右键空白取消文件选择，目录树菜单固定到该目录。导航、传输、写操作均复用现有 action，不另开 ADB 实现；关闭/刷新/切标签时清除菜单，防止目标过期。
- 本轮：用户明确确认“同步到硬盘”指设备端刷盘（adb shell sync），不是备份到电脑。
- 本轮：目录脚本每个项目都用命令替换启动 stat 子进程；前端 navigate 即便已有目录缓存仍重新请求，loadTree 与 navigate 可能重复读同一路径，且每次更新都重建整个应用 DOM。
- 2026-10-17：文件页独立于 xterm 会话，绑定打开时设备；切换 ADB 下拉不改变已打开文件页操作目标。
- 2026-10-17：当前 ADB 文本执行器的 8 秒超时不适用于文件传输；新模块提供后台进度、取消、独立超时和进程退出清理。
- 2026-10-17：远端目录使用 NUL 分隔元数据，避免按空格解析 ls；路径严格引用，拒绝根目录删除和危险名称，覆盖/删除前确认。
- 2026-10-17：原生文件对话框使用已存在的 windows-sys，不引入 Android Studio 或完整 SDK 依赖。
| 决策 | 理由 |
|------|------|
| 换用 xterm.js | 成熟终端组件覆盖终端核心复杂度，减少自研 bug 面 |
| 保留现有深色工具型 UI 外壳 | 用户要求 UI 风格一致，且这部分适合项目内定制 |
| 用 FitAddon 跟随窗口尺寸 | 保持终端行列随容器变化，而不是手写滚动/resize 算法 |
| Backspace/Delete 统一映射为 BS | 延续现有串口 shell 行为，避免简单设备把 Delete 序列回显成普通文本 |
| 字体选择使用自绘搜索下拉 | 支持输入过滤、Enter 选择首项、Esc 关闭，并能统一滚动条主题色 |
| 后端 `list_fonts` 命令返回字体族名 | 前端不依赖写死列表，搜索范围随系统字体变化 |
| 所有 picker 使用 fixed 浮层和视口夹取定位 | 下方空间不足时向上展开，并保证 left/top/maxHeight 在窗口内 |
| 输入发送使用队列合并 | 保持 Enter/Ctrl+C 等控制输入立即发送，普通输入/删除短批量合并 |
| picker 移除搜索框 | 符合用户最新要求，同时减少 DOM 与事件绑定开销 |
| Rust 后端使用 writer channel | Tauri 命令只入队字节，后台线程负责 `write_all`，降低连续删除/输入对前端交互的阻塞 |
| 终端右键菜单改为自绘主题菜单 | 浏览器默认 context menu 无法匹配主题，且交互项不可控 |
| 控制输入允许抢占延迟 flush | Backspace/Delete/Enter/Ctrl+C/ESC 是交互反馈最敏感路径，应尽快进入真实串口写入队列 |
| RX 输出和统计更新按动画帧合并 | 降低每包事件造成的 `terminal.write` 与 DOM textContent 更新频率 |
| 优先切换 xterm 官方 WebGL renderer | 先替换渲染后端，不立刻换完整终端库，能最大程度保留现有功能和 API |
| RX 输出改为 `Uint8Array` 写入 | 降低 JS 字符串分配/拼接，保留 xterm 的流式 UTF-8 处理 |
| 替换为 `ghostty-web` | 用户反馈 WebGL xterm 仍卡；Ghostty WASM parser + canvas renderer 能替换完整终端核心，同时继续复用当前串口桥接和 UI 外壳 |
| 终端 DOM 改用持久化 surface | Ghostty 将 `open()` 入参作为终端根元素，重渲染 UI 时移动这个 surface 比重新 open 或挂载 `terminal.element` 更安全 |
| 不继续盲目替换终端 | CDP 证据显示 Ghostty 渲染链路稳定，继续换终端不会命中本轮已抓到的卡顿点 |
| 前端串口写入采用有限并发 | 允许最多 4 个 `write_text` invoke 并行消化积压，避免单个慢 invoke 把队列完全堵死 |
| 删除输入使用更长 debounce | Backspace/Delete 连续重复时合并发送，降低 IPC 数量，同时保留真实串口发送而不是本地假预览 |
| 串口 reader 改为低延迟读 | 回车交互输出需要优先保证行级反馈，4KB buffer + 5ms timeout 比 32KB + 50ms 更符合终端手感 |
| RX 到达即写入终端 | 统计 DOM 可以帧级节流，但终端正文不应为省重绘把交互输出攒到下一帧统一写 |
| TX 到达即尝试发送 | 串口写入已后台化，前端输入应优先保证交互连续性，积压只在 invoke 并发耗尽时发生 |
| writer 不再 drain 合并小包 | 快速输入/删除需要按键级到达设备，后台线程不应再把多个按键主动合成一次 `write_all` |
| xterm 分支不引入 WebGL addon | 用户要求普通 xterm.js，本分支只使用 `@xterm/xterm` 和 `@xterm/addon-fit` |
| 覆盖 xterm 内部背景 | resize 时字符网格不能连续填满所有像素，必须让所有可能露出的内部层使用同一终端背景色 |
| 退出清理由后端生命周期统一处理 | 串口句柄和 adb/scrcpy 子进程属于后端资源；窗口关闭时直接从 Tauri 后端清理比依赖前端异步调用更可靠 |
| 崩溃/强杀场景对子进程使用 Windows Job Object | 进程被强杀或崩溃时 Rust 代码不能保证继续执行；Job Object 的 `KILL_ON_JOB_CLOSE` 能让系统在主进程句柄关闭后自动结束已加入的子进程 |
| Job Object 仅托管长生命周期子进程 | `adb connect/devices` 这类短命令不放入长生命周期 Job，避免影响 adb server；仅将 scrcpy 和交互式 `adb shell` 加入主进程生命周期托管 |
| ADB 首次启动和具体命令分开设置超时 | 首次 `adb start-server` 使用更长 20 秒窗口；普通 ADB 命令和远程连接命令放宽到 8 秒，既减少误判也避免设备不存在时无限等待 |

## 遇到的问题
| 问题 | 解决方案 |
|------|---------|
| 手写终端无法快速达到功能一致 | 使用成熟终端控件替换 |
| xterm 已初始化后 DOM 重建可能重复 open | 检测 `terminal.element`，重建 UI 时将已有 xterm 节点重新挂载 |
| Tauri CLI 找 cargo 的问题仍需兼容 | 继续使用项目内 wrapper 注入 `%USERPROFILE%\.cargo\bin` |
| 字体下拉滚动条颜色不符 | 原生 select 换成自绘列表，使用 CSS scrollbar-color 和 webkit scrollbar 设置主题色 |
| 端口和波特率仍是原生下拉 | 改为同一套 picker 渲染和交互 |
| 输入/删除卡顿 | 降低前端 IPC 频率并去掉后端每次写入 flush |
| 删除仍比较卡 | 后端同步 `write_all` 改为后台 writer 线程，前端 invoke 只等待 channel 入队 |
| 右键菜单不符合主题 | 使用固定定位的自绘菜单并按视口夹取位置 |
| 输入仍比较卡且不能恢复本地预览 | 优化真实链路：前端抢占 flush、RX 帧合并、统计节流、后端 writer 合并小包 |
| 连续输入/删除仍卡且怀疑终端本身 | 接入 `@xterm/addon-webgl`，使用 GPU renderer；若不可用自动回退 DOM |
| WebGL xterm 仍卡 | 替换为 `ghostty-web`，清理 xterm/WebGL 依赖和内部 CSS，修正挂载和键盘拦截语义 |
| Ghostty 后仍反馈卡死 | 通过 CDP profile、合成输出、真实 Tauri RX 和真实 COM5 键盘输入分层定位，确认主要瓶颈是 `write_text` 调用密度和 invoke 抖动，而不是终端渲染 |
| 按住回车输出成段刷新 | 直接 COM5 读写测试显示硬件逐行返回；移除前端 RX rAF 合并，并降低 Rust reader timeout/buffer |
| 删除和快速输入仍成块 | 移除前端 Backspace/普通输入 debounce；提高 invoke 并发上限；后端 writer 改为每个入队包单独 `write_all` |
| 切回 xterm.js 需要重新处理键盘语义 | 将 Ctrl+C 选区复制处理改为返回 `false` 阻止 xterm 发送中断 |
| xterm resize 黑边 | 将 root、viewport、screen、scroll area、scrollable element、scrollbar track 统一设为 `--terminal`，并给 screen 设置最小 100% 宽高 |
| 关闭软件后可能残留串口、ADB shell 或 scrcpy 进程 | 给 `SerialManager` 增加 Drop 兜底，并在 `CloseRequested` 窗口事件中主动调用串口、ADB shell、scrcpy 的统一 cleanup |
| 主进程崩溃/强杀时进程内 cleanup 无法可靠执行 | 用 Windows Job Object 管理 scrcpy/ADB Shell 子进程，主进程消亡时 Job 句柄关闭并由系统结束子进程；串口 HANDLE 由系统自动关闭 |
| ADB 启动慢导致 3 秒超时误判 | 在执行实际 ADB 命令前先用 20 秒超时启动 adb server，并将普通命令/连接命令超时提高到 8 秒 |

## 资源
- 本轮实机只读基准：448 项的 /system/bin 旧脚本 10238/10238/10175ms；仅批量 stat 仍需 5380–5527ms，说明逐项 shell test 也昂贵。改为 find 原生遍历 + 批量 stat，脚本降至 146/134/163ms；不显示设备文件名、不执行任何写操作。
- 实机 stat -c 不解释 \\000 转义，因此不能直接依赖 stat 输出 NUL。新协议用 shell printf 输出 NUL 文件名、数值元数据按批次行数校验，find -L 补充目录链接，避免破坏特殊名称安全。
- 最终批量协议经过实际只读输出验证：/system/bin 448 项 145–155ms，/ 根目录 32 项 132–140ms。stat 数值输出可能为 CRLF，Rust lines 与诊断脚本均兼容。缓存仅用于浏览，上传覆盖预检仍使用新读取的目录结果。
- 目录导航改为文件面板 rAF 合并重绘，不重建侧栏、标签或串口终端；虚拟列表固定 32px 行高，ResizeObserver 同步视口。回归发现并修正导航后的旧滚动捕获、确认弹窗残留，以及 ROOT 旧独立上边距。
- 设备端 sync 使用独立 120 秒后台窗口，目标固定为文件页的 deviceId；不是电脑备份，也不宣称能完全避免断电/硬件故障的数据损失。成功、失败、重试及切标签不阻塞串口均通过模拟设备测试。
- 2026-10-17：ADB 原始手册确认 push/pull 支持文件和目录，shell -T 可禁用伪终端，目录元数据保留 NUL 分隔。来源：https://android.googlesource.com/platform/packages/modules/adb/%2Bshow/refs/heads/main/docs/user/adb.1.md
- 2026-10-17：Toybox 官方帮助确认 stat -c 以及 mv -n/-T；重命名不覆盖、目标不按目录解释。来源：https://landley.net/toybox/help.html
- 2026-10-17：下载成功提交使用 Windows MoveFileExW，无覆盖授权时以 OS 原子 no-replace 防止目标在检查后出现；失败或取消只清理本次创建的唯一临时目录。
- 2026-10-10：ADB ROOT 改动已提交为 `d9a1b8b`，本阶段开始时工作区干净。
- 2026-10-10：`adb_connect` / `adb_disconnect` 在同步 Tauri 命令中等待 `ADB_SERVER_START_LOCK` 和进程输出；scrcpy 启动还在同步命令中固定等待 150ms。统一改为 async command + blocking worker。
- 2026-10-10：现有 `ADB_SERVER_STARTED` 仅表示曾经启动成功，不能检测 server 被其他软件结束；增加 server 响应探测，失效时重新启动，不重放已发出的 root。
- 2026-10-10：初始化通知复用现有 SCRCPY 标题状态，事件监听后再读取快照并按版本号防止旧快照覆盖新通知。
- 2026-10-10：新增独立 `adb_server.rs` 管理 idle/starting/ready/failed 状态，初始化锁仅由 blocking worker 获取；只读 smart-socket `host:version` 验证 server 存活，兼容自定义 TCP server 地址。
- 2026-10-10：scrcpy 使用与应用相同的 ADB 可执行文件（子进程 `ADB` 环境变量），避免 PATH 解析差异；初始化仍使用 20 秒窗口，实际命令仍使用 8 秒窗口。
- 2026-10-10（修正）：`applyAndroidState` 在目标暂时消失时会清空或改选其他设备；root 按钮因此禁用，甚至下次点击会命中其他设备。必须保留 root 的当前选择，用户主动改选时解除保留。
- 2026-10-10（修正）：第一版 root 虽然运行在 blocking worker，但仍等待命令结束后才返回 IPC；改为只启动命令，丢弃输出，后台回收进程句柄。
- 2026-10-10（修正）：定时 `list_adb_state` 仍是同步 Tauri 命令，可能在 root 重连期间阻塞；设备查询应在 blocking worker 中执行。
- 2026-10-10：当前“打开 scrcpy”和“打开 ADB Shell”位于同一行；ADB ROOT 新按钮放在该行下方，复用主题按钮样式和当前设备选择。
- 2026-10-10：ADB ROOT 每次点击只调用一次 `adb -s <device_id> root`，不重试、不解析结果；后端使用 blocking worker，避免进程等待阻塞界面。
- 2026-10-10：新命令复用同目录优先、PATH 回退；阶段 21 已移除 root 的 server 预启动等待和命令超时等待，改为直接启动 root 进程并立即返回。仍提示找不到 adb / 无法启动进程等错误，忽略 root 本身的输出和执行结果。
- 本地 `serial_terminal` Tauri 项目。

## 视觉/浏览器发现
- 阶段 32 完成：侧键 11 组和原有 25/17/12 组 UI、15 项前端逻辑、27 项后端测试全部通过；图像 `device-files-side-history.png` 为模拟设备验收证据。规划技能用于跟踪历史状态与回归边界，未更改当前布局或引入真实设备写操作。
- 阶段 32 兼容性：原有 25 组目录/菜单/文件操作回归通过，侧键测试截图 `device-files-side-history.png` 显示 TEST-A 标签仍绑定原设备，即使 ADB 下拉选 TEST-B。新版 EXE 已构建，未增加任何新工具栏控件或改变布局。
- 阶段 32 最终侧键回归：11 组标准 DOM 侧键/按键模拟全部通过，验证单步、边界、滚动位置、失败重试、分支和设备隔离；返回大目录时 scrollTop 精确恢复为 32000，树仍保持折叠。此验证不冒充真实鼠标驱动/WebView2 端到端体验。
- 阶段 32 回归：目录历史回退继续复用现有 30 秒缓存，因此测试模拟“设备端目录已消失”时需要显式失效浏览缓存，不能仅删除 mock 数据就期待访问失败；已采用模拟写操作后的确定性失效路径，真实设备未写入。
- 阶段 32 实现：导航历史独立于浏览器网页 history，只有经过 request/cacheVersion 保护的成功结果才能入栈/移动游标。历史条目保存列表滚动，不改变树展开状态。后退/前进最多保留 128 项，避免长期使用无限增长；同一次 down/up/auxclick 只在 up 触发。
- 阶段 32：目前仅 navigate 更新 path，无浏览历史。历史只能在目录读取成功且 request/cacheVersion 仍有效时提交，不能在点击或发请求时移动索引。MDN 说明 MouseEvent.button 3/4 是常见后退/前进侧键，auxclick 在鼠标释放后发生，应统一阻止默认网页动作并避免同一次鼠标序列重复导航。参考：https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent/button ，https://developer.mozilla.org/en-US/docs/Web/API/Element/auxclick_event 。
- 阶段 31 最终：12 组框选/删除/进度、17 组拖放与 25 组原有 UI 回归、7 项前端逻辑和 27 项后端测试均通过；新版 EXE 已生成。规划记录本轮只改拖动命中优先级，不擅自删除用户上一条要求的 3 秒终态提示。
- 阶段 31：12 组扩展隔离回归通过，验证元数据/名称五处命中均启动原生拖动命令并保留整组选择；旧合成 tr 命中助手已换为真实子元素。该结果仅证明前端路由与隔离传输生命周期，不等于真实资源管理器拖出已验收。
- 阶段 31：选择后的元数据列也必须作为整组拖动把手，不能先按“非名称”规则启动框选。此前拖动夹具直接对 tr 发送 pointerdown，和真实 td/span 命中不同；改为实际子元素和其几何坐标，扩展已选行的名称/大小/时间/权限等列回归。
- 阶段 30 截图 `device-files-group-delete.png`：模拟设备两文件仍高亮，统一确认显示准确路径、总数与危险提示；未实际删除真实设备文件。新版 EXE 位于 `src-tauri/target/release/serial_terminal.exe`，本轮使用规划技能保留前轮修改和实机验证边界。
- 阶段 30 最终兼容性：原有 25 组与拖放 17 组回归均通过，真实浏览器点击两个模拟文件确认多选后删除按钮启用；新增 11 组的删除/收起行为及原生后端 27 项、前端纯逻辑 7 项均通过。
- 阶段 30：新增 11 组模拟交互回归通过，实测矩形框选跨 5000 项虚拟列表时包含未渲染条目，松开停止边缘滚动；删除失败保留剩余项目，成功/失败/取消均 3 秒收起，切换标签及新传输不受旧计时器影响。真实设备文件未读写。
- 阶段 30 首轮实现：框选矩形使用完整目录行索引和滚动偏移，名称以外的单元格也可作为起点，避免大目录没有底部空白时无法框选；名称仍用于拖出。批量删除快照包含文件/目录，确认列出前 20 项并提示总数，失败停止并保留剩余项目。
- 阶段 30：结束后的取消/失败进度仍用原生 progress 且无 value，浏览器会继续不确定动画；现有终态没有自动收起计时器。将终态和取消中均渲染确定值，终态按会话/传输 ID 设置 3 秒清理，批量上传只在整批结束后启动计时。
- 阶段 30：已有多选集合但删除与重命名共享“多选禁用”，需单独开启删除并使用确认时捕获的条目快照逐项执行；大目录仅渲染可见行，框选必须按固定行高与完整 entries 计算，不能只遍历 DOM。
- 阶段 29 最终：17 组拖放 + 25 组原有隔离 UI 回归通过，27 项后端（含实际 COM 跨 apartment 封送）和 5 项多选逻辑测试通过。真实浏览器 Ctrl 点击验证两个文件同时高亮及已选计数，截图 `device-files-multi-select.png` 为模拟设备；新版独立 EXE 已构建，真实 Android 到资源管理器拖出仍待用户复测。
- 阶段 29：标准 CoMarshalInterThreadInterfaceInStream / CoGetInterfaceAndReleaseStream 实测可在 MTA 接收端读取源 STA 创建的敏捷虚拟文件，无需源线程消息泵；同时确认 GetData 仅返回惰性流，不调用 ADB loader。异步源拖动已结束但 EndOperation 尚未到达时，完成结果不会提前返回。
- 阶段 29：官方异步拖放协议为 IDataObjectAsyncCapability StartOperation/EndOperation；DoDragDrop 返回后异步数据提取可能未结束，必须等待 EndOperation，不应在等待投放时显示下载进度。来源：https://learn.microsoft.com/en-us/windows/win32/shell/datascenarios#dragging-and-dropping-shell-objects-asynchronously 。旧独立无窗口线程循环与实机鼠标释放不收敛高度相关，尚未对用户运行中的进程进行侵入诊断。
- 阶段 28 视觉确认：多文件统一弹窗，每项显示原名/大小和独立名称输入，当前设备与上传目录明确可见；首个输入获得焦点，背景保持当前 UI 主题。截图 `device-files-drag-upload.png` 为模拟文件，不代表真实设备传输。
- 阶段 28 最终扩展交互：14 组模拟拖放回归通过，设备/目录变化不会重定向队列，关闭标签等待取消并不启动后续文件；单文件 Esc 和逐文件弹窗焦点也通过。原生流 CopyTo 按块读取并检查同一取消标志，24 项后端隔离测试通过。
- 阶段 28：11 组新增浏览器拖放交互通过，模拟 Tauri 原生事件而非网页 dataTransfer，因此覆盖物理坐标到 CSS 像素换算、列表边界与真实路径进入重命名流程；真实设备及资源管理器的端到端拖动尚未执行。
- 阶段 28 方案：Windows 专用 STA 线程提供 FILEGROUPDESCRIPTORW + 延迟 FILECONTENTS/IStream，只允许 COPY。GetData 请求内容时复用安全 adb pull 至独立临时目录，流持有目录生命周期，取消/错误不冒充成功；原生循环和设备读取均不占窗口 UI 线程。拖出仅普通文件，目录继续用下载按钮。
- 阶段 28 调研：Tauri onDragDropEvent 可提供本地路径和物理坐标；设备文件尚不存在电脑磁盘，拖出不能仅传设备路径，应使用 Windows Shell 虚拟文件（FILEGROUPDESCRIPTORW/FILECONTENTS）复制协议或等价原生方案。官方参考：https://v2.tauri.app/reference/javascript/api/namespacewebview/ ，https://learn.microsoft.com/en-us/windows/win32/shell/dataobject 。
- 阶段 27 最终浏览器回归：25 组通过，目录名称只导航/收起，箭头独立展开，导航和刷新不改变折叠祖先；延迟树读取与右侧导航仍共用一个请求且不会重新展开。截图确认 sdcard 左侧收起、右侧内容显示。
- 阶段 27：目录展开状态应只由箭头增加；名称点击删除该节点的展开状态，右侧导航不展开目标或祖先。写操作后只补读已展开祖先的缓存，避免折叠目录产生无用读取。
- 阶段 26 最终：混合文件/目录的 24 组回归全部通过，目录名称鼠标重复点击行为已实际确认；严格行高检查需允许折叠表头边框对首行造成的 0.5px，其他可见行均为 32px，不会累计滚动偏差。测试截图为模拟设备，未读取或改动真实设备文件。
- 阶段 26：24 组隔离回归确认隐藏目录、. / .. 安全导航、根目录选中隔离、名称重复折叠、固定列宽；5000 项不同长短文件名与大小的模拟目录从顶部/中间/底部上下滚动，所有表头 left 差异 <0.5px，仍只渲染视口行。
- 阶段 25 最终：包含菜单上传下载路由和键盘 Home/End 的 22 组隔离回归通过；真实鼠标右键文件夹及截图确认选中、全部菜单项和终端主题一致，20px 箭头未破坏固定行高。测试环境已清理，未改动真实设备或用户正在运行的程序。
- 阶段 25：22 组隔离 UI 回归通过，确认 20px 箭头、body 菜单浮层、终端共用 CSS、10 项操作、右键选中、空白禁用、视口夹取、目录树上下文、删除确认、离线禁用和各类关闭事件；新增上传下载与键盘导航测试继续复核。未连接或改动真实设备文件。
- Tauri dev 已启动到真实 exe 运行态，无立即崩溃输出；未进行人工视觉截图验收。
- 字体下拉已改为自绘组件；滚动条 thumb 使用 `--accent`，hover 使用 `--accent-strong`，track 使用近黑色。
- 本轮未截图，但 `npm run tauri dev` 已进入 exe 运行态且无初始化错误输出。
- 最新一轮已移除自绘下拉搜索框；列表仍使用主题滚动条和 fixed 浮层定位。
- 最新问题聚焦终端右键菜单和真实输入链路性能；本轮不采用已被回滚的本地输入预览策略。
- 主题右键菜单和真实链路优化后，`npm run build`、`cargo check`、`npm run tauri -- info` 与 `npm run tauri dev` 均通过；Rust 命令仍会输出已知路径 canonicalize 警告。
- 本轮已安装 `@xterm/addon-webgl`，并通过 `npm run build`、`cargo check`、`npm run tauri -- info`、`npm run tauri dev`；Tauri dev 启动后保持运行 10 秒无初始化崩溃。
- 本轮开始替换为 `ghostty-web`；前端 `npm run build` 已通过，待继续做 Rust/Tauri 启动验证。
- 本轮已完成证据驱动性能定位；所有临时调试命令和前端 debug API 已移除，最终生产改动仅保留串口发送节流策略。
- 本轮直接 COM5 测试结果：120 次回车对应 120 个读事件、每个读事件 25 字节/1 个换行；这证明行级输出在应用外是成立的。
- 本轮发现当前 COM5 被正在运行的 `serial_terminal.exe` 占用，因此没有强行做直接串口删除测试，避免打断用户正在测试的实例。
- 本轮 `codex/xterm-js` 分支 Tauri dev 已启动到 `target\debug\serial_terminal.exe`，无立即崩溃。
- 本轮退出清理改动后，`cargo check --manifest-path src-tauri\Cargo.toml` 通过，`git diff --check` 通过；仅有已知路径 canonicalize 和 CRLF 提醒。
- 本轮崩溃/强杀兜底改动后，`cargo check --manifest-path src-tauri\Cargo.toml`、`npm run build` 和 `git diff --check` 均通过；仅有已知路径 canonicalize、npm 版本提示和 CRLF 提醒。
- 本轮 ADB 超时策略优化后，`cargo check --manifest-path src-tauri\Cargo.toml`、`npm run build` 和 `git diff --check` 均通过；仅有已知路径 canonicalize、npm 版本提示和 CRLF 提醒。

---
*每执行2次查看/浏览器或搜索操作后更新此文件*
