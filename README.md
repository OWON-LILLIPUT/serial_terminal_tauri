# Tauri + Vanilla TS

## 设备文件管理（第一版）

- 在现有 ADB 设备框中选择已连接、已授权的设备，点击「浏览文件」（与「ADB SYNC」并排）。
- 左侧第一排为「ADB ROOT」「ADB REMOUNT」，第二排为「ADB SYNC」「浏览文件」。REMOUNT 和 SYNC 分别对点击时选中的设备执行 `adb -s <设备> remount` 和 `adb -s <设备> shell sync`。每次点击启动一次，后台执行、不等待成功/失败结果、不禁用等待；找不到工具或启动失败会提示。无设备选择时禁用，使用现有同目录优先/PATH 回退策略。REMOUNT 不自动 root 或重启，SYNC 是设备端刷盘，不是上传文件的 `adb sync`。文件标签内原有「同步到硬盘」仍保留等待结果的交互。
- 文件页以右侧标签打开，可同时打开多个设备；标签绑定打开时的设备，之后改变下拉选择不会改变操作目标。
- 支持懒加载目录树、双击/Enter 进入目录、路径输入、上一级和刷新；显示名称、大小、修改时间和权限。
- 文件标签激活时，鼠标后退/前进侧键按目录浏览历史切换，并恢复该历史项的列表滚动位置；不是简单跳父目录。每个设备标签独立保存最多 128 项历史，刷新和失败导航不入栈，后退后打开新目录会清除旧前进分支。不展开目录树，不影响串口标签；确认弹窗、忙碌、加载中、设备离线和原生拖动期间不切换。兼容驱动映射为 BrowserBack/BrowserForward 的按键。[侧键标准映射](https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent/button)。
- 显示点开头的隐藏目录；列表顶部仅保留 `..` 返回上级目录（双击或 Enter），不显示 `.`。`..` 不是可下载、重命名或删除的文件。点击目录树名称在右侧进入该目录，不自动展开左树；若该节点已展开则同时收起。只有点击旁边箭头才展开子目录，路径导航和刷新也不自动展开。
- 元数据列使用固定列宽，名称占据剩余空间；虚拟列表上下滚动不再因长短文件名改变表头位置。窄窗口可横向滚动，长名称及截断元数据可悬停查看。
- 目录树和列表中的文件夹箭头更醒目；文件区域右键菜单与终端共用主题样式，包含上一级、前往、刷新、上传文件/目录、下载、新建目录、重命名、删除和同步到硬盘。右键列表项目自动选中，空白处禁用文件级操作；目录树右键以该目录为上传/新建/刷新目标。删除和覆盖仍需确认。
- 目录元数据通过 `find` + 批量 `stat` 获取，目录树/列表共享每设备 30 秒缓存并合并重复请求；刷新强制重新读取，写操作和连接状态变化会使缓存失效。大目录列表仅渲染视口附近的行，导航不再重建整个应用。
- 「同步到硬盘」对当前文件标签绑定的设备执行 `adb -s <设备> shell sync`，将文件系统缓存刷入设备存储，不是备份到电脑或上传 `adb sync`。后台执行时显示「正在同步…」，完成/失败会提示，允许切换到其他标签。刷盘不能代替备份，也不能保证硬件故障或异常断电时绝不丢失数据。
- 支持上传文件/目录、下载文件/目录、新建目录、重命名和删除；删除和文件覆盖需要确认。目录不合并覆盖，同名目录请先重命名或选择其他保存位置。
- 可将电脑普通文件拖入右侧文件列表（一次 1–256 个），高亮区域表示上传到当前目录，不是鼠标下的子文件夹。松开后统一弹窗逐文件修改上传名称，默认保留原名和扩展名；取消或 Esc 整批不上传。非法/重复名称需修改；同名普通文件另行整批确认覆盖，目录和符号链接不能覆盖。确认后顺序上传，取消会停止当前和后续项目，失败也停止队列；已经完成的文件不会回滚。
- Windows 下右侧支持空白处按住鼠标左键框选（包括未选条目名称以外的单元格空白），靠近边缘自动滚动；Ctrl 框选切换选中、Shift 框选追加。也支持 Ctrl 多选、Shift 范围选择、Ctrl+A 全选和 Esc 清除选择。多选后按住任意已选普通文件的整行（名称、大小、时间、权限或空白）拖到资源管理器文件夹或桌面，即可批量复制下载，不删除设备文件。拖动时不会清掉多选；多选状态禁用单项目重命名/下载按钮，删除按钮、右键删除和 Delete 键则作用于整组选中项目，统一确认后逐项删除，失败停止并保留剩余项目。目录/符号链接请使用原下载按钮；一次最多拖出 256 个普通文件，不兼容 Windows 的名称需先重命名。
- 原生 OLE 鼠标循环在窗口所属线程，实际键位与释放检测负责结束拖动；内容为线程安全的惰性文件流，GetData 不等待 ADB，接收端请求读取时才后台下载到独立临时缓存。支持异步复制 StartOperation/EndOperation，鼠标投放结束不被误认为复制完成；等待投放/目标读取时无动画进度，真正下载后才显示进度。未投放、Esc 或取消、目标失败均结束等待。最终保存和重名提示由 Windows 处理，不另弹路径框。
- 原生拖出缓存由文件流引用持有，正常完成/取消/失败后释放；异常强杀可能留下 `%TEMP%/serial-terminal-drag-*` 目录。大文件拖出需额外临时磁盘空间，目标复制结果以 Windows 的完成/错误提示为准。原生跨应用拖放尚需实际设备与资源管理器验收，隔离测试不等同实机验证。
- 请求取消后进度停止更新，完成、失败或取消后的进度不再播放动画，底部传输提示在终态显示 3 秒后自动收起，也可以手动收起；批量上传只在整批结束后计时。新传输不会被上一次的收起计时器影响。
- 传输在后台进行，可查看进度和取消；ADB 不提供百分比时显示不确定进度。按钮上传/下载使用临时文件提交，取消或失败不会替换原目标文件；拖出下载最终写入由 Windows 复制流程负责。
- 正常关闭会取消文件命令和传输，Windows Job Object 在异常退出时清理托管的 ADB 子进程。网络断开或异常退出可能在设备上留下 `.serial-terminal-upload-*` 临时目录；清理失败会在仍打开的文件页提示路径。
- 目录访问以实际 ADB 权限为准，不自动提权；第一版未实现 `run-as` 私有应用目录访问、文件预览/编辑。设备 shell 需支持 `stat -c`、`find -maxdepth/-mindepth/-exec {} +/-print0` 和 `mv -n/-T`（较旧系统不支持时会提示错误）。
- 沿用同目录优先、系统 PATH 回退的 `adb.exe` 查找方式，不依赖 Android Studio。

### 构建与验证

```powershell
npm run build
node --experimental-transform-types --test tests/file-selection.test.mjs
node --experimental-transform-types --test tests/file-history.test.mjs
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib
npm run tauri -- build --no-bundle
```

独立 EXE 输出为 `src-tauri/target/release/serial_terminal.exe`；同目录放置完整 ADB/scrcpy 工具文件（含其 DLL），或使用系统 PATH 中的工具。

隔离 UI 回归：运行 `npm run dev -- --host 127.0.0.1 --port 1422 --strictPort`，浏览器打开 `http://127.0.0.1:1422/tests/device-files-ui.html`，分别点击「运行隔离回归测试」「运行拖放测试」「运行框选与进度测试」「运行侧键历史测试」和「运行侧栏 ADB 测试」（每次运行前重新加载页面）。该页面使用 Tauri 官方 `mockIPC`，不会连接真实设备、执行真实 ADB 操作或访问本地真实文件，也不会进入生产构建。纯历史/多选/进度逻辑测试需要支持 TypeScript 类型转换的 Node.js；侧键事件为标准 DOM 序列模拟，实际鼠标驱动的 WebView2 事件分发需实机验证。后端另有真实跨 COM apartment 封送、异步结束与文件流测试，但这些仍不等于资源管理器和实际设备的端到端验收。

只读目录性能诊断：`node scripts/benchmark-device-directory.mjs <adb路径> <设备ID> /system/bin`。仅输出数量、耗时和数据大小，不打印/保存文件名，不执行设备写操作。

This template should help get you started developing with Tauri in vanilla HTML, CSS and Typescript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
