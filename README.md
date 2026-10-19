# Tauri + Vanilla TS

## 设备文件管理（第一版）

- 在现有 ADB 设备框中选择已连接、已授权的设备，点击「浏览文件」（与「ADB ROOT」并排）。
- 文件页以右侧标签打开，可同时打开多个设备；标签绑定打开时的设备，之后改变下拉选择不会改变操作目标。
- 支持懒加载目录树、双击/Enter 进入目录、路径输入、上一级和刷新；显示名称、大小、修改时间和权限。
- 显示点开头的隐藏目录；列表顶部仅保留 `..` 返回上级目录（双击或 Enter），不显示 `.`。`..` 不是可下载、重命名或删除的文件。点击目录树名称在右侧进入该目录，不自动展开左树；若该节点已展开则同时收起。只有点击旁边箭头才展开子目录，路径导航和刷新也不自动展开。
- 元数据列使用固定列宽，名称占据剩余空间；虚拟列表上下滚动不再因长短文件名改变表头位置。窄窗口可横向滚动，长名称及截断元数据可悬停查看。
- 目录树和列表中的文件夹箭头更醒目；文件区域右键菜单与终端共用主题样式，包含上一级、前往、刷新、上传文件/目录、下载、新建目录、重命名、删除和同步到硬盘。右键列表项目自动选中，空白处禁用文件级操作；目录树右键以该目录为上传/新建/刷新目标。删除和覆盖仍需确认。
- 目录元数据通过 `find` + 批量 `stat` 获取，目录树/列表共享每设备 30 秒缓存并合并重复请求；刷新强制重新读取，写操作和连接状态变化会使缓存失效。大目录列表仅渲染视口附近的行，导航不再重建整个应用。
- 「同步到硬盘」对当前文件标签绑定的设备执行 `adb -s <设备> shell sync`，将文件系统缓存刷入设备存储，不是备份到电脑或上传 `adb sync`。后台执行时显示「正在同步…」，完成/失败会提示，允许切换到其他标签。刷盘不能代替备份，也不能保证硬件故障或异常断电时绝不丢失数据。
- 支持上传文件/目录、下载文件/目录、新建目录、重命名和删除；删除和文件覆盖需要确认。目录不合并覆盖，同名目录请先重命名或选择其他保存位置。
- 传输在后台进行，可查看进度和取消；ADB 不提供百分比时显示不确定进度。取消或失败不会替换原目标文件。
- 正常关闭会取消文件命令和传输，Windows Job Object 在异常退出时清理托管的 ADB 子进程。网络断开或异常退出可能在设备上留下 `.serial-terminal-upload-*` 临时目录；清理失败会在仍打开的文件页提示路径。
- 目录访问以实际 ADB 权限为准，不自动提权；第一版未实现 `run-as` 私有应用目录访问、文件预览/编辑。设备 shell 需支持 `stat -c`、`find -maxdepth/-mindepth/-exec {} +/-print0` 和 `mv -n/-T`（较旧系统不支持时会提示错误）。
- 沿用同目录优先、系统 PATH 回退的 `adb.exe` 查找方式，不依赖 Android Studio。

### 构建与验证

```powershell
npm run build
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib
npm run tauri -- build --no-bundle
```

独立 EXE 输出为 `src-tauri/target/release/serial_terminal.exe`；同目录放置完整 ADB/scrcpy 工具文件（含其 DLL），或使用系统 PATH 中的工具。

隔离 UI 回归：运行 `npm run dev -- --host 127.0.0.1 --port 1422 --strictPort`，浏览器打开 `http://127.0.0.1:1422/tests/device-files-ui.html`，点击「运行隔离回归测试」。该页面使用 Tauri 官方 `mockIPC`，不会连接真实设备或访问本地真实文件，也不会进入生产构建。重复回归前重新加载页面。

只读目录性能诊断：`node scripts/benchmark-device-directory.mjs <adb路径> <设备ID> /system/bin`。仅输出数量、耗时和数据大小，不打印/保存文件名，不执行设备写操作。

This template should help get you started developing with Tauri in vanilla HTML, CSS and Typescript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
