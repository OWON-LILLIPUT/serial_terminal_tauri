use crate::{
    assign_child_to_cleanup_job, ensure_adb_server_started, hide_command_window, run_android_task,
    terminate_child_with_timeout, AndroidManager,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

const CANCELLED: &str = "已取消传输";
const FILE_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const FILE_SYNC_TIMEOUT: Duration = Duration::from_secs(120);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
pub struct FileManager {
    transfers: Mutex<HashMap<String, Arc<AtomicBool>>>,
    closing: AtomicBool,
}

impl FileManager {
    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        if let Ok(transfers) = self.transfers.lock() {
            for flag in transfers.values() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub permissions: String,
}

#[derive(Serialize)]
pub struct DirectoryListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Serialize)]
pub struct LocalSelection {
    path: String,
    name: String,
    is_dir: bool,
    exists: bool,
    size: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TransferRequest {
    id: String,
    device_id: String,
    direction: String,
    remote_path: String,
    local_path: String,
    overwrite: bool,
}

#[derive(Clone, Serialize)]
pub struct TransferProgress {
    pub id: String,
    pub device_id: String,
    pub phase: String,
    pub message: String,
    pub percent: Option<u8>,
    pub bytes: u64,
    pub total: Option<u64>,
    pub elapsed_ms: u64,
}

pub fn normalize_path(path: &str) -> Result<String, String> {
    if !path.starts_with('/') || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err("请输入有效的设备绝对路径（以 / 开头，不含控制字符）".into());
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.contains('/')
        || name.chars().any(char::is_control)
        || name.len() > 255
    {
        return Err("名称不能为空，不能含 /、控制字符，也不能是 . 或 ..".into());
    }
    Ok(())
}

fn mutable_path(path: &str) -> Result<String, String> {
    let path = normalize_path(path)?;
    if matches!(
        path.as_str(),
        "/" | "/sdcard"
            | "/storage"
            | "/data"
            | "/system"
            | "/vendor"
            | "/dev"
            | "/proc"
            | "/sys"
            | "/etc"
            | "/mnt"
            | "/apex"
            | "/product"
            | "/odm"
    ) {
        return Err("不能删除、重命名或覆盖设备根目录/关键系统目录".into());
    }
    Ok(path)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn join_remote(parent: &str, name: &str) -> Result<String, String> {
    validate_name(name)?;
    normalize_path(&format!(
        "{}/{name}",
        normalize_path(parent)?.trim_end_matches('/')
    ))
}

fn device_id(value: &str) -> Result<String, String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("无效的 ADB 设备 ID".into());
    }
    Ok(value.to_string())
}

fn run_shell(
    app: &AppHandle,
    manager: &AndroidManager,
    device: &str,
    script: &str,
) -> Result<Vec<u8>, String> {
    run_shell_with_timeout(app, manager, device, script, FILE_COMMAND_TIMEOUT)
}

fn run_shell_with_timeout(
    app: &AppHandle,
    manager: &AndroidManager,
    device: &str,
    script: &str,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let files = app.state::<FileManager>();
    if files.closing.load(Ordering::SeqCst) {
        return Err("软件正在关闭".into());
    }
    let device = device_id(device)?;
    let adb = ensure_adb_server_started(app, manager)?;
    let mut command = Command::new(&adb);
    command.args([
        "-s",
        &device,
        "shell",
        "-T",
        "sh",
        "-c",
        &shell_quote(script),
    ]);
    command.current_dir(adb.parent().unwrap_or(Path::new(".")));
    hide_command_window(&mut command);
    let output =
        capture_file_command_with_timeout(&mut command, &files.closing, timeout, |child| {
            assign_child_to_cleanup_job(manager, child, "设备文件操作")
        })?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr);
        let details = if details.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout)
        } else {
            details
        };
        return Err(format!(
            "设备文件操作失败（请检查连接和目录权限）: {}",
            details.trim()
        ));
    }
    Ok(output.stdout)
}

// RAII also covers errors during pipe setup/Job assignment, not only normal completion.
struct FileChild(Child);
impl std::ops::Deref for FileChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for FileChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for FileChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            terminate_child_with_timeout(&mut self.0, Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
fn capture_file_command(
    command: &mut Command,
    closing: &AtomicBool,
    assign: impl FnOnce(&mut Child) -> Result<(), String>,
) -> Result<Output, String> {
    capture_file_command_with_timeout(command, closing, FILE_COMMAND_TIMEOUT, assign)
}

fn capture_file_command_with_timeout(
    command: &mut Command,
    closing: &AtomicBool,
    timeout: Duration,
    assign: impl FnOnce(&mut Child) -> Result<(), String>,
) -> Result<Output, String> {
    check_cancel(closing)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = FileChild(command.spawn().map_err(|e| e.to_string())?);
    assign(&mut child)?;
    let (out_tx, out_rx) = mpsc::sync_channel(32);
    let (err_tx, err_rx) = mpsc::sync_channel(32);
    let reader1 = output_reader(child.stdout.take().ok_or("目录输出不可用")?, out_tx);
    let reader2 = output_reader(child.stderr.take().ok_or("目录错误输出不可用")?, err_tx);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let started = Instant::now();
    let result = loop {
        for chunk in out_rx.try_iter().take(64) {
            stdout.extend_from_slice(&chunk);
        }
        for chunk in err_rx.try_iter().take(64) {
            stderr.extend_from_slice(&chunk);
            if stderr.len() > 16384 {
                stderr.drain(..stderr.len() - 16384);
            }
        }
        if stdout.len() > 16 * 1024 * 1024 {
            break Err("目录数据过大，请直接输入子目录路径".into());
        }
        if closing.load(Ordering::SeqCst) {
            break Err("软件正在关闭".into());
        }
        if started.elapsed() > timeout {
            break Err("设备文件操作超时，请检查设备连接和目录权限".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // Pipe readers still need to drain buffered bytes after the child exits.
                while let Ok(chunk) = out_rx.recv_timeout(Duration::from_millis(100)) {
                    stdout.extend_from_slice(&chunk);
                    if stdout.len() > 16 * 1024 * 1024 {
                        break;
                    }
                }
                while let Ok(chunk) = err_rx.recv_timeout(Duration::from_millis(100)) {
                    stderr.extend_from_slice(&chunk);
                    if stderr.len() > 16384 {
                        stderr.drain(..stderr.len() - 16384);
                    }
                }
                break if stdout.len() > 16 * 1024 * 1024 {
                    Err("目录数据过大".into())
                } else {
                    Ok(Output {
                        status,
                        stdout,
                        stderr,
                    })
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => break Err(error.to_string()),
        }
    };
    if result.is_err() {
        terminate_child_with_timeout(&mut child, Duration::from_millis(500));
    }
    drop(out_rx);
    drop(err_rx);
    let _ = reader1.join();
    let _ = reader2.join();
    result
}

fn listing_script(path: &str) -> String {
    format!(
        r#"cd -- {} || exit 1
[ -r . ] && [ -x . ] || {{ echo 'Permission denied' >&2; exit 1; }}
printf 'STFILES3\000'
# find batches argv without a per-file shell loop/test/command substitution.
# Names remain NUL-delimited; stat prints numeric fields only, in the same argv order.
find . -mindepth 1 -maxdepth 1 -exec sh -c '
  printf "%s\000" "$#" "$@"
  stat -c "%s %Y %a %f" -- "$@" || exit 1
  printf "\000"
' sh {{}} + || exit 1
printf 'DIRS\000'
find -L . -mindepth 1 -maxdepth 1 -type d -print0"#,
        shell_quote(path)
    )
}

fn parse_listing(path: &str, output: &[u8]) -> Result<DirectoryListing, String> {
    if output.len() > 16 * 1024 * 1024 {
        return Err("目录过大，请直接输入子目录路径".into());
    }
    if output.starts_with(b"STFILES2\0") || output.starts_with(b"STFILES3\0") {
        return parse_batched_listing(path, output);
    }
    let mut fields = output.split(|byte| *byte == 0);
    if fields.next() != Some(b"STFILES1".as_slice()) {
        return Err("设备未返回有效目录数据（需要支持 stat 的 Android shell）".into());
    }
    let mut entries = Vec::new();
    loop {
        let name = match fields.next() {
            None | Some([]) => break,
            Some(name) => name,
        };
        let name =
            std::str::from_utf8(name).map_err(|_| "设备目录含非 UTF-8 文件名，无法安全操作")?;
        validate_name(name)?;
        let metadata = fields.next().ok_or("目录数据不完整")?;
        let metadata = std::str::from_utf8(metadata).map_err(|_| "目录元数据无效")?;
        let metadata: Vec<&str> = metadata.split_whitespace().collect();
        if metadata.len() != 3 {
            return Err("设备 stat 返回格式不兼容".into());
        }
        let kind = fields.next().ok_or("目录类型数据不完整")?;
        let kind = std::str::from_utf8(kind).map_err(|_| "目录类型无效")?;
        let (kind, is_dir) = kind.split_once(' ').ok_or("目录类型无效")?;
        if !matches!(kind, "directory" | "file" | "symlink" | "other")
            || !matches!(is_dir, "0" | "1")
        {
            return Err("目录类型无效".into());
        }
        entries.push(FileEntry {
            name: name.into(),
            path: join_remote(path, name)?,
            kind: kind.into(),
            is_dir: is_dir == "1",
            size: metadata[0].parse().map_err(|_| "文件大小无效")?,
            modified: metadata[1].parse().map_err(|_| "修改时间无效")?,
            permissions: metadata[2].into(),
        });
        if entries.len() > 20000 {
            return Err("目录项目过多，请直接输入子目录路径".into());
        }
    }
    entries.sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    Ok(DirectoryListing {
        path: path.into(),
        entries,
    })
}

fn parse_batched_listing(path: &str, output: &[u8]) -> Result<DirectoryListing, String> {
    if !output.ends_with(b"\0") {
        return Err("目录数据不完整，请重试刷新".into());
    }
    let fast = output.starts_with(b"STFILES3\0");
    let mut fields = output.split(|byte| *byte == 0);
    fields.next();
    let mut entries = Vec::new();
    let mut followed_directories = std::collections::HashSet::new();
    let mut has_directories = false;
    loop {
        let count = match fields.next() {
            None | Some([]) => break,
            Some(count) => count,
        };
        if fast && count == b"DIRS" {
            has_directories = true;
            for directory in fields {
                if directory.is_empty() {
                    continue;
                }
                let directory = std::str::from_utf8(directory).map_err(|_| "目录名称编码无效")?;
                let directory = directory.strip_prefix("./").ok_or("目录名称无效")?;
                validate_name(directory)?;
                followed_directories.insert(directory.to_string());
            }
            break;
        }
        let count = std::str::from_utf8(count)
            .map_err(|_| "目录批次无效")?
            .parse::<usize>()
            .map_err(|_| "目录批次无效")?;
        if !(1..=20000).contains(&count) {
            return Err("目录批次大小无效".into());
        }
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let name = std::str::from_utf8(fields.next().ok_or("文件名不完整")?)
                .map_err(|_| "设备目录含非 UTF-8 文件名，无法安全操作")?;
            let name = if fast {
                name.strip_prefix("./").ok_or("目录文件名无效")?
            } else {
                name
            };
            validate_name(name)?;
            let linkdir = if fast {
                false
            } else {
                let value = fields.next().ok_or("目录类型不完整")?;
                if !matches!(value, b"0" | b"1") {
                    return Err("目录类型无效".into());
                }
                value == b"1"
            };
            names.push((name, linkdir));
        }
        let metadata = std::str::from_utf8(fields.next().ok_or("目录元数据不完整")?)
            .map_err(|_| "目录元数据无效")?;
        let rows: Vec<&str> = metadata.lines().collect();
        if rows.len() != count {
            return Err("设备 stat 返回数量不匹配，请重试刷新".into());
        }
        for ((name, linkdir), row) in names.into_iter().zip(rows) {
            let values: Vec<&str> = row.split_whitespace().collect();
            if values.len() != 4 {
                return Err("设备 stat 返回格式不兼容".into());
            }
            let mode = u32::from_str_radix(values[3], 16).map_err(|_| "文件类型无效")?;
            let kind = match mode & 0xf000 {
                0x4000 => "directory",
                0x8000 => "file",
                0xa000 => "symlink",
                _ => "other",
            };
            entries.push(FileEntry {
                name: name.into(),
                path: join_remote(path, name)?,
                kind: kind.into(),
                is_dir: kind == "directory" || (kind == "symlink" && linkdir),
                size: values[0].parse().map_err(|_| "文件大小无效")?,
                modified: values[1].parse().map_err(|_| "修改时间无效")?,
                permissions: values[2].into(),
            });
        }
        if entries.len() > 20000 {
            return Err("目录项目过多，请直接输入子目录路径".into());
        }
    }
    if fast && !has_directories {
        return Err("目录类型数据不完整，请重试刷新".into());
    }
    for entry in &mut entries {
        if entry.kind == "symlink" && followed_directories.contains(&entry.name) {
            entry.is_dir = true;
        }
    }
    entries.sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    Ok(DirectoryListing {
        path: path.into(),
        entries,
    })
}

#[tauri::command]
pub async fn sync_device_files(app: AppHandle, device_id: String) -> Result<(), String> {
    run_android_task(app, move |app, manager| {
        // Flush OS filesystem caches on the tab's device. This is not `adb sync` (host upload).
        run_shell_with_timeout(app, manager, &device_id, "sync", FILE_SYNC_TIMEOUT).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn list_device_directory(
    app: AppHandle,
    device_id: String,
    path: String,
) -> Result<DirectoryListing, String> {
    run_android_task(app, move |app, manager| {
        let path = normalize_path(&path)?;
        let output = run_shell(app, manager, &device_id, &listing_script(&path))?;
        parse_listing(&path, &output)
    })
    .await
}

fn mutation_script(
    action: &str,
    path: &str,
    name: &str,
    confirmed: bool,
) -> Result<String, String> {
    let path = normalize_path(path)?;
    match action {
        "mkdir" => Ok(format!(
            "mkdir -- {}",
            shell_quote(&join_remote(&path, name)?)
        )),
        "rename" => {
            let source = mutable_path(&path)?;
            let parent = source
                .rsplit_once('/')
                .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
                .unwrap();
            let target = mutable_path(&join_remote(parent, name)?)?;
            if source == target {
                return Err("名称没有变化".into());
            }
            Ok(format!("if [ -e {to} ] || [ -L {to} ]; then echo '目标已存在，未覆盖' >&2; exit 1; fi; mv -n -T -- {from} {to} || exit 1; if [ -e {from} ] || [ -L {from} ]; then echo '目标已存在，未覆盖' >&2; exit 1; fi", to=shell_quote(&target), from=shell_quote(&source)))
        }
        "delete" => {
            if !confirmed {
                return Err("删除操作需要确认".into());
            }
            Ok(format!("rm -r -- {}", shell_quote(&mutable_path(&path)?)))
        }
        _ => Err("不支持的文件操作".into()),
    }
}

#[tauri::command]
pub async fn mutate_device_file(
    app: AppHandle,
    device_id: String,
    action: String,
    path: String,
    name: String,
    confirmed: bool,
) -> Result<(), String> {
    run_android_task(app, move |app, manager| {
        let script = mutation_script(&action, &path, &name, confirmed)?;
        run_shell(app, manager, &device_id, &script).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn choose_device_local_path(
    app: AppHandle,
    mode: String,
    name: String,
) -> Result<Option<LocalSelection>, String> {
    #[cfg(windows)]
    let owner = app
        .get_webview_window("main")
        .ok_or("主窗口不存在")?
        .hwnd()
        .map_err(|e| e.to_string())?
        .0 as isize;
    #[cfg(not(windows))]
    let owner = {
        let _ = app;
        0
    };
    tauri::async_runtime::spawn_blocking(move || {
        let Some(mut path) = crate::file_dialog::choose(owner, &mode, &name)? else {
            return Ok(None);
        };
        if mode == "download-directory" {
            validate_name(&name)?;
            if name.chars().any(|c| "<>:\"\\|?*".contains(c)) || name.ends_with(['.', ' ']) {
                return Err("设备目录名称不兼容 Windows，请先重命名或下载目录内的文件".into());
            }
            path = path.join(&name);
        }
        let path_text = path.to_str().ok_or("本地路径无法编码为 UTF-8")?.to_string();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("无效的本地文件名")?
            .to_string();
        let metadata = fs::metadata(&path).ok();
        Ok(Some(LocalSelection {
            path: path_text,
            name,
            is_dir: metadata.as_ref().is_some_and(|m| m.is_dir()),
            exists: path.exists(),
            size: metadata.map(|m| m.len()).unwrap_or(0),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
pub fn cancel_device_transfer(manager: State<'_, FileManager>, id: String) -> Result<(), String> {
    if let Some(flag) = manager
        .transfers
        .lock()
        .map_err(|_| "传输状态不可用")?
        .get(&id)
    {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub async fn transfer_device_file(app: AppHandle, request: TransferRequest) -> Result<(), String> {
    if request.id.len() < 8
        || request.id.len() > 64
        || !request
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("无效的传输 ID".into());
    }
    let flag = Arc::new(AtomicBool::new(false));
    {
        let manager = app.state::<FileManager>();
        if manager.closing.load(Ordering::SeqCst) {
            return Err("软件正在关闭".into());
        }
        let mut transfers = manager.transfers.lock().map_err(|_| "传输状态不可用")?;
        if transfers.contains_key(&request.id) {
            return Err("传输 ID 重复".into());
        }
        transfers.insert(request.id.clone(), Arc::clone(&flag));
    }
    let cleanup_app = app.clone();
    let cleanup_id = request.id.clone();
    let result = run_android_task(app, move |app, manager| {
        let started = Instant::now();
        let result = transfer_inner(app, manager, &request, &flag, started);
        let phase = match &result {
            Ok(_) => "completed",
            Err(error) if error.starts_with(CANCELLED) => "cancelled",
            _ => "failed",
        };
        let message = match &result {
            Ok(_) => "传输完成".to_string(),
            Err(error) => error.clone(),
        };
        emit_progress(
            app,
            &request,
            phase,
            &message,
            if result.is_ok() { Some(100) } else { None },
            0,
            None,
            started,
        );
        result
    })
    .await;
    if let Ok(mut transfers) = cleanup_app.state::<FileManager>().transfers.lock() {
        transfers.remove(&cleanup_id);
    }
    result
}

fn check_cancel(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::SeqCst) {
        Err(CANCELLED.into())
    } else {
        Ok(())
    }
}

fn transfer_inner(
    app: &AppHandle,
    manager: &AndroidManager,
    request: &TransferRequest,
    flag: &AtomicBool,
    started: Instant,
) -> Result<(), String> {
    let device = device_id(&request.device_id)?;
    let remote = if request.direction == "upload" {
        mutable_path(&request.remote_path)?
    } else {
        let path = normalize_path(&request.remote_path)?;
        if path == "/" {
            return Err("请下载具体目录，而不是整个设备根目录".into());
        }
        path
    };
    let local = PathBuf::from(&request.local_path);
    if !local.is_absolute() {
        return Err("本地路径必须为绝对路径".into());
    }
    check_cancel(flag)?;
    let adb = ensure_adb_server_started(app, manager)?;
    check_cancel(flag)?;
    if request.direction == "upload" {
        let metadata =
            fs::symlink_metadata(&local).map_err(|e| format!("读取上传文件失败: {e}"))?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err("只支持上传普通文件或目录".into());
        }
        let parent = remote
            .rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
            .unwrap();
        let stage = join_remote(parent, &format!(".serial-terminal-upload-{}", request.id))?;
        run_shell(
            app,
            manager,
            &device,
            &format!("mkdir -- {}", shell_quote(&stage)),
        )?;
        let result = (|| {
            check_cancel(flag)?;
            let mut command =
                transfer_command(&adb, &device, "push", &local.to_string_lossy(), &stage);
            wait_transfer(
                app,
                manager,
                request,
                &mut command,
                flag,
                None,
                if metadata.is_file() {
                    Some(metadata.len())
                } else {
                    None
                },
                started,
            )?;
            check_cancel(flag)?;
            let source_name = local
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("本地文件名编码无效")?;
            let payload = join_remote(&stage, source_name)?;
            let target = shell_quote(&remote);
            let guard = if request.overwrite && metadata.is_file() {
                format!("if [ -d {target} ] || [ -L {target} ]; then echo '不能覆盖目录或符号链接' >&2; exit 1; fi;")
            } else {
                format!("if [ -e {target} ] || [ -L {target} ]; then echo '目标已存在，未覆盖' >&2; exit 1; fi;")
            };
            run_shell(
                app,
                manager,
                &device,
                &format!("{guard} mv {} -T -- {source} {target} || exit 1; if [ -e {source} ] || [ -L {source} ]; then echo '目标已存在，未覆盖' >&2; exit 1; fi", if request.overwrite && metadata.is_file() { "-f" } else { "-n" }, source = shell_quote(&payload)),
            )?;
            Ok(())
        })();
        // Only remove the unique directory this operation successfully created; never the target.
        let cleanup = run_shell(
            app,
            manager,
            &device,
            &format!("rm -r -- {}", shell_quote(&stage)),
        );
        if cleanup.is_err() {
            return Err(match result {
                Ok(_) => format!("上传已完成，但临时目录清理失败: {stage}"),
                Err(error) => format!("{error}；临时目录清理失败，可能残留于 {stage}"),
            });
        }
        result
    } else if request.direction == "download" {
        let parent = local
            .parent()
            .ok_or("本地保存路径无效")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let filename = local.file_name().ok_or("本地保存文件名无效")?;
        let target = parent.join(filename);
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            if !request.overwrite || !metadata.is_file() {
                return Err("本地目标已存在，未覆盖（目录不合并覆盖）".into());
            }
        }
        let total = run_shell(
            app,
            manager,
            &device,
            &format!(
                "if [ -f {path} ]; then stat -L -c '%s' -- {path}; fi",
                path = shell_quote(&remote)
            ),
        )?
        .as_slice()
        .split(|b| *b == b'\n')
        .next()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| text.trim().parse::<u64>().ok());
        check_cancel(flag)?;
        let stage =
            LocalStage::new(parent.join(format!(".serial-terminal-download-{}", request.id)))?;
        let payload = stage.path.join("payload");
        let mut command =
            transfer_command(&adb, &device, "pull", &remote, &payload.to_string_lossy());
        wait_transfer(
            app,
            manager,
            request,
            &mut command,
            flag,
            Some(&payload),
            total,
            started,
        )?;
        check_cancel(flag)?;
        commit_download(&payload, &target, request.overwrite)?;
        Ok(())
    } else {
        Err("无效的传输方向".into())
    }
}

struct LocalStage {
    path: PathBuf,
}
impl LocalStage {
    fn new(path: PathBuf) -> Result<Self, String> {
        fs::create_dir(&path).map_err(|e| format!("创建下载临时目录失败: {e}"))?;
        Ok(Self { path })
    }
}
impl Drop for LocalStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn commit_download(payload: &Path, target: &Path, overwrite: bool) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(target) {
        if !overwrite || !metadata.is_file() || !payload.is_file() {
            return Err("本地目标已存在，未覆盖".into());
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = payload.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags = MOVEFILE_WRITE_THROUGH
            | if overwrite {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        // Without REPLACE_EXISTING the OS atomically refuses a target created after our check.
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
            return Err(format!(
                "保存下载文件失败: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(payload, target).map_err(|e| format!("保存下载文件失败: {e}"))
}

fn transfer_command(adb: &Path, device: &str, action: &str, from: &str, to: &str) -> Command {
    let mut command = Command::new(adb);
    command.args(["-s", device, action, from, to]);
    command.current_dir(adb.parent().unwrap_or(Path::new(".")));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_command_window(&mut command);
    command
}

fn output_reader<R: Read + Send + 'static>(
    mut reader: R,
    sender: mpsc::SyncSender<Vec<u8>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    })
}

fn progress_percent(text: &str) -> Option<u8> {
    let end = text.rfind("%]")?;
    let start = text[..end].rfind('[')?;
    text[start + 1..end]
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|p| *p <= 100)
}

#[allow(clippy::too_many_arguments)]
fn emit_progress(
    app: &AppHandle,
    request: &TransferRequest,
    phase: &str,
    message: &str,
    percent: Option<u8>,
    bytes: u64,
    total: Option<u64>,
    started: Instant,
) {
    let _ = app.emit(
        "device-file-transfer",
        TransferProgress {
            id: request.id.clone(),
            device_id: request.device_id.clone(),
            phase: phase.into(),
            message: message.into(),
            percent,
            bytes,
            total,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn wait_transfer(
    app: &AppHandle,
    manager: &AndroidManager,
    request: &TransferRequest,
    command: &mut Command,
    flag: &AtomicBool,
    download: Option<&Path>,
    total: Option<u64>,
    started: Instant,
) -> Result<(), String> {
    monitor_transfer(
        command,
        flag,
        download,
        started,
        TRANSFER_TIMEOUT,
        |child| assign_child_to_cleanup_job(manager, child, "设备文件传输"),
        |percent, bytes| {
            let percent = percent.or_else(|| {
                if download.is_some() {
                    total
                        .filter(|size| *size > 0)
                        .map(|size| (bytes.saturating_mul(100) / size).min(99) as u8)
                } else {
                    None
                }
            });
            emit_progress(
                app,
                request,
                "running",
                if request.direction == "upload" {
                    "正在上传"
                } else {
                    "正在下载"
                },
                percent,
                bytes,
                total,
                started,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn monitor_transfer(
    command: &mut Command,
    flag: &AtomicBool,
    download: Option<&Path>,
    started: Instant,
    timeout: Duration,
    assign: impl FnOnce(&mut std::process::Child) -> Result<(), String>,
    mut report: impl FnMut(Option<u8>, u64),
) -> Result<(), String> {
    check_cancel(flag)?;
    let mut child = FileChild(
        command
            .spawn()
            .map_err(|e| format!("无法启动文件传输: {e}"))?,
    );
    if let Err(error) = assign(&mut child) {
        terminate_child_with_timeout(&mut child, Duration::from_millis(500));
        return Err(error);
    }
    let (sender, receiver) = mpsc::sync_channel(32);
    let stdout = child.stdout.take().ok_or("传输输出不可用")?;
    let stderr = child.stderr.take().ok_or("传输错误输出不可用")?;
    let reader1 = output_reader(stdout, sender.clone());
    let reader2 = output_reader(stderr, sender);
    let mut output = Vec::new();
    let mut percent = None;
    let mut last_update = Instant::now() - Duration::from_secs(1);
    let result = loop {
        for chunk in receiver.try_iter().take(64) {
            output.extend_from_slice(&chunk);
            if output.len() > 16384 {
                output.drain(..output.len() - 16384);
            }
            percent = progress_percent(&String::from_utf8_lossy(&output)).or(percent);
        }
        if flag.load(Ordering::SeqCst) {
            break Err(CANCELLED.into());
        }
        if started.elapsed() > timeout {
            break Err("文件传输超过最长等待时间，已停止".into());
        }
        if last_update.elapsed() >= Duration::from_millis(200) {
            let bytes = download
                .and_then(|p| fs::metadata(p).ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .unwrap_or(0);
            report(percent, bytes);
            last_update = Instant::now();
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // Readers may still have the last error/progress bytes after process exit.
                while let Ok(chunk) = receiver.recv_timeout(Duration::from_millis(100)) {
                    output.extend_from_slice(&chunk);
                    if output.len() > 16384 {
                        output.drain(..output.len() - 16384);
                    }
                }
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "文件传输失败: {}",
                        String::from_utf8_lossy(&output).trim()
                    ))
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => break Err(format!("文件传输进程失败: {error}")),
        }
    };
    if result.is_err() {
        terminate_child_with_timeout(&mut child, Duration::from_millis(500));
    }
    drop(receiver);
    let _ = reader1.join();
    let _ = reader2.join();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_and_mutations_are_guarded() {
        assert_eq!(normalize_path("/sdcard/a/../b//").unwrap(), "/sdcard/b");
        for path in ["/", "/data/..", "/system/", "/sdcard"] {
            assert!(mutation_script("delete", path, "", true).is_err());
        }
        assert!(mutation_script("delete", "/sdcard/a", "", false).is_err());
        for name in ["", "..", "a/b", "a\nb"] {
            assert!(mutation_script("mkdir", "/sdcard", name, false).is_err());
        }
        let script =
            mutation_script("rename", "/sdcard/a'; echo hacked", "b $HOME", false).unwrap();
        assert!(script.contains("'\\''"));
        assert!(script.contains("目标已存在"));
        assert!(normalize_path("/sdcard/\0x").is_err());
    }
    #[test]
    fn parses_spaces_quotes_unicode_and_hidden_files() {
        let output = b"STFILES1\0a ' file.txt\00042 1700000000 644\0file 0\0.hidden\0000 1700000001 755\0directory 1\0";
        let listing = parse_listing("/sdcard", output).unwrap();
        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.entries[0].name, ".hidden");
        assert_eq!(listing.entries[1].path, "/sdcard/a ' file.txt");
        assert!(parse_listing("/", b"bad output").is_err());
        assert!(parse_listing("/", b"STFILES1\0a\0broken\0file 0\0").is_err());
        assert!(listing_script("/sdcard/a'b").contains("'\\''"));
        let unicode = "STFILES1\0中文 文件.txt\00042 1700000000 644\0file 0\0";
        assert_eq!(
            parse_listing("/sdcard", unicode.as_bytes())
                .unwrap()
                .entries[0]
                .name,
            "中文 文件.txt"
        );
        assert!(
            parse_listing("/sdcard", b"STFILES1\0\xff\00042 1700000000 644\0file 0\0").is_err()
        );
    }
    #[test]
    fn batched_listing_preserves_names_types_and_directory_links() {
        let output = "STFILES3\0\
            4\0./中文 ' $文件.txt\0./.hidden\0./sdcard\0./broken-link\0\
            42 1700000000 644 81a4\n0 1700000001 755 41ed\n7 1700000002 777 a1ff\n8 1700000003 777 a1ff\n\0\
            1\0./pipe\0\
            0 1700000004 600 1180\n\0DIRS\0./.hidden\0./sdcard\0";
        let listing = parse_listing("/", output.as_bytes()).unwrap();
        assert_eq!(
            parse_listing("/", output.replace('\n', "\r\n").as_bytes())
                .unwrap()
                .entries
                .len(),
            5
        );
        assert_eq!(listing.entries.len(), 5);
        assert_eq!(listing.entries[0].name, ".hidden");
        assert_eq!(listing.entries[1].name, "sdcard");
        assert!(listing.entries[1].is_dir);
        assert_eq!(listing.entries[1].kind, "symlink");
        let broken = listing
            .entries
            .iter()
            .find(|entry| entry.name == "broken-link")
            .unwrap();
        assert!(!broken.is_dir);
        let file = listing
            .entries
            .iter()
            .find(|entry| entry.kind == "file")
            .unwrap();
        assert_eq!(file.path, "/中文 ' $文件.txt");
        assert_eq!(file.size, 42);
        assert_eq!(file.modified, 1700000000);
        assert_eq!(file.permissions, "644");
        assert_eq!(
            listing
                .entries
                .iter()
                .find(|entry| entry.name == "pipe")
                .unwrap()
                .kind,
            "other"
        );
        assert!(parse_listing("/", b"STFILES3\0DIRS\0")
            .unwrap()
            .entries
            .is_empty());
    }

    #[test]
    fn batched_listing_handles_large_native_argv_batches() {
        let mut output = String::from("STFILES3\0");
        for (start, count) in [(0, 300), (300, 148)] {
            output.push_str(&format!("{count}\0"));
            for index in start..start + count {
                output.push_str(&format!("./file-{index:04}\0"));
            }
            for _ in 0..count {
                output.push_str("24 1700000000 644 81a4\n");
            }
            output.push('\0');
        }
        output.push_str("DIRS\0");
        let listing = parse_listing("/system/bin", output.as_bytes()).unwrap();
        assert_eq!(listing.entries.len(), 448);
        assert_eq!(listing.entries[447].path, "/system/bin/file-0447");
    }

    #[test]
    fn batched_listing_rejects_truncation_and_unsafe_metadata() {
        for output in [
            b"STFILES3\0DIRS".as_slice(),
            b"STFILES3\0000\0DIRS\0",
            b"STFILES3\020001\0DIRS\0",
            b"STFILES3\0001\0./a\0",
            b"STFILES3\0002\0./a\0./b\0001 2 644 81a4\n\0DIRS\0",
            b"STFILES3\0001\0./a\0001 2 644 nope\n\0DIRS\0",
            b"STFILES3\0001\0./a/b\0001 2 644 81a4\n\0DIRS\0",
            b"STFILES3\0001\0./a\nb\0001 2 644 81a4\n\0DIRS\0",
            b"STFILES3\0001\0./\xff\0001 2 644 81a4\n\0DIRS\0",
            b"STFILES3\0001\0./a\0001 2 644 81a4\n\0",
        ] {
            assert!(
                parse_listing("/", output).is_err(),
                "accepted malformed batched listing"
            );
        }
        assert!(listing_script("/").contains("sh {} +"));
        assert!(!listing_script("/").contains("$(stat"));
    }

    #[test]
    fn progress_is_optional_and_bounded() {
        assert_eq!(progress_percent("[ 42%] file"), Some(42));
        assert_eq!(progress_percent("[100%]"), Some(100));
        assert_eq!(progress_percent("[142%]"), None);
        assert_eq!(progress_percent("uploading"), None);
        assert_eq!(progress_percent("filename42%"), None);
        assert!(check_cancel(&AtomicBool::new(true)).is_err());
    }
    #[test]
    fn incomplete_download_does_not_replace_existing_file() {
        let root =
            std::env::temp_dir().join(format!("serial-terminal-files-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let target = root.join("target.txt");
        fs::write(&target, b"original").unwrap();
        {
            let stage = LocalStage::new(root.join("stage")).unwrap();
            let payload = stage.path.join("payload");
            fs::write(&payload, b"downloaded").unwrap();
            assert!(commit_download(&payload, &target, false).is_err());
            assert_eq!(fs::read(&target).unwrap(), b"original");
            commit_download(&payload, &target, true).unwrap();
        }
        assert_eq!(fs::read(&target).unwrap(), b"downloaded");
        assert!(!root.join("stage").exists());
        let existing_dir = root.join("existing-directory");
        fs::create_dir(&existing_dir).unwrap();
        fs::write(existing_dir.join("keep.txt"), b"keep").unwrap();
        {
            let stage = LocalStage::new(root.join("directory-stage")).unwrap();
            fs::create_dir(stage.path.join("payload")).unwrap();
            assert!(commit_download(&stage.path.join("payload"), &existing_dir, true).is_err());
            assert_eq!(fs::read(existing_dir.join("keep.txt")).unwrap(), b"keep");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shutdown_marks_every_active_transfer_cancelled() {
        let manager = FileManager::default();
        let flag1 = Arc::new(AtomicBool::new(false));
        let flag2 = Arc::new(AtomicBool::new(false));
        manager.transfers.lock().unwrap().extend([
            ("first".into(), flag1.clone()),
            ("second".into(), flag2.clone()),
        ]);
        manager.shutdown();
        assert!(manager.closing.load(Ordering::SeqCst));
        assert!(flag1.load(Ordering::SeqCst) && flag2.load(Ordering::SeqCst));
    }

    #[cfg(windows)]
    fn test_process(script: &str) -> Command {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        hide_command_window(&mut command);
        command
    }

    #[cfg(windows)]
    #[test]
    fn monitor_reads_progress_and_reports_process_failure() {
        let mut percentages = Vec::new();
        let mut command = test_process("[Console]::Out.Write('[ 42%]'); Start-Sleep -Milliseconds 300; [Console]::Error.Write('simulated permission failure'); exit 7");
        let error = monitor_transfer(
            &mut command,
            &AtomicBool::new(false),
            None,
            Instant::now(),
            Duration::from_secs(10),
            |_| Ok(()),
            |percent, _| percentages.push(percent),
        )
        .unwrap_err();
        assert!(error.contains("simulated permission failure"));
        assert!(percentages.contains(&Some(42)));
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_terminates_a_running_transfer_promptly() {
        let flag = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&flag);
        let setter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            cancel.store(true, Ordering::SeqCst);
        });
        let mut command = test_process("Start-Sleep -Seconds 20");
        let started = Instant::now();
        let result = monitor_transfer(
            &mut command,
            &flag,
            None,
            started,
            Duration::from_secs(30),
            |_| Ok(()),
            |_, _| {},
        );
        setter.join().unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(windows)]
    #[test]
    fn transfer_timeout_terminates_the_child() {
        let mut command = test_process("Start-Sleep -Seconds 20");
        let started = Instant::now();
        let result = monitor_transfer(
            &mut command,
            &AtomicBool::new(false),
            None,
            started,
            Duration::from_millis(150),
            |_| Ok(()),
            |_, _| {},
        );
        assert!(result.unwrap_err().contains("最长等待时间"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(windows)]
    #[test]
    fn file_commands_preserve_binary_stdout_and_separate_errors() {
        let mut command = test_process("[Console]::Out.Write('STFILES1'); [Console]::Out.Write([char]0); [Console]::Error.Write('diagnostic')");
        let output =
            capture_file_command(&mut command, &AtomicBool::new(false), |_| Ok(())).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"STFILES1\0");
        assert_eq!(output.stderr, b"diagnostic");
    }

    #[cfg(windows)]
    #[test]
    fn closing_interrupts_directory_commands() {
        let closing = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&closing);
        let setter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });
        let mut command = test_process("Start-Sleep -Seconds 20");
        let started = Instant::now();
        let error = capture_file_command(&mut command, &closing, |_| Ok(())).unwrap_err();
        setter.join().unwrap();
        assert!(error.contains("关闭"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(windows)]
    #[test]
    fn file_command_timeout_is_configurable_for_longer_sync_operations() {
        assert!(FILE_SYNC_TIMEOUT > FILE_COMMAND_TIMEOUT);
        let mut command = test_process("Start-Sleep -Seconds 20");
        let started = Instant::now();
        let error = capture_file_command_with_timeout(
            &mut command,
            &AtomicBool::new(false),
            Duration::from_millis(150),
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
