# Serial Terminal · Ratatui

在仓库根目录的平台终端里启动。Windows 上可用 Windows Terminal。

```powershell
cargo run --manifest-path ratatui/Cargo.toml
```

Ratatui 版有独立的配置目录。在 Windows 上，设置保存在
`%APPDATA%\com.lt.serial-terminal-ratatui\settings.json`。
ADB 和 scrcpy 先从程序同级目录查找，再查找系统 PATH。
项目直接引用仓库内的串口驱动、ADB Server、ADB Shell PTY 和 Windows 文件对话框代码。

## 使用

| 操作 | 快捷键 |
| --- | --- |
| 帮助 | Ctrl+F1 |
| 切换功能区和内容区焦点 | Ctrl+F2 / Ctrl+F3 |
| 上一个和下一个标签 | Ctrl+F4 / Ctrl+F5 |
| 关闭当前标签 | Ctrl+F6 |
| 退出 | Ctrl+F10 |

功能区支持鼠标、方向键和 Enter。串口和每台设备的 ADB Shell 使用各自的终端标签。
自动尺寸同步默认开启，使用绑定的 ADB 设备更新 Android 串口控制台。
ADB Shell 通过 PTY 协议同步尺寸。两者都使用右侧内容区的实际行列。
终端支持鼠标拖选、右键复制粘贴与清屏，以及滚轮或 Shift+PageUp/PageDown 回看输出。

设备文件标签支持 Enter 打开、Backspace 返回上级和 Alt+←/→ 浏览历史。
窗口够宽时显示可展开的目录树。空格、Ctrl+单击和鼠标拖动可多选。
工具栏和右键菜单可访问刷新、上传、下载、新建、重命名、删除和设备刷盘。
Windows 下使用本地文件对话框与文件剪贴板。
Ctrl+V 粘贴文件或文件夹后，逐项确认上传名称，Esc 取消整批。
Ctrl+C 先下载到临时目录，然后可在资源管理器中粘贴。应用退出时释放临时目录。

字体、字号和行距由宿主终端控制。资源管理器与 TUI 之间的跨窗口拖放尚未接入。
旧 ADB 如果回退到传统交互 Shell，连接可用，PTY 尺寸同步需要支持 Shell v2 的 ADB。

## 验证

```powershell
cargo test --manifest-path ratatui/Cargo.toml
```

指定一台允许测试的 Android 设备，可以运行端到端检查。
它会建立单独的 ADB Shell，并在设备的 `/data/local/tmp` 创建和清理独立测试目录。

```powershell
$env:SERIAL_TERMINAL_SMOKE_DEVICE = '2253005'
cargo test --manifest-path ratatui/Cargo.toml adb_shell_and_file_operations -- --ignored
```
