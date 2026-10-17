# Tauri + Vanilla TS

## 设备文件管理（第一版）

- 在现有 ADB 设备框中选择已连接、已授权的设备，点击「打开设备文件」。
- 文件页以右侧标签打开，可同时打开多个设备；标签绑定打开时的设备，之后改变下拉选择不会改变操作目标。
- 支持懒加载目录树、双击/Enter 进入目录、路径输入、上一级和刷新；显示名称、大小、修改时间和权限。
- 支持上传文件/目录、下载文件/目录、新建目录、重命名和删除；删除和文件覆盖需要确认。目录不合并覆盖，同名目录请先重命名或选择其他保存位置。
- 传输在后台进行，可查看进度和取消；ADB 不提供百分比时显示不确定进度。取消或失败不会替换原目标文件。
- 正常关闭会取消文件命令和传输，Windows Job Object 在异常退出时清理托管的 ADB 子进程。网络断开或异常退出可能在设备上留下 `.serial-terminal-upload-*` 临时目录；清理失败会在仍打开的文件页提示路径。
- 目录访问以实际 ADB 权限为准，不自动提权；第一版未实现 `run-as` 私有应用目录访问、文件预览/编辑。设备 shell 需支持 `stat -c` 和 `mv -n/-T`（较旧系统不支持时会提示错误）。
- 沿用同目录优先、系统 PATH 回退的 `adb.exe` 查找方式，不依赖 Android Studio。

### 构建与验证

```powershell
npm run build
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib
npm run tauri -- build --no-bundle
```

独立 EXE 输出为 `src-tauri/target/release/serial_terminal.exe`；同目录放置完整 ADB/scrcpy 工具文件（含其 DLL），或使用系统 PATH 中的工具。

隔离 UI 回归：运行 `npm run dev -- --host 127.0.0.1 --port 1422 --strictPort`，浏览器打开 `http://127.0.0.1:1422/tests/device-files-ui.html`，点击「运行隔离回归测试」。该页面使用 Tauri 官方 `mockIPC`，不会连接真实设备或访问本地真实文件，也不会进入生产构建。重复回归前重新加载页面。

This template should help get you started developing with Tauri in vanilla HTML, CSS and Typescript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
