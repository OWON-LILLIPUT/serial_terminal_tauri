use crate::backend::{Adb, Result};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub size: u64,
    pub modified: i64,
    pub mode: String,
}

pub struct Browser {
    pub path: String,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub scroll: usize,
    pub marked: HashSet<String>,
    pub history: Vec<String>,
    pub history_index: usize,
    pub loading: bool,
    pub busy: bool,
    pub request: u64,
    pub expanded: HashSet<String>,
    pub directories: HashMap<String, Vec<Entry>>,
    pub tree_requests: HashMap<String, u64>,
    pub tree_scroll: usize,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            path: "/".into(),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            marked: HashSet::new(),
            history: Vec::new(),
            history_index: 0,
            loading: false,
            busy: false,
            request: 0,
            expanded: HashSet::from(["/".into()]),
            directories: HashMap::new(),
            tree_requests: HashMap::new(),
            tree_scroll: 0,
        }
    }
}

impl Browser {
    pub fn apply(&mut self, path: String, mut entries: Vec<Entry>, history: Option<usize>) {
        self.cache_directories(&path, &entries);
        if path != "/" {
            entries.insert(
                0,
                Entry {
                    name: "..".into(),
                    path: parent(&path).into(),
                    directory: true,
                    size: 0,
                    modified: 0,
                    mode: String::new(),
                },
            );
        }
        if let Some(index) = history {
            self.history_index = index;
        } else if self.history.get(self.history_index) != Some(&path) {
            self.history.truncate(self.history_index + 1);
            self.history.push(path.clone());
            self.history_index = self.history.len() - 1;
        }
        self.path = path;
        self.entries = entries;
        self.cursor = 0;
        self.scroll = 0;
        self.marked.clear();
    }

    pub fn selected(&self) -> Vec<Entry> {
        if self.marked.is_empty() {
            return self
                .entries
                .get(self.cursor)
                .filter(|entry| entry.name != "..")
                .cloned()
                .into_iter()
                .collect();
        }
        self.entries
            .iter()
            .filter(|entry| self.marked.contains(&entry.path))
            .cloned()
            .collect()
    }

    pub fn cache_directories(&mut self, path: &str, entries: &[Entry]) {
        self.directories.insert(
            path.to_owned(),
            entries
                .iter()
                .filter(|entry| entry.directory)
                .cloned()
                .collect(),
        );
    }

    pub fn tree(&self) -> Vec<(usize, &str, &str)> {
        let mut rows = Vec::new();
        let mut pending = vec![(0, "/", "/")];
        while let Some((depth, path, name)) = pending.pop() {
            rows.push((depth, path, name));
            if depth < 64 && self.expanded.contains(path) {
                if let Some(entries) = self.directories.get(path) {
                    pending.extend(
                        entries
                            .iter()
                            .rev()
                            .map(|entry| (depth + 1, entry.path.as_str(), entry.name.as_str())),
                    );
                }
            }
        }
        rows
    }
}

pub fn normalize_path(path: &str) -> Result<String> {
    if !path.starts_with('/') || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err("请输入有效的设备绝对路径".into());
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

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.len() > 255
        || name.contains('/')
        || name.chars().any(char::is_control)
    {
        return Err("名称不能为空、. 或 ..，也不能包含 / 与控制字符".into());
    }
    Ok(())
}

pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or(
        "/",
        |(parent, _)| if parent.is_empty() { "/" } else { parent },
    )
}

fn join(parent: &str, name: &str) -> Result<String> {
    validate_name(name)?;
    Ok(format!(
        "{}/{name}",
        normalize_path(parent)?.trim_end_matches('/')
    ))
}

fn mutable_path(path: &str) -> Result<String> {
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
        return Err("不能删除、重命名或覆盖设备根目录或关键系统目录".into());
    }
    Ok(path)
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell(adb: &Adb, device: &str, script: &str) -> Result<Vec<u8>> {
    adb.run(
        &["-s", device, "shell", "-T", "sh", "-c", &quote(script)],
        Duration::from_secs(10),
    )
}

pub fn list(adb: &Adb, device: &str, path: &str) -> Result<Vec<Entry>> {
    let path = normalize_path(path)?;
    let script = format!(
        r#"cd -- {} || exit 1
[ -r . ] && [ -x . ] || exit 1
printf 'STFILES3\000'
find . -mindepth 1 -maxdepth 1 -exec sh -c '
  printf "%s\000" "$#" "$@"
  stat -c "%s %Y %a %f" -- "$@" || exit 1
  printf "\000"
' sh {{}} + || exit 1
printf 'DIRS\000'
find -L . -mindepth 1 -maxdepth 1 -type d -print0 || exit 1
printf 'END\000'"#,
        quote(&path)
    );
    parse_listing(&path, &shell(adb, device, &script)?)
}

fn parse_listing(path: &str, data: &[u8]) -> Result<Vec<Entry>> {
    let data = std::str::from_utf8(data).map_err(|_| "目录包含无效 UTF-8 文件名")?;
    let mut fields = data.split('\0');
    if fields.next() != Some("STFILES3") {
        return Err("ADB 未返回有效目录数据".into());
    }
    let mut entries = Vec::new();
    let mut links = HashMap::new();
    loop {
        let token = fields.next().ok_or("目录数据不完整")?;
        if token == "DIRS" {
            break;
        }
        let count: usize = token.parse().map_err(|_| "目录批次无效")?;
        if count == 0 || count > 100_000 {
            return Err("目录批次超出限制".into());
        }
        let names = fields.by_ref().take(count).collect::<Vec<_>>();
        let metadata = fields.next().ok_or("目录元数据丢失")?;
        let lines = metadata.lines().collect::<Vec<_>>();
        if names.len() != count || lines.len() != count {
            return Err("目录数据不完整".into());
        }
        for (name, line) in names.into_iter().zip(lines) {
            let name = name.strip_prefix("./").ok_or("目录文件名格式无效")?;
            if validate_name(name).is_err() {
                continue;
            }
            let mut values = line.split_whitespace();
            let size = values
                .next()
                .and_then(|value| value.parse().ok())
                .ok_or("文件大小无效")?;
            let modified = values
                .next()
                .and_then(|value| value.parse().ok())
                .ok_or("修改时间无效")?;
            let mode = values.next().ok_or("缺少文件权限")?.to_owned();
            let kind = values
                .next()
                .and_then(|value| u32::from_str_radix(value, 16).ok())
                .ok_or("文件类型无效")?
                & 0xf000;
            if kind == 0xa000 {
                links.insert(name.to_owned(), entries.len());
            }
            entries.push(Entry {
                name: name.into(),
                path: join(path, name)?,
                directory: kind == 0x4000,
                size,
                modified,
                mode,
            });
        }
    }
    loop {
        let token = fields.next().ok_or("目录数据不完整")?;
        if token == "END" {
            break;
        }
        if let Some(index) = token.strip_prefix("./").and_then(|name| links.get(name)) {
            entries[*index].directory = true;
        }
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn mkdir(adb: &Adb, device: &str, path: &str, name: &str) -> Result<()> {
    shell(
        adb,
        device,
        &format!("mkdir -- {}", quote(&join(path, name)?)),
    )
    .map(|_| ())
}

fn no_overwrite_move(source: &str, target: &str) -> String {
    let source = quote(source);
    let target = quote(target);
    format!("if [ -e {target} ] || [ -L {target} ]; then echo '目标已存在' >&2; exit 1; fi; mv -n -T -- {source} {target} && [ ! -e {source} ] && [ ! -L {source} ]")
}

pub fn rename(adb: &Adb, device: &str, path: &str, name: &str) -> Result<()> {
    let source = mutable_path(path)?;
    let target = mutable_path(&join(parent(&source), name)?)?;
    shell(adb, device, &no_overwrite_move(&source, &target)).map(|_| ())
}

pub fn delete(adb: &Adb, device: &str, entries: &[Entry]) -> Result<()> {
    let paths = entries
        .iter()
        .map(|entry| mutable_path(&entry.path))
        .collect::<Result<Vec<_>>>()?;
    for path in paths {
        shell(adb, device, &format!("rm -r -- {}", quote(&path)))?;
    }
    Ok(())
}

fn local_text(path: &Path) -> Result<&str> {
    path.to_str().ok_or("本地路径不是有效 UTF-8".into())
}

fn local_directories(root: &Path) -> Result<Vec<String>> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(format!("上传目录包含重解析点：{}", path.display()));
            }
        }
        if metadata.is_dir() {
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            let parts = relative
                .iter()
                .map(|name| {
                    let name = name.to_str().ok_or("本地目录名称不是 UTF-8")?;
                    validate_name(name)?;
                    Ok(name)
                })
                .collect::<Result<Vec<_>>>()?;
            directories.push(parts.join("/"));
            for entry in fs::read_dir(&path).map_err(|error| error.to_string())? {
                pending.push(entry.map_err(|error| error.to_string())?.path());
            }
        } else if !metadata.is_file() {
            return Err(format!("只支持普通文件和目录：{}", path.display()));
        }
    }
    Ok(directories)
}

fn restore_remote_directories(
    adb: &Adb,
    device: &str,
    root: &str,
    directories: &[String],
) -> Result<()> {
    const PREFIX: &str = "mkdir -p --";
    let mut script = PREFIX.to_owned();
    for directory in directories {
        let path = quote(&format!("{root}/{directory}"));
        if script.len() + path.len() > 6500 && script.len() > PREFIX.len() {
            shell(adb, device, &script)?;
            script.truncate(PREFIX.len());
        }
        script.push(' ');
        script.push_str(&path);
    }
    if script.len() > PREFIX.len() {
        shell(adb, device, &script)?;
    }
    Ok(())
}

pub fn upload(adb: &Adb, device: &str, path: &str, files: &[(PathBuf, String)]) -> Result<()> {
    let mut destinations = HashSet::new();
    let mut sources = Vec::with_capacity(files.len());
    for (local, name) in files {
        let directories = local_directories(local)?;
        let target = mutable_path(&join(path, name)?)?;
        if !destinations.insert(target.clone()) {
            return Err(format!("上传目标重复：{name}"));
        }
        sources.push((local_text(local)?, target, directories));
    }
    for (local, target, directories) in sources {
        let template = format!(
            "{}/.serial-terminal-upload.XXXXXX",
            path.trim_end_matches('/')
        );
        let stage = shell(adb, device, &format!("mktemp -d {}", quote(&template)))?;
        let stage = std::str::from_utf8(&stage)
            .map_err(|_| "设备临时路径无效")?
            .trim();
        if !stage.starts_with(template.trim_end_matches('X')) || parent(stage) != path {
            return Err("设备未返回预期的临时目录".into());
        }
        let payload = join(stage, "payload")?;
        let result = adb
            .run(
                &["-s", device, "push", local, &payload],
                Duration::from_secs(1200),
            )
            .and_then(|_| restore_remote_directories(adb, device, &payload, &directories))
            .and_then(|_| shell(adb, device, &no_overwrite_move(&payload, &target)));
        let cleanup = shell(adb, device, &format!("rm -r -- {}", quote(stage)));
        result?;
        cleanup?;
    }
    Ok(())
}

fn download_directory(adb: &Adb, device: &str, remote: &str, payload: &Path) -> Result<()> {
    let script = format!(
        r#"cd -- {} || exit 1
find . -type d -print0 || exit 1
printf 'FILES\000'
find . -type f -print0 || exit 1
printf 'SPECIAL\000'
find . ! -type d ! -type f -print0 || exit 1
printf 'END\000'"#,
        quote(remote)
    );
    let bytes = shell(adb, device, &script)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "设备包含非 UTF-8 路径")?;
    let mut entries = text.split_terminator('\0');
    let mut seen = HashSet::new();
    let mut local_target = |path: &str| -> Result<PathBuf> {
        let key = if cfg!(windows) {
            path.to_lowercase()
        } else {
            path.to_owned()
        };
        if !seen.insert(key) {
            return Err(format!("设备名称在本地发生冲突：{path}"));
        }
        if path == "." {
            return Ok(payload.to_owned());
        }
        let path = path.strip_prefix("./").ok_or("设备目录路径无效")?;
        path.split('/')
            .try_fold(payload.to_owned(), |parent, name| {
                destination(&parent, name)
            })
    };
    let directories = entries
        .by_ref()
        .take_while(|path| *path != "FILES")
        .map(&mut local_target)
        .collect::<Result<Vec<_>>>()?;
    let files = entries
        .by_ref()
        .take_while(|path| *path != "SPECIAL")
        .map(|name| Ok((name, local_target(name)?)))
        .collect::<Result<Vec<_>>>()?;
    if entries.next() != Some("END") || entries.next().is_some() || !text.ends_with('\0') {
        return Err("下载目录包含特殊文件，或设备目录清单不完整".into());
    }

    // Windows adb's recursive pull can corrupt Unicode descendant names.
    let individual = cfg!(windows)
        && (files.iter().any(|(name, _)| !name.is_ascii())
            || directories.iter().any(|path| {
                !path
                    .strip_prefix(payload)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .is_ascii()
            }));
    if !individual && !files.is_empty() {
        pull(adb, device, remote, payload)?;
        local_directories(payload)?;
    }
    for directory in directories {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    if individual {
        for (name, local) in files {
            let relative = name.strip_prefix("./").ok_or("设备文件路径无效")?;
            pull(adb, device, &format!("{remote}/{relative}"), &local)?;
        }
    }
    Ok(())
}

fn pull(adb: &Adb, device: &str, remote: &str, target: &Path) -> Result<()> {
    adb.run(
        &["-s", device, "pull", remote, local_text(target)?],
        Duration::from_secs(1200),
    )
    .map(|_| ())
}

pub fn download(adb: &Adb, device: &str, entry: &Entry, target: &Path) -> Result<()> {
    if entry.path == "/" || entry.name == ".." {
        return Err("请选择具体文件或目录".into());
    }
    if fs::symlink_metadata(target).is_ok() {
        return Err(format!("本地目标已存在：{}", target.display()));
    }
    let directory = target
        .parent()
        .filter(|path| path.is_dir())
        .ok_or("本地目标目录不存在")?;
    let stage = tempfile::Builder::new()
        .prefix(".serial-terminal-")
        .tempdir_in(directory)
        .map_err(|error| error.to_string())?;
    let payload = stage.path().join("payload");
    if entry.directory {
        download_directory(adb, device, &entry.path, &payload)?;
    } else {
        pull(adb, device, &entry.path, &payload)?;
    }
    move_local(&payload, target)
}

#[cfg(windows)]
fn move_local(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileW;
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe { MoveFileW(source.as_ptr(), destination.as_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn move_local(source: &Path, destination: &Path) -> Result<()> {
    if fs::symlink_metadata(destination).is_ok() {
        return Err("本地目标已存在".into());
    }
    fs::rename(source, destination).map_err(|error| error.to_string())
}

pub fn local_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .ok_or("无法读取本地文件名".into())
}

pub fn destination(directory: &Path, name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    #[cfg(windows)]
    {
        let base = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if name.contains(['\\', ':', '*', '?', '"', '<', '>', '|'])
            || name.ends_with(['.', ' '])
            || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && base.as_bytes()[3].is_ascii_digit())
        {
            return Err(format!("文件名不能直接用于 Windows：{name}"));
        }
    }
    Ok(directory.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_hidden_names_and_directory_symlinks() {
        let bytes = b"STFILES3\x003\x00./.hidden\x00./folder\x00./link\x004 20 644 81a4\n0 21 755 41ed\n5 22 777 a1ff\n\x00DIRS\x00./folder\x00./link\x00END\x00";
        let rows = parse_listing("/sdcard", bytes).unwrap();
        assert_eq!(rows[0].path, "/sdcard/folder");
        assert!(rows[1].directory);
        assert_eq!(rows[2].name, ".hidden");
    }

    #[test]
    fn critical_paths_and_unfinished_batches_are_rejected() {
        assert!(mutable_path("/sdcard/..").is_err());
        assert!(parse_listing("/", b"STFILES3\x00DIRS\x00").is_err());
        assert_eq!(quote("a'b"), "'a'\\''b'");
    }
}
