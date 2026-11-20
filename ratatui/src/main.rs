#[path = "../../src-tauri/src/adb_server.rs"]
mod adb_server;
#[path = "../../src-tauri/src/adb_shell_protocol.rs"]
mod adb_shell_protocol;
#[path = "../../src-tauri/src/file_dialog.rs"]
mod file_dialog;
#[cfg(windows)]
#[path = "../../src-tauri/src/windows_serial.rs"]
mod windows_serial;

mod app;
mod backend;
mod files;
#[cfg(test)]
mod live_tests;
mod settings;
mod terminal;
mod ui;

use settings::SerialConfig;
const SERIAL_READ_TIMEOUT_MS: u64 = 5;

fn main() -> std::io::Result<()> {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("Serial Terminal · Ratatui\n\n启动：serial-terminal-ratatui\nCtrl+F1 帮助 · Ctrl+F2 功能区 · Ctrl+F3 内容 · Ctrl+F4/F5 切换标签 · Ctrl+F10 退出");
        return Ok(());
    }
    ratatui::run(app::run)
}
