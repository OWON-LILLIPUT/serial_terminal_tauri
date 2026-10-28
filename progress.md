# 进度日志

## 2026-10-28：限制右键菜单触发范围
- 全局阻止 WebView 默认右键菜单，但不阻止事件传播，保留终端自定义右键菜单。文件菜单仅在文件列表或目录树触发，工具栏、页脚、标题和输入区不再触发菜单。
- 增加隔离回归覆盖默认菜单屏蔽、不阻断自定义菜单、列表/目录树可用及非功能区/弹窗无菜单；不操作真实设备、不打包、不自动提交。
- 4 项右键菜单及 5 项地址保留回归、前端生产构建、测试类型和差异检查通过。

## 2026-10-26：切换本地设备保留远程地址
- 修复旧同步逻辑在选择 USB 设备时清空 IP，或在无远程设备时强制重置默认 IP 的问题。切到本地设备保留现有输入，仅原本为空且没有远程设备时填默认地址；远程设备回填逻辑不变。
- 增加独立回归，覆盖有/无远程设备时切换本地设备、自定义端口与半填地址保留、空值默认回填、选择远程设备及后台焦点保护；不访问真实设备、不打包、不自动提交。
- 5 项回归、前端生产构建、测试类型检查和差异检查通过。

## 2026-10-26：远程 ADB 地址分段输入
- 用户要求将地址按 IPv4 四段和端口一段分开，方便选中修改。以 482ee0c 干净工作区开始，沿用现有连接/断开和历史设备选择，不操作真实设备、不生成最终 EXE、不自动提交。
- 用户澄清仅需输入框样式方便修改，不要扩展功能。已收窄范围：只拆分五段、聚焦选中和必要的原状态同步，不新增校验、整地址粘贴或自动跳转规则，不改 ADB 连接命令。
- 已在 main.ts 和 styles.css 完成五段同排输入，端口稍宽，点击新段或 Tab 聚焦时选中该段；输入仍组合成原地址字符串，历史回填及刷新焦点判断同步适配。前端生产构建、测试夹具类型检查、git diff --check 通过；未进行真实设备/UI 操作，未生成最终 EXE，未提交。
- 用户确认提交本轮输入框调整；仅本地提交，不推送，不生成最终 EXE。

## 2026-10-22：提交文件与文件夹剪贴板传输
- 用户确认提交阶段 35、36：Ctrl+C / Ctrl+V 文件及文件夹上传下载、混合多选、空目录、逐项改名与冲突保护、对应测试和说明；仅本地提交，不推送，不生成最终 EXE。
- 提交前重新通过 50 项后端测试、15 项前端逻辑测试、生产与测试 TypeScript 类型检查、Rust 格式检查及 git diff --check；未重新运行真实设备操作或打包。

## 2026-10-22：文件夹剪贴板传输
- 用户确认补上文件夹和文件/文件夹混合多选复制粘贴。保留阶段 35 全部未提交改动，不打包 EXE，不操作用户剪贴板或实际设备测试。
- 使用文件规划技能跟踪递归与空目录、链接/特殊文件和 Windows 路径边界。上传继续不合并覆盖已有目录；下载只先准备目录清单，内容仍在电脑请求粘贴时读取。
- 已接入目录清单与空目录描述符、混合选中复制、粘贴目录验证和逐项改名；拖放仍保持原有普通文件边界。首轮新增测试编译发现 packed FILEDESCRIPTOR 字段不能直接借用，已复制到对齐局部变量后校验；测试临时目录改用系统时间纳秒戳避免重名。
- 50 项后端隔离测试通过。新增 7 组文件夹 UI 回归，原普通文件剪贴板测试改为接受目录；已通过测试夹具类型检查。按电脑操作技能优先使用浏览器验证本地模拟页面，无系统剪贴板或实际设备读写。
- UI 首轮测试在同步派发 Ctrl+C 后立即检查尚未绘制的通知而失败，实际复制已成功；改为等待模拟 IPC 和最终状态，避免帧调度导致的测试误报。源码查询中一次 rg 引号/通配路径写法不适配 PowerShell，改用单引号及已解析路径，不影响产品代码。
- 7 组文件夹 UI 回归全部通过：混合复制、逐根改名/取消、目录与文件冲突、禁止目录合并、设备绑定、批量取消停止后续项和复制失败恢复。15 项前端逻辑及生产后端 cargo check 通过。已更新说明中的文件夹/空目录能力与清单限制，继续跑原剪贴板及文件管理回归。
- 原剪贴板 10 组、文件管理 25 组通过；拖放回归因断言仍匹配调整前的旧错误文案而失败，已改为检查新的禁止同名覆盖提示后复测。浏览器结果等待偶尔提前超时，继续读取页面最终 PASS/FAIL，不把定位器等待错误当作产品故障。
- 最终既有 UI 回归通过：17 组拖放、12 组框选/删除/进度、11 组侧键历史和 7 组侧栏 ADB；50 项后端、15 项前端逻辑、前端生产构建、测试类型、后端编译、格式和差异检查通过。最后核对文件夹混合上传确认框及当前主题，未生成最终 EXE。
- 最终再次通过 7 组文件夹回归，并核对文件夹/文件两个独立名称输入框、目录标签和取消/确认按钮均沿用当前 UI；截图 `device-files-folder-clipboard.png`。浏览器工具直接 Ctrl+V 被其空虚拟剪贴板拦截，改用应用右键粘贴展示（快捷键由夹具 DOM 事件回归覆盖），未读写系统剪贴板。阶段 36 完成；真实设备/资源管理器端到端仍待实测，不打包 EXE、不提交。

## 2026-10-22：文件剪贴板上传下载
- 用户要求用 Ctrl+C / Ctrl+V 复制粘贴来上传下载文件。以 dd43490 干净工作区开始，复用当前文件页选择、多文件改名/覆盖确认及 Windows 虚拟文件流；不操作用户剪贴板或真实设备进行测试，不打包 EXE。
- 下载采用原生虚拟文件剪贴板，Ctrl+C 不预下载大文件，电脑粘贴时再获取内容；只做复制，不做剪切/删除。普通文件范围与现有拖出一致，目录继续使用原有上传/下载按钮。
- 已接入原生 OLE 剪贴板持有/释放、CF_HDROP 安全解析、剪贴板复制 IPC 与 UI 快捷键/菜单。首次检查发现 ReleaseStgMedium 位于 Ole 而非 Com 模块，已修正导入，未新增依赖。剪贴板所有权必须精确比较 S_OK，不能使用会把 S_FALSE 也当成功的 Result<()> 包装，避免退出时清除别人复制的内容。
- 后端 44 项隔离测试通过（含原生 COM 虚拟剪贴板多次粘贴/取消、CF_HDROP 数据对象提取、文件列表边界和 Windows 同名冲突）。一次测试补丁因同文件片段顺序不匹配被拒绝，已按源码顺序重排后应用；没有丢失现有修改。前端构建和测试类型检查通过，开始浏览器隔离交互回归。
- 新增 10 组剪贴板 UI 回归通过，含多选复制不预下载、不锁定等待、重复按键抑制、输入框/终端隔离、上传逐文件改名/确认/取消、设备绑定、空剪贴板、模拟 IPC 错误及旧通知过滤。既有 25 组文件管理与 17 组拖放回归通过；一次结果定位器提前超时，随后读取实际结果确认为 PASS，非产品回归。文档补丁未匹配长段落，改为独立剪贴板说明小节。
- 既有 12 组框选/删除/进度和 11 组侧键历史回归通过，15 项前端逻辑、生产前端构建、测试夹具类型及生产后端 cargo check --offline --lib 通过；侧栏 ADB 回归与最终视觉核对进行中。未启动真实应用、操作设备或读写系统剪贴板。
- 7 组侧栏 ADB 回归通过。视觉确认多选 2 个模拟文件时，复制/粘贴使用现有终端右键主题，原单文件下载/重命名保持禁用；截图 `device-files-clipboard.png`。初次截图菜单被测试浮层遮住、精确文本定位缺少图标前缀，已改用源码明确的文件名元素打开菜单，最终两项均完整可见，无产品代码变更。
- 阶段 35 完成：44 项后端、15 项前端逻辑、10 组剪贴板及既有 25/17/12/11/7 组 UI 回归、生产编译和格式/差异检查通过。测试不读写系统剪贴板、不执行真实 ADB；实际资源管理器/Android 端到端仍待用户验证。记录使用规划技能限定设备绑定与生命周期边界，浏览器优先验证当前主题；不构建最终 EXE，不提交代码。

## 2026-10-21：退出时可靠关闭串口
- 用户要求程序关闭时主动关闭仍打开的串口。工作区从 c6cd24d 干净状态开始；已有 CloseRequested 清理与 SerialManager Drop，继续核对实际线程退出、句柄释放及打开/关闭交错，不仅补前端异步调用。
- 使用文件规划技能记录关闭顺序与隔离验证，不操作真实串口或用户运行中的程序，不生成最终 EXE。
- 本地 Tauri 源码最初按旧版本路径查询不存在；通过文件清单定位到实际缓存版本 2.11.5 后继续核对退出 API，没有新增或更新依赖。
- 已补上退出标志、连接代次校验、读线程主动取消、writer 唤醒及不可继承串口句柄，并接入窗口销毁/应用退出兜底。首轮测试编译发现测试中 unwrap_err 要求 SerialState 实现 Debug，已改为显式检查锁中毒并恢复状态，不为测试扩大生产类型约束。
- 最终 39 项后端测试全部通过，新增 8 项串口生命周期测试与 2 项 Windows 句柄/停止测试，涵盖收回读写资源、在途 Sender 克隆、重复关闭、正常重连、慢打开后退出/断开、并发连接覆盖、打开失败与状态锁中毒后的清理。
- 前端无 UI 改动，TypeScript 无输出类型检查、15 项既有逻辑测试、cargo check --offline --lib 和 fmt 检查通过。Windows 测试使用隔离事件句柄，不接触真实串口；真实驱动取消行为与关闭后重新占用串口仍待实机确认。未启动用户程序或访问实际设备，未构建最终 EXE，代码未提交。
- 用户确认提交本轮串口退出清理、竞态保护、Windows 句柄修复及对应测试文档；仅本地提交，不推送远端，不生成 EXE。

## 2026-10-21：调整 ADB 按钮分组并提交
- 最终布局为第一排 ADB ROOT / ADB REMOUNT，第二排 ADB SYNC / 浏览文件；同步更新布局回归断言和使用说明。
- 本次布局调整后，前端生产构建、测试夹具类型检查及 15 项前端逻辑测试通过；未重新生成最终 EXE。已记录用户约定：以后默认不打包 EXE，仅在明确要求时生成。
- 用户确认提交，提交范围为 REMOUNT / SYNC 功能、按钮分组、对应测试及文档，不包含构建产物，不推送远端。

## 2026-10-21：新增左侧 REMOUNT 与 SYNC
- 用户要求在左侧增加 ADB REMOUNT、ADB SYNC，分别执行 adb remount 和 adb shell sync。工作区干净，保留已提交功能；按钮同排放在 ROOT/浏览文件下方，使用点击时的当前 ADB 设备，不跟随后续选择变化。
- 沿用 ROOT 的后台单次启动方式，不等待命令结果、不锁按钮，复用工具查找与隐藏控制台；只提示找不到工具/启动失败，不自动 root、重启或额外挂载参数。测试只模拟命令或检查 argv，不对真实设备执行这两个有状态命令。
- 前后端已接入，ROOT/REMOUNT/SYNC 共用安全 argv 构建与后台启动回收；两条新 IPC 已注册。29 项 Rust 测试通过，新增测试检查 USB/远程/含空格设备参数不经 shell 拼接、准确命令与工具目录；15 项已有前端逻辑、生产构建与测试类型检查通过。
- 新增 7 组侧栏 ADB 隔离 UI 回归通过：同排布局/主题、REMOUNT 和 SYNC 连续点击、点击时设备绑定与后续切换隔离、模拟工具启动错误、无设备禁用与恢复，以及 ROOT 原有立即返回行为。延迟 IPC 下按钮均不锁定，没有执行真实 ADB 命令。
- 原有 25 组文件管理 UI 回归通过；29 项后端、15 项前端逻辑及生产构建、夹具类型、fmt/diff 检查通过。新版独立 EXE 构建成功（1m29s，4,940,288 字节，2026-10-21 16:28:57）；未对真实设备执行 ROOT/REMOUNT/SYNC。
- 模拟页视觉确认两按钮同排，位于 ROOT/浏览文件下方，截图 `adb-remount-sync-buttons.png`。规划记录保证本轮不自动 root/reboot、不执行真实设备命令。阶段 33 完成，代码未提交；关闭本轮测试页面与服务器，未改动用户运行的程序。

## 2026-10-19：提交鼠标侧键目录导航
- 用户确认提交。本次仅包含每标签独立目录历史、鼠标后退/前进侧键、滚动恢复、边界保护及对应测试和文档，不包含构建产物，不推送远端。
- 已完成前端生产构建、15 项前端逻辑、11 组新侧键与 25/17/12 组既有 UI、27 项后端回归；物理鼠标驱动分发仍待实机体验。

## 2026-10-19：鼠标侧键目录历史导航
- 用户要求支持鼠标侧键前进后退。工作区干净，最新提交 afc9bcd；实现范围限定设备文件浏览器，每个设备文件标签独立记录成功目录导航，刷新和失败不入栈，后退后新导航清理前进分支。
- 已实现成功时提交历史游标、后退/前进恢复列表滚动、最多 128 项历史、活动文件标签内拦截 mouse down/up/auxclick 且仅 mouseup 导航一次；兼容 BrowserBack/BrowserForward 键，确认弹窗/忙碌/离线/加载中及原生拖动时不切目录。前端构建与夹具类型检查通过，7 项新历史测试 + 8 项已有测试全部通过。
- 首轮侧键 UI 回归在第 4 组的预期失败目录超时：复用的 navigate 测试助手只等待成功路径，因此永远等不到失败目录。改为直接提交非法目标并等待错误提示；生产导航保持当前目录的行为符合预期，继续回归。
- 第二轮已过 8 组，预期“旧目录被删除”用例仍命中 30 秒浏览缓存，无法触发读取失败。测试前置改为模拟新建目录完成后显式失效全部缓存，再删除模拟旧目录；并对加载中点击的场景同样明确失效缓存，避免把缓存跳转当作慢读取。未改变生产缓存策略。
- 最终 11 组侧键历史 UI 隔离回归通过，覆盖一次点击一步、刷新/失败不入栈、前进分支清理、5000 项列表滚动恢复、树不展开、多设备隔离、驱动按键映射、离线/弹窗/加载中保护、历史读取失败重试与串口不拦截。27 项后端测试通过，新版独立 EXE 已构建成功（release 1m24s）；真实鼠标硬件分发仍需实机体验。
- 原有 25 组文件管理回归通过，15 项前端纯逻辑和类型/fmt/diff 检查再次通过。新版 EXE 为 4,929,024 字节（2026-10-19 16:26:07），生产代码与产物一致；最后仅调整测试的失败等待与缓存失效前置。侧键回归截图为 `device-files-side-history.png`（模拟设备）。
- 原有 17 组上传/原生拖出隔离回归通过；继续验证 12 组框选/删除/进度，侧键事件拦截未改变普通左/中/右键行为。
- 12 组框选/删除/进度回归通过，最终共 11 组侧键 + 25/17/12 组既有 UI 回归、15 项前端逻辑和 27 项后端测试通过。阶段 32 完成，代码未提交；关闭本轮模拟页和服务器，不操作真实设备文件或用户程序。物理鼠标/驱动到 WebView2 事件的端到端体验仍由用户复测。

## 2026-10-19：提交文件拖放与多选功能
- 用户确认提交。本次范围包含拖入上传及逐文件改名、原生多文件拖出、鼠标框选与多选删除、拖动状态/进度提示修复、移除耗时秒数，以及对应测试和文档；不提交构建产物，不推送远端。
- 最近独立 EXE 构建成功；前端 8 项逻辑、后端 27 项及 12/17/25 组隔离 UI 回归通过。真实设备到资源管理器端到端拖出仍待实测，此限制随提交保留。

## 2026-10-19：移除底部耗时秒数
- 用户明确要求移除底部“几秒”的显示。删除传输详情初次渲染和实时更新中的耗时文本，保留百分比/大小与原有 3 秒终态自动收起规则；补充两条渲染路径的隔离断言。
- 前端构建、测试类型检查、8 项选择/进度逻辑测试与 diff 检查通过；新版独立 EXE 构建成功（release 1m30s），代码未提交。

## 2026-10-19：已选行拖出与框选优先级
- 用户反馈框选可用但选中后拖出又不可用，并询问 3 秒提示的用途。说明 3 秒是上一条明确要求的结束结果停留时间，并非传输等待；本轮没有擅自改变该时长。
- 已定位到 pointerdown 在判断已选状态前，把所有非名称单元格先转入框选；改为无修饰键的已选整行优先拖出，未选条目空白和 Ctrl/Shift 框选保留。补充从各列起拖、真实名称节点命中的回归，旧测试只在 tr 派发事件因此漏过该路径。
- 前端生产构建、测试类型检查及 7 项选择/计时器测试通过；各列命中浏览器回归与新版 EXE 构建进行中。夹具等待新拖动进入独立等待状态，防止上一轮相同完成文字造成误判。
- 扩展后的 12 组框选/删除/进度隔离回归通过，包含同一框选结果从名称单元格、大小、修改时间、权限和名称 span 五种位置连续拖出，每次均传完整 4 个路径且不创建新框选。截图 `device-files-selected-row-drag.png` 为模拟测试结果；继续验证原有拖放用例。
- 调整为真实子节点起拖的 17 组上传/拖放回归通过；后端 27 项原生边界测试再次通过。保留 3 秒结束提示策略，仅修复前端选择与拖动判定，真实设备到资源管理器仍待用户复测。
- 最终原有 25 组回归通过，前端构建/测试类型检查/差异检查通过。独立 EXE 构建成功（1m34s，2026-10-19 16:00:36，4,928,512 字节），包含已选行命中修正。阶段 31 完成，代码未提交；清理本轮模拟测试页和服务器，未操作真实设备文件或用户程序。

## 2026-10-19：框选、多选删除与传输提示收起
- 保留阶段 28/29 未提交修改。用户要求鼠标左键按住框选、多选删除，以及取消后停止进度动画、结束 3 秒后自动收起底部进度。
- 框选从列表空白处启动，拖动条目继续用于原生拖出下载；计划兼容 Ctrl/Shift 选择、大目录虚拟列表和边缘滚动。批量删除复用后端逐项安全验证，不操作真实设备文件。
- 已实现列表空白/名称外单元格框选、Ctrl 切换/Shift 追加和边缘自动滚动；文件名称保留原生拖出。多选删除统一确认并捕获快照，失败时停止、刷新并保留未删除项选择。取消/终态进度改为确定值，传输 ID 隔离的 3 秒计时器不会清掉下一次传输；批量上传仅整批结束后计时。前端构建、夹具类型检查和 5 项选择逻辑测试通过。
- 新增 11 组框选/删除/进度浏览器隔离回归全部通过，覆盖反向框选、Ctrl 切换/Shift 追加、空白单击、整批取消、混合文件目录删除、失败停止和剩余选中、5000 项边缘滚动与跨虚拟视口框选、终态静止/迟到事件忽略、成功失败取消 3 秒收起以及旧计时器/切标签隔离。纯逻辑扩展至 7 项全部通过；EXE 构建中，继续跑原有 25 组与拖放 17 组回归。
- 原有 25 组文件管理回归与 17 组上传/拖出回归均通过，后端 27 项测试、测试类型检查、fmt/diff 检查通过。最终独立 EXE 构建完成（1m34s，4,928,512 字节）；真实设备到资源管理器端到端仍未验证，本轮新功能均在模拟设备和本地隔离测试中验证。
- 视觉确认多选删除弹窗列出两条正确设备路径与不可撤销/失败停止说明，截图为 visualization 目录 `device-files-group-delete.png`（模拟设备）。阶段 30 完成，代码未提交；关闭本轮测试页面和服务器，不操作真实设备文件或用户程序。

## 2026-10-19：修复拖出挂起与多选
- 用户实机确认拖入上传可用，但拖出鼠标释放后仍显示等待和不确定进度，不能下载；要求补上多选拖动。保留阶段 28 未提交修改，开始阶段 29。
- 旧原生循环在没有窗口的独立 STA，QueryContinueDrag 只看传入键位，且非敏捷数据对象没有异步提取协商；上轮隔离测试未覆盖跨窗口鼠标循环，不能证明实机可用。改为窗口所属 UI 线程启动 OLE，耗时内容通过线程安全虚拟文件/流和异步完成接口提取。
- 替换模块的首次补丁因同一补丁同时删除/新增同一路径被拒绝，改为 apply_patch 原子整文件更新，原文件未丢失。首次编译两处 Windows 类型不匹配（SendMessageW 可选参数、STATSTG 类型值）已按本地签名修正。
- 已接入 UI 线程 OLE、实际按键状态检查、丢失释放/取消看门狗、多文件描述和独立惰性流、异步 EndOperation 等待；前端增加 Ctrl/Shift/Ctrl+A 多选和拖动时保留选择，等待投放不再画动画进度。
- 前端首次构建发现生产 tsconfig 的 lib 不包含 Array.at，已改为兼容的索引/pop，不提高运行时要求。Rust 检查已通过，接下来补回并扩展原生数据/生命周期测试。
- 新原生测试首次编译需明确字节类型与 COM Marshal/StructuredStorage 的函数模块，已补齐本地缓存 API feature/命名空间。新增真实 COM apartment 间封送读取测试，验证线程安全数据对象不需要窗口线程处理内容请求。
- 27 项后端测试全部通过，包含真实 STA→MTA COM 封送读取（不泵源 UI 消息）、多文件描述/流索引、并发请求缓存、拖动结束后等待 EndOperation、接收端失败及取消不挂起。新依赖 feature 导致本轮重新编译 Windows/Tauri 缓存，构建锁占用为本轮测试自身，未中断用户进程。
- 17 组拖放浏览器隔离回归通过，新增 Ctrl/Shift/Ctrl+A 选择、按住选中项不丢失多选、多个原始路径传入、等待无动画/读取后才显示进度、取消等待恢复按钮。继续补充“已投放等待读取”与真正在读的区分，并复核原有 25 组回归。
- “已投放等待目标读取”与真正运行进度的最终 17 组回归通过；后台开始读取后不会被迟到的投放通知退回等待状态。补充等待阶段取消重发，继续构建最终 EXE。
- 原有回归在第 10 组后的重复右键删除确认失败：新代码保留已选行的条件为 false 时，误进入空白列表的 else 分支，清空选中。已改为先区分“有行/空白”再判断是否重选，并增加多选重复右键断言；属于真实生产回归，重新验证后再出最终产物。
- 修正后的原有 25 组回归全部通过，5 项纯多选逻辑测试也通过。第一次 release（2m11s）已编译新 Windows feature 与原生重构；将再次打包纳入最后的右键分支修正。
- 最终 17 组拖放回归再次通过，包含上传名称弹窗与队列回归；真实浏览器点击确认两个文件同时高亮、显示已选 2 项，截图 `device-files-multi-select.png`（模拟设备）。后端 27 项、纯逻辑 5 项、原有界面 25 组均已通过；最终测试类型检查、fmt 与 diff 检查通过。
- 最终独立 EXE 构建成功（release 1m32s，4,926,976 字节），包含最后的右键修正，路径 `src-tauri/target/release/serial_terminal.exe`。未进行真实 Android/资源管理器端到端拖放，不将隔离测试视为该项已验收；交付新版供用户复测。阶段 29 实施完成，代码尚未提交。
- 本轮创建的模拟测试页面与 Vite 服务器已关闭，未改动真实设备文件或用户程序。

## 2026-10-19：文件拖入上传与拖出下载
- 用户确认文件拖入右侧当前目录，统一弹窗逐文件改名，整批确认后上传，取消不上传；设备文件拖到 Windows 文件夹/桌面复制下载。
- 开始阶段 28，工作区干净，上一轮目录导航已提交 b62dbe6。先研究原生拖放并沿用现有传输安全边界，不操作真实设备文件。
- 原生虚拟文件实现首次检查缺少 windows-core 直接依赖（implement 宏要求）；补充现有缓存版本，无需新增远程下载。采用独立 STA 拖放循环，普通文件延迟读取，临时目录随流释放。
- 第二次检查为 FD_FLAGS 未提供 BitOr，改为底层旗标组合后前端构建、夹具类型检查与 cargo check 均通过。已接入 Tauri 原生拖入位置判定、多文件名称表单、整批覆盖预检、顺序上传与队列取消；开始测试原生文件描述/流/清理及隔离浏览器交互。
- 原生隔离测试发现 SHCreateStreamOnFileEx 不实现 Clone（E_NOTIMPL）；包装流改为重新打开缓存文件并复制游标位置，各克隆继续持有临时目录，避免拖出目标请求克隆失败。
- 23 项后端测试通过；拖放浏览器首轮已过 4 组后，覆盖测试读到了上一批相同的完成文案，补充本批 IPC 数量条件再等待，避免把旧完成状态当作新批完成。生产代码未因此变更。
- 最终本轮新增拖放交互首套 11 组通过，包含区域高亮、取消零上传、逐个改名、非法/重复名称、覆盖整批确认/取消、目录冲突、队列取消/失败中止、拖动阈值、文件目标绑定、拖出成功/取消和目录输入提示。后端新增本地路径数量/类型/唯一性测试后共 24 项通过。继续复查 COM 跨线程边界与原有 25 组文件浏览回归。
- 复查本地 windows-implement 源码发现默认自动提供敏捷封送；数据对象和流显式设 Agile=false，确保 Rc/RefCell 与 Shell 流仅在所属 STA 访问，增加接口断言防止回归。
- 原有 25 组文件浏览回归通过。补充原生 IStream CopyTo 的分块取消、克隆读取取消及二进制目标流断言，后端仍全部 24 项通过；注册传输时在锁内复查关闭标志，避免清理与新注册交错。继续增加队列导航/设备切换、Esc 和关标签回归。
- 扩展队列回归在切换回目录时，测试表单越过了仍禁用的前往按钮（上传完成后目录刷新尚未结束）；导航助手增加就绪等待。另复查到路径输入原先有焦点时会在名称弹窗渲染后恢复到背景，限制模态期间只能恢复弹窗内输入焦点并增加断言。
- 扩展后的 14 组拖放隔离回归全部通过，新增单文件/Esc、名称弹窗焦点、队列中切换目录与 ADB 选择仍绑定原目标、关闭标签取消当前及后续项目。前端生产构建/测试类型检查、Rust fmt/diff 检查通过，最终 EXE 重新构建中；真实设备与 Windows 资源管理器端到端拖动未执行。
- 最终源码的原有 25 组回归再次通过。截图预览工具首次在根目录尚未加载时提前提交导航，已改为等待根目录实际项目出现后再导航；仅修改测试夹具，不影响生产产物。
- 最终 25 组原有 + 14 组拖放浏览器隔离回归、24 项后端测试均通过。多文件名称弹窗视觉确认通过，截图为 visualization 目录 `device-files-drag-upload.png`（模拟设备）；README 已说明普通文件范围、重命名/覆盖/队列规则、临时空间与异常残留边界。
- 最新独立 EXE 构建成功（release 1m28s），包含最后的模态焦点修正，路径 `src-tauri/target/release/serial_terminal.exe`。最后仅调整测试预览等待，生产产物与代码一致。本轮没有操作真实设备文件；真实设备到资源管理器的原生端到端拖放未实测，不能将隔离验证视为该项已验收。阶段 28 实施完成，代码尚未提交。

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
