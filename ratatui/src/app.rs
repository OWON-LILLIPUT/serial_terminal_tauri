use crate::{
    adb_shell_protocol::TerminalSize,
    backend::{self, Adb, Connection, Device, ManagedChild, Result, SessionEvent},
    files::{self, Browser, Entry},
    settings::Settings,
    terminal::{self, Session},
    ui,
};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
};
use ratatui::DefaultTerminal;
use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Serial,
    Shell(String),
    Files(String),
}

#[derive(Clone, Copy)]
pub enum FileAction {
    Refresh,
    Path,
    Upload,
    UploadFolder,
    Download,
    Mkdir,
    Rename,
    Delete,
    Sync,
    Copy,
    Paste,
}

#[derive(Clone)]
pub enum Action {
    Ports,
    Port,
    Baud,
    DataBits,
    Parity,
    StopBits,
    Serial,
    SerialSync,
    Devices,
    Device,
    Address,
    Remote,
    Codec,
    Bitrate,
    Scrcpy,
    Shell,
    Files,
    Root,
    Remount,
    Sync,
    File(FileAction),
    Clear,
    CopyText,
    PasteText,
    Cancel,
    SetPort(String),
    SetBaud(u32),
    SetBits(u8),
    SetParity(String),
    SetStop(u8),
    SetDevice(String),
    SetCodec(String),
    Delete(Vec<Entry>),
}

#[derive(Clone)]
pub enum Field {
    Address,
    Bitrate,
    Path,
    Mkdir,
    Rename(Entry),
    UploadPath,
    DownloadPath(Vec<Entry>),
}

pub struct UploadNames {
    pub files: Vec<(PathBuf, String)>,
    pub index: usize,
}

pub enum Modal {
    Pick {
        title: String,
        options: Vec<(String, Action)>,
        index: usize,
    },
    Edit {
        title: String,
        value: String,
        field: Field,
    },
    Upload(UploadNames),
    Help,
}

enum JobEvent {
    Devices(Result<Vec<Device>>),
    Ports(Result<Vec<String>>),
    Opened(Tab, u64, Result<Connection>),
    Status(Result<String>),
    Remote(String, bool, Result<String>),
    Scrcpy(String, Result<ManagedChild>),
    Sized(u64, TerminalSize, Result<()>),
    Listing(String, u64, String, Option<usize>, Result<Vec<Entry>>),
    TreeListing(String, u64, String, Result<Vec<Entry>>),
    FileDone(String, Result<String>),
    UploadPicked(String, Result<Option<PathBuf>>),
    Clipboard(String, Result<(tempfile::TempDir, Vec<String>)>),
}

pub struct App {
    pub settings: Settings,
    pub ports: Vec<String>,
    pub devices: Vec<Device>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub sidebar: bool,
    pub control: usize,
    pub control_scroll: usize,
    pub serial: Session,
    pub shells: HashMap<String, Session>,
    pub files: HashMap<String, Browser>,
    pub scrcpy: HashMap<String, ManagedChild>,
    pub modal: Option<Modal>,
    pub status: String,
    pub error: bool,
    pub quitting: bool,
    adb: Arc<Adb>,
    pending_scrcpy: HashSet<String>,
    next_id: u64,
    devices_loading: bool,
    next_refresh: Instant,
    next_serial_size: Instant,
    size_inflight: bool,
    size_attempt: Option<(u64, TerminalSize)>,
    serial_input_waiting: Vec<u8>,
    remote_busy: bool,
    file_drag_anchor: Option<(String, usize)>,
    last_file_click: Option<(String, usize, Instant)>,
    jobs: Sender<JobEvent>,
    job_events: Receiver<JobEvent>,
    io: Sender<SessionEvent>,
    io_events: Receiver<SessionEvent>,
    _clipboard: Option<tempfile::TempDir>,
}

impl App {
    pub(crate) fn new() -> Self {
        let (jobs, job_events) = mpsc::channel();
        let (io, io_events) = mpsc::channel();
        let now = Instant::now();
        Self {
            settings: Settings::load(),
            ports: Vec::new(),
            devices: Vec::new(),
            tabs: vec![Tab::Serial],
            active: 0,
            sidebar: true,
            control: 0,
            control_scroll: 0,
            serial: Session::new(0, TerminalSize { rows: 24, cols: 80 }),
            shells: HashMap::new(),
            files: HashMap::new(),
            scrcpy: HashMap::new(),
            modal: None,
            status: "就绪".into(),
            error: false,
            quitting: false,
            adb: Arc::new(Adb::default()),
            pending_scrcpy: HashSet::new(),
            next_id: 1,
            devices_loading: false,
            next_refresh: now,
            next_serial_size: now,
            size_inflight: false,
            size_attempt: None,
            serial_input_waiting: Vec::new(),
            remote_busy: false,
            file_drag_anchor: None,
            last_file_click: None,
            jobs,
            job_events,
            io,
            io_events,
            _clipboard: None,
        }
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn session(&self) -> Option<&Session> {
        match self.active_tab() {
            Tab::Serial => Some(&self.serial),
            Tab::Shell(device) => self.shells.get(device),
            Tab::Files(_) => None,
        }
    }

    fn session_mut(&mut self) -> Option<&mut Session> {
        match self.active_tab().clone() {
            Tab::Serial => Some(&mut self.serial),
            Tab::Shell(device) => self.shells.get_mut(&device),
            Tab::Files(_) => None,
        }
    }

    fn by_generation(&mut self, id: u64) -> Option<&mut Session> {
        if self.serial.id == id {
            return Some(&mut self.serial);
        }
        self.shells.values_mut().find(|session| session.id == id)
    }

    pub fn selected_device(&self) -> Option<&Device> {
        self.devices
            .iter()
            .find(|device| device.id == self.settings.adb_device)
    }

    fn online_device(&self) -> Result<String> {
        self.selected_device()
            .filter(|device| device.online())
            .map(|device| device.id.clone())
            .ok_or("请选择在线的 ADB 设备".into())
    }

    fn save(&mut self) {
        if let Err(error) = self.settings.save() {
            self.result(Err(format!("保存配置失败：{error}")));
        }
    }

    fn result(&mut self, result: Result<String>) {
        self.error = result.is_err();
        self.status = result
            .unwrap_or_else(|error| error)
            .chars()
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .take(300)
            .collect();
        if self.status.is_empty() {
            self.status = "操作完成".into();
        }
    }

    fn job(&self, task: impl FnOnce(&Adb) -> JobEvent + Send + 'static) {
        let jobs = self.jobs.clone();
        let adb = self.adb.clone();
        thread::spawn(move || {
            let _ = jobs.send(task(&adb));
        });
    }

    fn generation(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn set_tab(&mut self, tab: Tab) {
        let index = self
            .tabs
            .iter()
            .position(|existing| *existing == tab)
            .unwrap_or_else(|| {
                self.tabs.push(tab);
                self.tabs.len() - 1
            });
        self.active = index;
        self.sidebar = false;
    }

    fn close_tab(&mut self) {
        match self.active_tab().clone() {
            Tab::Serial => return,
            Tab::Shell(id) => {
                self.shells.remove(&id);
            }
            Tab::Files(id) => {
                if self.files[&id].busy {
                    return;
                }
                self.files.remove(&id);
            }
        }
        self.tabs.remove(self.active);
        self.active = self.active.saturating_sub(1);
    }

    fn refresh_devices(&mut self) {
        if self.devices_loading {
            return;
        }
        self.devices_loading = true;
        self.next_refresh = Instant::now() + Duration::from_secs(5);
        self.job(|adb| JobEvent::Devices(adb.devices()));
    }

    fn refresh_ports(&self) {
        self.job(|_| JobEvent::Ports(backend::ports()));
    }

    fn serial_target(&self) -> Option<String> {
        if !self.settings.serial_sync || self.serial.connection.is_none() {
            return None;
        }
        let id = self
            .settings
            .serial_binding
            .as_ref()
            .filter(|(port, _)| port == &self.settings.serial.port_name)
            .map_or(self.settings.adb_device.as_str(), |(_, device)| {
                device.as_str()
            });
        self.devices
            .iter()
            .find(|device| device.id == id && device.online())
            .map(|device| device.id.clone())
    }

    fn tick(&mut self) {
        self.scrcpy.retain(|_, process| process.running());
        if Instant::now() >= self.next_refresh {
            self.refresh_devices();
        }
        let Some(device) = self.serial_target() else {
            self.size_attempt = None;
            self.flush_serial_input();
            return;
        };
        if self.size_inflight || Instant::now() < self.next_serial_size {
            return;
        }
        let (id, size) = (self.serial.id, self.serial.size);
        if self.size_attempt == Some((id, size)) {
            self.flush_serial_input();
            return;
        }
        if self
            .settings
            .serial_binding
            .as_ref()
            .is_none_or(|(port, _)| port != &self.settings.serial.port_name)
        {
            self.settings.serial_binding =
                Some((self.settings.serial.port_name.clone(), device.clone()));
            self.save();
        }
        self.size_inflight = true;
        self.size_attempt = Some((id, size));
        self.job(move |adb| JobEvent::Sized(id, size, adb.sync_serial(&device, size)));
    }

    fn send_key_bytes(&mut self, bytes: Vec<u8>) {
        if self.active_tab() == &Tab::Serial
            && self.serial_target().is_some()
            && (!self.serial_input_waiting.is_empty()
                || (bytes.contains(&9)
                    && (self.size_inflight
                        || self.size_attempt != Some((self.serial.id, self.serial.size)))))
        {
            self.serial_input_waiting.extend(bytes);
            self.next_serial_size = Instant::now();
        } else if let Some(session) = self.session_mut() {
            session.send(bytes);
        }
    }

    fn flush_serial_input(&mut self) {
        if !self.serial_input_waiting.is_empty() {
            self.serial
                .send(std::mem::take(&mut self.serial_input_waiting));
        }
    }

    fn layout_size(&mut self, size: TerminalSize) {
        if self.serial.size != size {
            let cleanup = self.serial_target().is_some();
            self.serial.resize(size, cleanup);
            self.next_serial_size = Instant::now() + Duration::from_millis(140);
        }
        for session in self.shells.values_mut() {
            if session.size != size {
                session.resize(size, false);
                session.send(Vec::new());
            }
        }
    }

    fn toggle_serial(&mut self) -> Result<()> {
        if self.serial.opening || self.serial.connection.take().is_some() {
            self.serial.id = 0;
            self.serial.opening = false;
            self.size_attempt = None;
            self.serial_input_waiting.clear();
            self.result(Ok("串口已关闭".into()));
            return Ok(());
        }
        if self.settings.serial.port_name.is_empty() {
            return Err("请选择串口".into());
        }
        let config = self.settings.serial.clone();
        let id = self.generation();
        self.serial = Session::new(id, self.serial.size);
        self.serial.opening = true;
        let events = self.io.clone();
        self.set_tab(Tab::Serial);
        self.result(Ok(format!("正在打开 {}", config.port_name)));
        self.job(move |_| {
            JobEvent::Opened(Tab::Serial, id, Connection::serial(config, id, events))
        });
        Ok(())
    }

    fn toggle_shell(&mut self) -> Result<()> {
        let device = self.settings.adb_device.clone();
        if let Some(session) = self.shells.get_mut(&device) {
            if session.opening || session.connection.take().is_some() {
                session.opening = false;
                session.id = 0;
                self.result(Ok("ADB Shell 已关闭".into()));
                return Ok(());
            }
        }
        let device = self.online_device()?;
        let id = self.generation();
        let size = self.serial.size;
        let mut session = Session::new(id, size);
        session.opening = true;
        self.shells.insert(device.clone(), session);
        self.set_tab(Tab::Shell(device.clone()));
        let events = self.io.clone();
        self.job(move |adb| {
            let connection = Connection::shell(adb, &device, size, id, events);
            JobEvent::Opened(Tab::Shell(device), id, connection)
        });
        Ok(())
    }

    fn toggle_scrcpy(&mut self) -> Result<()> {
        let id = self.settings.adb_device.clone();
        if self.scrcpy.remove(&id).is_some() {
            self.result(Ok(format!("已关闭 scrcpy · {id}")));
            return Ok(());
        }
        if self.pending_scrcpy.contains(&id) {
            return Ok(());
        }
        let id = self.online_device()?;
        self.pending_scrcpy.insert(id.clone());
        let (codec, bitrate) = (
            self.settings.video_codec.clone(),
            self.settings.video_bit_rate.clone(),
        );
        self.job(move |adb| {
            let result = adb.scrcpy(&id, &codec, &bitrate);
            JobEvent::Scrcpy(id, result)
        });
        Ok(())
    }

    fn remote(&mut self) -> Result<()> {
        if self.remote_busy {
            return Ok(());
        }
        let id = address(&self.settings.adb_address)?;
        let disconnect = self
            .devices
            .iter()
            .any(|device| device.id == id && device.online());
        self.remote_busy = true;
        self.result(Ok(format!(
            "{} {id}",
            if disconnect {
                "正在断开"
            } else {
                "正在连接"
            }
        )));
        self.job(move |adb| {
            let result = adb
                .execute(&[if disconnect { "disconnect" } else { "connect" }, &id])
                .and_then(|message| {
                    if !disconnect
                        && !message.lines().any(|line| {
                            line.starts_with("connected to ")
                                || line.starts_with("already connected to ")
                        })
                    {
                        return Err(message);
                    }
                    Ok(message)
                });
            JobEvent::Remote(id, disconnect, result)
        });
        Ok(())
    }

    fn picked(&mut self, title: &str, options: Vec<(String, Action)>) {
        self.modal = Some(Modal::Pick {
            title: title.into(),
            options,
            index: 0,
        });
    }

    fn edit(&mut self, title: &str, value: String, field: Field) {
        self.modal = Some(Modal::Edit {
            title: title.into(),
            value,
            field,
        });
    }

    fn action(&mut self, action: Action) {
        if let Err(error) = self.perform(action) {
            self.result(Err(error));
        }
    }

    fn perform(&mut self, action: Action) -> Result<()> {
        use Action::*;
        let serial_locked = self.serial.opening || self.serial.connection.is_some();
        match action {
            Ports => self.refresh_ports(),
            Devices => self.refresh_devices(),
            Port | Baud | DataBits | Parity | StopBits if serial_locked => {
                return Err("请先关闭串口".into())
            }
            Port => self.picked(
                "选择串口",
                self.ports
                    .iter()
                    .map(|port| (port.clone(), SetPort(port.clone())))
                    .collect(),
            ),
            Baud => self.picked(
                "波特率",
                [
                    110, 300, 600, 1200, 2400, 4800, 9600, 14400, 19200, 38400, 57600, 115200,
                    128000, 230400, 256000, 460800, 576000, 921600, 1_000_000, 1_500_000,
                    2_000_000, 3_000_000, 4_000_000,
                ]
                .into_iter()
                .map(|rate| (rate.to_string(), SetBaud(rate)))
                .collect(),
            ),
            DataBits => self.picked(
                "数据位",
                (5..=8)
                    .map(|bits| (bits.to_string(), SetBits(bits)))
                    .collect(),
            ),
            Parity => self.picked(
                "校验位",
                [("无", "none"), ("偶校验", "even"), ("奇校验", "odd")]
                    .into_iter()
                    .map(|(label, value)| (label.into(), SetParity(value.into())))
                    .collect(),
            ),
            StopBits => self.picked(
                "停止位",
                [1, 2]
                    .into_iter()
                    .map(|bits| (bits.to_string(), SetStop(bits)))
                    .collect(),
            ),
            SetPort(port) => {
                self.settings.serial.port_name = port;
                self.save();
            }
            SetBaud(rate) => {
                self.settings.serial.baud_rate = rate;
                self.save();
            }
            SetBits(bits) => {
                self.settings.serial.data_bits = bits;
                self.save();
            }
            SetParity(parity) => {
                self.settings.serial.parity = parity;
                self.save();
            }
            SetStop(bits) => {
                self.settings.serial.stop_bits = bits;
                self.save();
            }
            Serial => self.toggle_serial()?,
            SerialSync => {
                self.settings.serial_sync = !self.settings.serial_sync;
                if !self.settings.serial_sync {
                    self.settings.serial_binding = None;
                }
                self.size_attempt = None;
                self.save();
            }
            Device => {
                let mut options: Vec<_> = self
                    .devices
                    .iter()
                    .map(|device| {
                        (
                            format!("{} · {}", device.id, device_state(&device.state)),
                            SetDevice(device.id.clone()),
                        )
                    })
                    .collect();
                options.extend(
                    self.settings
                        .adb_history
                        .iter()
                        .filter(|id| !self.devices.iter().any(|device| &device.id == *id))
                        .map(|id| (format!("{id} · 历史"), SetDevice(id.clone()))),
                );
                self.picked("选择 ADB 设备", options);
            }
            SetDevice(id) => {
                if id.contains(':') {
                    self.settings.adb_address = id.clone();
                }
                self.settings.adb_device = id;
                self.save();
            }
            Address => self.edit(
                "远程 ADB 地址",
                self.settings.adb_address.clone(),
                Field::Address,
            ),
            Remote => self.remote()?,
            Codec | Bitrate
                if self.scrcpy.contains_key(&self.settings.adb_device)
                    || self.pending_scrcpy.contains(&self.settings.adb_device) =>
            {
                return Err("请先关闭当前设备的 scrcpy".into())
            }
            Codec => self.picked(
                "视频编码",
                ["h264", "h265"]
                    .into_iter()
                    .map(|value| (value.into(), SetCodec(value.into())))
                    .collect(),
            ),
            SetCodec(value) => {
                self.settings.video_codec = value;
                self.save();
            }
            Bitrate => self.edit(
                "视频码率",
                self.settings.video_bit_rate.clone(),
                Field::Bitrate,
            ),
            Scrcpy => self.toggle_scrcpy()?,
            Shell => self.toggle_shell()?,
            Files => {
                let device = self.online_device()?;
                let path = self.files.entry(device.clone()).or_default().path.clone();
                self.set_tab(Tab::Files(device.clone()));
                self.list_files(device, path, None);
            }
            Root | Remount | Sync => {
                let id = self.online_device()?;
                self.job(move |adb| {
                    let args: &[&str] = match action {
                        Root => &["-s", &id, "root"],
                        Remount => &["-s", &id, "remount"],
                        _ => &["-s", &id, "shell", "sync"],
                    };
                    JobEvent::Status(adb.execute(args))
                });
                self.result(Ok("ADB 命令已发送".into()));
            }
            File(action) => self.file_action(action)?,
            Delete(entries) => {
                let device = self.file_device()?;
                self.file_job(device, move |adb, id| {
                    files::delete(adb, id, &entries).map(|_| format!("已删除 {} 项", entries.len()))
                });
            }
            Clear => {
                if let Some(session) = self.session_mut() {
                    session.clear();
                }
            }
            CopyText => self.copy_text()?,
            PasteText => self.paste_text()?,
            Cancel => {}
        }
        Ok(())
    }

    fn list_files(&mut self, device: String, path: String, history: Option<usize>) {
        let request = self.generation();
        let Some(browser) = self.files.get_mut(&device) else {
            return;
        };
        browser.loading = true;
        browser.request = request;
        self.job(move |adb| {
            let result = files::list(adb, &device, &path);
            JobEvent::Listing(device, request, path, history, result)
        });
    }

    fn toggle_tree(&mut self, device: String, path: String) {
        let request = self.generation();
        let browser = self.files.get_mut(&device).unwrap();
        if browser.expanded.remove(&path) {
            browser.tree_scroll = browser
                .tree_scroll
                .min(browser.tree().len().saturating_sub(1));
            return;
        }
        browser.expanded.insert(path.clone());
        if !browser.directories.contains_key(&path) {
            browser.tree_requests.insert(path.clone(), request);
            self.job(move |adb| {
                let result = files::list(adb, &device, &path);
                JobEvent::TreeListing(device, request, path, result)
            });
        }
    }

    fn file_device(&self) -> Result<String> {
        let Tab::Files(device) = self.active_tab() else {
            return Err("请先打开设备文件".into());
        };
        if !self
            .devices
            .iter()
            .any(|item| &item.id == device && item.online())
        {
            return Err("ADB 设备离线".into());
        }
        if self.files[device].busy || self.files[device].loading {
            return Err("正在处理设备文件".into());
        }
        Ok(device.clone())
    }

    fn file_job(
        &mut self,
        device: String,
        task: impl FnOnce(&Adb, &str) -> Result<String> + Send + 'static,
    ) {
        self.files.get_mut(&device).unwrap().busy = true;
        self.result(Ok("文件操作进行中…".into()));
        self.job(move |adb| {
            let result = task(adb, &device);
            JobEvent::FileDone(device, result)
        });
    }

    fn file_action(&mut self, action: FileAction) -> Result<()> {
        let device = self.file_device()?;
        let browser = &self.files[&device];
        match action {
            FileAction::Refresh => self.list_files(device, browser.path.clone(), None),
            FileAction::Path => self.edit("前往设备路径", browser.path.clone(), Field::Path),
            FileAction::Mkdir => self.edit("新建目录", String::new(), Field::Mkdir),
            FileAction::Rename => {
                let entry = one(browser.selected())?;
                self.edit("重命名", entry.name.clone(), Field::Rename(entry));
            }
            FileAction::Delete => {
                let entries = selected(browser.selected())?;
                self.picked(
                    &format!("永久删除设备上的 {} 项？", entries.len()),
                    vec![
                        ("取消 (Esc)".into(), Action::Cancel),
                        ("确认删除".into(), Action::Delete(entries)),
                    ],
                );
            }
            FileAction::Sync => self.file_job(device, |adb, id| {
                adb.execute(&["-s", id, "shell", "sync"])
                    .map(|_| "设备已刷盘".into())
            }),
            FileAction::Upload | FileAction::UploadFolder => {
                if cfg!(windows) {
                    let mode = if matches!(action, FileAction::Upload) {
                        "upload-file"
                    } else {
                        "upload-directory"
                    };
                    self.files.get_mut(&device).unwrap().busy = true;
                    self.job(move |_| {
                        JobEvent::UploadPicked(device, crate::file_dialog::choose(0, mode, ""))
                    });
                } else {
                    self.edit("上传本地路径", String::new(), Field::UploadPath);
                }
            }
            FileAction::Download => self.choose_download(device, selected(browser.selected())?),
            FileAction::Copy => self.copy_files(device, selected(browser.selected())?)?,
            FileAction::Paste => self.paste_files()?,
        }
        Ok(())
    }

    fn choose_download(&mut self, device: String, entries: Vec<Entry>) {
        if !cfg!(windows) {
            self.edit("选择下载目录", String::new(), Field::DownloadPath(entries));
            return;
        }
        self.file_job(device, move |adb, id| {
            if entries.len() == 1 && !entries[0].directory {
                let Some(destination) =
                    crate::file_dialog::choose(0, "download-file", &entries[0].name)?
                else {
                    return Ok("已取消下载".into());
                };
                files::download(adb, id, &entries[0], &destination)?;
            } else {
                let Some(directory) = crate::file_dialog::choose(0, "download-directory", "")?
                else {
                    return Ok("已取消下载".into());
                };
                for entry in &entries {
                    files::download(
                        adb,
                        id,
                        entry,
                        &files::destination(&directory, &entry.name)?,
                    )?;
                }
            }
            Ok(format!("已下载 {} 项", entries.len()))
        });
    }

    fn rename_uploads(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        let files = paths
            .into_iter()
            .map(|path| files::local_name(&path).map(|name| (path, name)))
            .collect::<Result<Vec<_>>>()?;
        if files.is_empty() {
            return Err("没有要上传的文件".into());
        }
        self.modal = Some(Modal::Upload(UploadNames { files, index: 0 }));
        Ok(())
    }

    fn submit(&mut self, field: Field, value: String) -> Result<()> {
        match field {
            Field::Address => {
                self.settings.adb_address = address(&value)?;
                self.save();
            }
            Field::Bitrate => {
                if value.is_empty()
                    || value.len() > 16
                    || !value
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.')
                {
                    return Err("码率格式无效，例如 8M".into());
                }
                self.settings.video_bit_rate = value;
                self.save();
            }
            Field::Path => {
                self.list_files(self.file_device()?, files::normalize_path(&value)?, None)
            }
            Field::Mkdir => {
                let device = self.file_device()?;
                let path = self.files[&device].path.clone();
                self.file_job(device, move |adb, id| {
                    files::mkdir(adb, id, &path, &value).map(|_| "目录已创建".into())
                });
            }
            Field::Rename(entry) => {
                if entry.name == value {
                    return Ok(());
                }
                self.file_job(self.file_device()?, move |adb, id| {
                    files::rename(adb, id, &entry.path, &value).map(|_| "已重命名".into())
                });
            }
            Field::UploadPath => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("请输入本地绝对路径".into());
                }
                self.rename_uploads(vec![path])?;
            }
            Field::DownloadPath(entries) => {
                let directory = PathBuf::from(value);
                if !directory.is_absolute() || !directory.is_dir() {
                    return Err("请输入存在的本地绝对目录".into());
                }
                self.file_job(self.file_device()?, move |adb, id| {
                    for entry in &entries {
                        files::download(
                            adb,
                            id,
                            entry,
                            &files::destination(&directory, &entry.name)?,
                        )?;
                    }
                    Ok(format!("已下载 {} 项", entries.len()))
                });
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn copy_files(&mut self, device: String, entries: Vec<Entry>) -> Result<()> {
        self.files.get_mut(&device).unwrap().busy = true;
        self.result(Ok("正在准备文件剪贴板…".into()));
        self.job(move |adb| {
            let result = (|| {
                let stage = tempfile::Builder::new()
                    .prefix("serial-terminal-clipboard-")
                    .tempdir()
                    .map_err(|err| err.to_string())?;
                let mut paths = Vec::new();
                for entry in &entries {
                    let path = files::destination(stage.path(), &entry.name)?;
                    files::download(adb, &device, entry, &path)?;
                    paths.push(path.to_string_lossy().into_owned());
                }
                Ok((stage, paths))
            })();
            JobEvent::Clipboard(device, result)
        });
        Ok(())
    }

    #[cfg(not(windows))]
    fn copy_files(&mut self, _device: String, _entries: Vec<Entry>) -> Result<()> {
        Err("系统文件剪贴板仅支持 Windows".into())
    }

    fn paste_files(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            let paths: Vec<PathBuf> =
                clipboard_win::get_clipboard(clipboard_win::formats::FileList)
                    .map_err(|error| error.to_string())?;
            self.rename_uploads(paths)
        }
        #[cfg(not(windows))]
        Err("系统文件剪贴板仅支持 Windows".into())
    }

    fn copy_text(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            let text = self
                .session()
                .and_then(Session::selected_text)
                .filter(|text| !text.is_empty())
                .ok_or("请先选择终端文本")?;
            clipboard_win::set_clipboard(clipboard_win::formats::Unicode, text)
                .map_err(|error| error.to_string())?;
            self.result(Ok("已复制终端选区".into()));
            Ok(())
        }
        #[cfg(not(windows))]
        Err("系统文本剪贴板仅支持 Windows".into())
    }

    fn paste_text(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            let text: String = clipboard_win::get_clipboard(clipboard_win::formats::Unicode)
                .map_err(|error| error.to_string())?;
            if let Some(session) = self.session_mut() {
                session.paste(text);
            }
            Ok(())
        }
        #[cfg(not(windows))]
        Err("请使用宿主终端的粘贴快捷键".into())
    }

    fn handle_job(&mut self, event: JobEvent) {
        match event {
            JobEvent::Devices(result) => {
                self.devices_loading = false;
                match result {
                    Ok(devices) => {
                        let target = self.serial_target();
                        self.devices = devices;
                        if !self
                            .devices
                            .iter()
                            .any(|device| device.id == self.settings.adb_device)
                            && !self
                                .settings
                                .adb_history
                                .contains(&self.settings.adb_device)
                        {
                            self.settings.adb_device = self
                                .devices
                                .iter()
                                .find(|device| device.online())
                                .map(|device| device.id.clone())
                                .unwrap_or_default();
                        }
                        if target != self.serial_target() {
                            self.size_attempt = None;
                        }
                    }
                    Err(error) => self.result(Err(error)),
                }
            }
            JobEvent::Ports(result) => match result {
                Ok(ports) => {
                    self.ports = ports;
                    if !self.ports.contains(&self.settings.serial.port_name)
                        && self.serial.connection.is_none()
                    {
                        self.settings.serial.port_name =
                            self.ports.first().cloned().unwrap_or_default();
                    }
                }
                Err(error) => self.result(Err(error)),
            },
            JobEvent::Opened(tab, id, result) => {
                let Some(session) = self.by_generation(id) else {
                    return;
                };
                session.opening = false;
                match result {
                    Ok(connection) => {
                        session.connection = Some(connection);
                        session.flush_replies();
                        if matches!(tab, Tab::Shell(_)) {
                            session.send(Vec::new());
                        }
                        self.result(Ok(if tab == Tab::Serial {
                            "串口已连接".into()
                        } else {
                            "ADB Shell 已连接".into()
                        }));
                    }
                    Err(error) => self.result(Err(error)),
                }
            }
            JobEvent::Status(result) => self.result(result),
            JobEvent::Remote(id, disconnect, result) => {
                self.remote_busy = false;
                if result.is_ok() && !disconnect {
                    self.settings.adb_history.retain(|address| address != &id);
                    self.settings.adb_history.insert(0, id.clone());
                    self.settings.adb_history.truncate(24);
                    self.settings.adb_device = id.clone();
                    self.settings.adb_address = id;
                    self.save();
                }
                self.result(result);
                self.refresh_devices();
            }
            JobEvent::Scrcpy(id, result) => {
                self.pending_scrcpy.remove(&id);
                match result {
                    Ok(child) => {
                        self.result(Ok(format!("已打开 scrcpy · {id}")));
                        self.scrcpy.insert(id, child);
                    }
                    Err(error) => self.result(Err(error)),
                }
            }
            JobEvent::Sized(id, size, result) => {
                self.size_inflight = false;
                if let Some(error) = result
                    .err()
                    .filter(|_| self.size_attempt == Some((id, size)))
                {
                    self.result(Err(format!("串口尺寸同步失败：{error}")));
                }
            }
            JobEvent::Listing(id, request, path, history, result) => {
                let Some(browser) = self
                    .files
                    .get_mut(&id)
                    .filter(|browser| browser.request == request)
                else {
                    return;
                };
                browser.loading = false;
                match result {
                    Ok(entries) => browser.apply(path, entries, history),
                    Err(error) => self.result(Err(error)),
                }
            }
            JobEvent::TreeListing(id, request, path, result) => {
                let Some(browser) = self.files.get_mut(&id) else {
                    return;
                };
                if browser.tree_requests.get(&path) != Some(&request) {
                    return;
                }
                browser.tree_requests.remove(&path);
                match result {
                    Ok(entries) => browser.cache_directories(&path, &entries),
                    Err(error) => {
                        browser.expanded.remove(&path);
                        self.result(Err(error));
                    }
                }
            }
            JobEvent::FileDone(id, result) => {
                self.result(result);
                if let Some(browser) = self.files.get_mut(&id) {
                    browser.busy = false;
                    let path = browser.path.clone();
                    self.list_files(id, path, None);
                }
            }
            JobEvent::UploadPicked(id, result) => {
                if let Some(browser) = self.files.get_mut(&id) {
                    browser.busy = false;
                }
                match result {
                    Ok(Some(path)) => {
                        self.set_tab(Tab::Files(id));
                        let result = self.rename_uploads(vec![path]);
                        if let Err(error) = result {
                            self.result(Err(error));
                        }
                    }
                    Ok(None) => (),
                    Err(error) => self.result(Err(error)),
                }
            }
            JobEvent::Clipboard(id, result) => {
                if let Some(browser) = self.files.get_mut(&id) {
                    browser.busy = false;
                }
                match result {
                    Ok((stage, paths)) => {
                        #[cfg(windows)]
                        match publish_file_clipboard(&paths) {
                            Ok(()) => {
                                self._clipboard = Some(stage);
                                self.result(Ok(format!(
                                    "已复制 {} 项，可在资源管理器粘贴",
                                    paths.len()
                                )));
                            }
                            Err(error) => self.result(Err(error.to_string())),
                        }
                        #[cfg(not(windows))]
                        drop((stage, paths));
                    }
                    Err(error) => self.result(Err(error)),
                }
            }
        }
    }

    fn drain(&mut self) {
        while let Ok(event) = self.job_events.try_recv() {
            self.handle_job(event);
        }
        for _ in 0..256 {
            match self.io_events.try_recv() {
                Ok(SessionEvent::Output(id, bytes)) => {
                    if let Some(session) = self.by_generation(id) {
                        session.output(&bytes);
                    }
                }
                Ok(SessionEvent::Ended(id, result)) => {
                    let serial_ended = self.serial.id == id;
                    if serial_ended {
                        self.serial_input_waiting.clear();
                    }
                    if let Some(session) = self.by_generation(id) {
                        session.id = 0;
                        session.opening = false;
                        session.connection = None;
                        let label = if serial_ended { "串口" } else { "ADB Shell" };
                        self.result(
                            result
                                .map(|()| format!("{label}已关闭"))
                                .map_err(|error| format!("{label}已断开：{error}")),
                        );
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn key(&mut self, event: KeyEvent, geometry: ui::Geometry) {
        if event.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        if event.code == KeyCode::F(10) && ctrl {
            self.quitting = true;
            return;
        }
        if self.modal.is_some() {
            self.modal_key(event);
            return;
        }
        if ctrl {
            match event.code {
                KeyCode::F(1) => {
                    self.modal = Some(Modal::Help);
                    return;
                }
                KeyCode::F(2) => {
                    self.sidebar = true;
                    return;
                }
                KeyCode::F(3) => {
                    self.sidebar = false;
                    return;
                }
                KeyCode::F(4) => {
                    self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
                    self.sidebar = false;
                    return;
                }
                KeyCode::F(5) => {
                    self.active = (self.active + 1) % self.tabs.len();
                    self.sidebar = false;
                    return;
                }
                KeyCode::F(6) => {
                    self.close_tab();
                    return;
                }
                _ => {}
            }
        }
        if self.sidebar {
            let controls = ui::controls(self);
            match event.code {
                KeyCode::Up => self.control = self.control.saturating_sub(1),
                KeyCode::Down => {
                    self.control = (self.control + 1).min(controls.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    if let Some((_, action)) = controls.get(self.control) {
                        self.action(action.clone());
                    }
                }
                KeyCode::Right | KeyCode::Tab => self.sidebar = false,
                KeyCode::Esc | KeyCode::Char('q') => self.quitting = true,
                _ => {}
            }
            return;
        }
        if matches!(self.active_tab(), Tab::Files(_)) {
            self.file_key(event, geometry.file_rows());
            return;
        }
        if ctrl && shift {
            match event.code {
                KeyCode::Char('c' | 'C') => {
                    self.action(Action::CopyText);
                    return;
                }
                KeyCode::Char('v' | 'V') => {
                    self.action(Action::PasteText);
                    return;
                }
                _ => {}
            }
        }
        if let Some(session) = self.session_mut() {
            match event.code {
                KeyCode::PageUp if shift => {
                    session.scroll(isize::from(geometry.terminal.height as i16))
                }
                KeyCode::PageDown if shift => {
                    session.scroll(-isize::from(geometry.terminal.height as i16))
                }
                _ => {
                    if session.screen().scrollback() > 0 {
                        session.scroll(-(session.screen().scrollback() as isize));
                    }
                    let bytes = terminal::encode_key(event, session.screen());
                    if !bytes.is_empty() {
                        self.send_key_bytes(bytes);
                    }
                }
            }
        }
    }

    fn modal_key(&mut self, key: KeyEvent) {
        let Some(mut modal) = self.modal.take() else {
            return;
        };
        if key.code == KeyCode::Esc {
            return;
        }
        match &mut modal {
            Modal::Help => return,
            Modal::Pick { options, index, .. } => match key.code {
                KeyCode::Up => *index = index.saturating_sub(1),
                KeyCode::Down => *index = (*index + 1).min(options.len().saturating_sub(1)),
                KeyCode::Enter => {
                    if let Some((_, action)) = options.get(*index) {
                        self.action(action.clone());
                    }
                    return;
                }
                _ => {}
            },
            Modal::Edit { field, value, .. } => {
                if key.code == KeyCode::Enter {
                    let result = self.submit(field.clone(), value.clone());
                    if let Err(error) = result {
                        self.result(Err(error));
                        self.modal = Some(modal);
                    }
                    return;
                }
                edit_value(key, value);
            }
            Modal::Upload(upload) => {
                if key.code == KeyCode::Enter {
                    if upload.index + 1 < upload.files.len() {
                        upload.index += 1;
                    } else {
                        match self.file_device() {
                            Ok(device) => {
                                let directory = self.files[&device].path.clone();
                                let entries = std::mem::take(&mut upload.files);
                                self.file_job(device, move |adb, id| {
                                    files::upload(adb, id, &directory, &entries)
                                        .map(|_| format!("已上传 {} 项", entries.len()))
                                });
                            }
                            Err(error) => self.result(Err(error)),
                        }
                        return;
                    }
                } else {
                    edit_value(key, &mut upload.files[upload.index].1);
                }
            }
        }
        self.modal = Some(modal);
    }

    fn file_key(&mut self, key: KeyEvent, visible: usize) {
        use FileAction::*;
        let Tab::Files(device) = self.active_tab().clone() else {
            return;
        };
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let action = match key.code {
            KeyCode::Char('c' | 'C') if ctrl => Some(Copy),
            KeyCode::Char('v' | 'V') if ctrl => Some(Paste),
            KeyCode::Char('r') => Some(Refresh),
            KeyCode::Char('g') => Some(Path),
            KeyCode::Char('u') => Some(Upload),
            KeyCode::Char('U') => Some(UploadFolder),
            KeyCode::Char('d') => Some(Download),
            KeyCode::Char('n') => Some(Mkdir),
            KeyCode::F(2) => Some(Rename),
            KeyCode::Delete => Some(Delete),
            KeyCode::Char('s') => Some(Sync),
            _ => None,
        };
        if let Some(action) = action {
            self.action(Action::File(action));
            return;
        }
        let browser = self.files.get_mut(&device).unwrap();
        match key.code {
            KeyCode::Up => browser.cursor = browser.cursor.saturating_sub(1),
            KeyCode::Down => {
                browser.cursor = (browser.cursor + 1).min(browser.entries.len().saturating_sub(1))
            }
            KeyCode::PageUp => browser.cursor = browser.cursor.saturating_sub(visible),
            KeyCode::PageDown => {
                browser.cursor =
                    (browser.cursor + visible).min(browser.entries.len().saturating_sub(1))
            }
            KeyCode::Home => browser.cursor = 0,
            KeyCode::End => browser.cursor = browser.entries.len().saturating_sub(1),
            KeyCode::Char(' ') => toggle_mark(browser),
            KeyCode::Enter if !browser.busy && !browser.loading => {
                if let Some(entry) = browser.entries.get(browser.cursor) {
                    if entry.directory {
                        let path = entry.path.clone();
                        self.list_files(device, path, None);
                    } else {
                        let entry = entry.clone();
                        self.choose_download(device, vec![entry]);
                    }
                }
                return;
            }
            KeyCode::Backspace if !browser.busy && !browser.loading => {
                let path = files::parent(&browser.path).to_owned();
                self.list_files(device, path, None);
                return;
            }
            KeyCode::Left | KeyCode::Right if alt && !browser.busy && !browser.loading => {
                let index = if key.code == KeyCode::Left {
                    browser.history_index.checked_sub(1)
                } else {
                    Some(browser.history_index + 1)
                };
                if let Some((index, path)) = index.and_then(|index| {
                    browser
                        .history
                        .get(index)
                        .cloned()
                        .map(|path| (index, path))
                }) {
                    self.list_files(device, path, Some(index));
                }
                return;
            }
            _ => {}
        }
        keep_visible(&mut browser.scroll, browser.cursor, visible);
    }

    fn mouse(&mut self, mouse: MouseEvent, geometry: ui::Geometry) {
        let pos = (mouse.column, mouse.row);
        if let Some(modal) = &self.modal {
            if let MouseEventKind::Down(button) = mouse.kind {
                if !geometry.inside_modal(modal, pos) {
                    self.modal = None;
                } else if button == MouseButton::Left {
                    if let Modal::Pick { options, .. } = modal {
                        if let Some(index) = ui::modal_option_at(geometry, modal, pos) {
                            let action = options[index].1.clone();
                            self.modal = None;
                            self.action(action);
                        }
                    }
                }
            }
            return;
        }
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            if let Some(index) = ui::tab_at(self, geometry, pos) {
                self.active = index;
                self.sidebar = false;
                return;
            }
            if let Some(index) = geometry.control_at(pos, self.control_scroll) {
                self.control = index;
                self.sidebar = true;
                if let Some((_, action)) = ui::controls(self).get(index) {
                    self.action(action.clone());
                }
                return;
            }
        }
        if geometry.inside_side(pos) {
            match mouse.kind {
                MouseEventKind::ScrollUp => self.control = self.control.saturating_sub(3),
                MouseEventKind::ScrollDown => {
                    self.control =
                        (self.control + 3).min(ui::controls(self).len().saturating_sub(1))
                }
                _ => {}
            }
            return;
        }
        if let Tab::Files(device) = self.active_tab().clone() {
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                self.file_drag_anchor = None;
            }
            if !geometry.inside_body(pos) {
                return;
            }
            self.sidebar = false;
            if let Some((tree_row, column)) = geometry.tree_at(pos) {
                let browser = self.files.get_mut(&device).unwrap();
                if matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                ) {
                    let change = if mouse.kind == MouseEventKind::ScrollUp {
                        -3
                    } else {
                        3
                    };
                    let last = browser
                        .tree()
                        .len()
                        .saturating_sub(geometry.file_rows().max(1));
                    browser.tree_scroll =
                        browser.tree_scroll.saturating_add_signed(change).min(last);
                } else if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && !browser.busy
                    && !browser.loading
                {
                    let target = browser
                        .tree()
                        .get(browser.tree_scroll + tree_row)
                        .map(|&(depth, path, _)| (depth, path.to_owned()));
                    if let Some((depth, path)) = target {
                        if column <= depth * 2 + 1 {
                            self.toggle_tree(device, path);
                        } else {
                            browser.expanded.remove(&path);
                            self.list_files(device, path, None);
                        }
                    }
                }
                return;
            }
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) {
                let direction = if mouse.kind == MouseEventKind::ScrollUp {
                    KeyCode::Up
                } else {
                    KeyCode::Down
                };
                for _ in 0..3 {
                    self.file_key(
                        KeyEvent::new(direction, KeyModifiers::NONE),
                        geometry.file_rows(),
                    );
                }
            } else if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
                if let Some((origin_device, anchor)) = &self.file_drag_anchor {
                    let browser = self.files.get_mut(&device).unwrap();
                    if let Some(index) = geometry.file_at(pos, browser.scroll).filter(|&index| {
                        origin_device == &device
                            && *anchor < browser.entries.len()
                            && index < browser.entries.len()
                    }) {
                        browser.cursor = index;
                        let range = (*anchor).min(index)..=(*anchor).max(index);
                        browser.marked = browser.entries[range]
                            .iter()
                            .filter(|entry| entry.name != "..")
                            .map(|entry| entry.path.clone())
                            .collect();
                        self.last_file_click = None;
                    }
                }
            } else if let MouseEventKind::Down(button) = mouse.kind {
                if let Some(index) = geometry
                    .file_at(pos, self.files[&device].scroll)
                    .filter(|&index| index < self.files[&device].entries.len())
                {
                    let browser = self.files.get_mut(&device).unwrap();
                    browser.cursor = index;
                    if button == MouseButton::Left {
                        if mouse.modifiers.contains(KeyModifiers::CONTROL) {
                            toggle_mark(browser);
                        } else {
                            browser.marked.clear();
                            self.file_drag_anchor = Some((device.clone(), index));
                            let previous = self.last_file_click.replace((
                                device.clone(),
                                index,
                                Instant::now(),
                            ));
                            if previous.is_some_and(|(prior, row, time)| {
                                prior == device
                                    && row == index
                                    && time.elapsed() <= Duration::from_millis(350)
                            }) {
                                self.last_file_click = None;
                                self.file_key(
                                    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                                    geometry.file_rows(),
                                );
                                return;
                            }
                        }
                    }
                }
                if button == MouseButton::Right || geometry.in_file_actions(pos) {
                    self.picked("设备文件操作", file_actions());
                }
            }
            return;
        }
        let Some(cell) = geometry.cell_at(pos) else {
            return;
        };
        self.sidebar = false;
        if mouse.kind == MouseEventKind::Down(MouseButton::Right) {
            let mut options = vec![
                ("复制".into(), Action::CopyText),
                ("粘贴".into(), Action::PasteText),
                ("清空".into(), Action::Clear),
            ];
            if self.active_tab() == &Tab::Serial {
                options.push((
                    if self.settings.serial_sync {
                        "关闭自动尺寸同步"
                    } else {
                        "开启自动尺寸同步"
                    }
                    .into(),
                    Action::SerialSync,
                ));
            }
            self.picked("终端操作", options);
            return;
        }
        if let Some(session) = self.session_mut() {
            match mouse.kind {
                MouseEventKind::ScrollUp => session.scroll(3),
                MouseEventKind::ScrollDown => session.scroll(-3),
                MouseEventKind::Down(MouseButton::Left) => session.selection = Some((cell, cell)),
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((start, _)) = session.selection {
                        session.selection = Some((start, cell));
                    }
                }
                MouseEventKind::Up(MouseButton::Left)
                    if session.selection == Some((cell, cell)) =>
                {
                    session.selection = None
                }
                _ => {}
            }
        }
    }

    fn paste(&mut self, value: String) {
        let value = value.replace(['\r', '\n'], "");
        match &mut self.modal {
            Some(Modal::Edit { value: field, .. }) => field.push_str(&value),
            Some(Modal::Upload(upload)) => upload.files[upload.index].1.push_str(&value),
            _ => {}
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.adb.closing.store(true, Ordering::Relaxed);
    }
}

pub fn keep_visible(scroll: &mut usize, cursor: usize, height: usize) {
    let height = height.max(1);
    if cursor < *scroll {
        *scroll = cursor;
    }
    if cursor >= scroll.saturating_add(height) {
        *scroll = cursor.saturating_sub(height - 1);
    }
}

fn edit_value(event: KeyEvent, value: &mut String) {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    match event.code {
        KeyCode::Char('u') if ctrl => value.clear(),
        KeyCode::Char('v') if ctrl => {
            #[cfg(windows)]
            if let Ok(text) =
                clipboard_win::get_clipboard::<String, _>(clipboard_win::formats::Unicode)
            {
                value.extend(text.chars().filter(|ch| !ch.is_control()));
            }
        }
        KeyCode::Char(ch) if !ctrl || event.modifiers.contains(KeyModifiers::ALT) => value.push(ch),
        KeyCode::Backspace => {
            value.pop();
        }
        _ => {}
    }
}

fn toggle_mark(browser: &mut Browser) {
    if let Some(entry) = browser
        .entries
        .get(browser.cursor)
        .filter(|entry| entry.name != "..")
    {
        if !browser.marked.remove(&entry.path) {
            browser.marked.insert(entry.path.clone());
        }
    }
}

#[cfg(windows)]
fn publish_file_clipboard(paths: &[String]) -> std::result::Result<(), clipboard_win::ErrorCode> {
    use clipboard_win::Setter;
    let _clipboard = clipboard_win::Clipboard::new_attempts(10)?;
    clipboard_win::formats::FileList.write_clipboard(paths)
}

fn selected(entries: Vec<Entry>) -> Result<Vec<Entry>> {
    if entries.is_empty() {
        Err("请先选择文件或目录".into())
    } else {
        Ok(entries)
    }
}

fn one(entries: Vec<Entry>) -> Result<Entry> {
    let [entry]: [Entry; 1] = entries.try_into().map_err(|_| "请选择单个文件或目录")?;
    Ok(entry)
}

pub fn device_state(state: &str) -> &str {
    match state {
        "device" => "已连接",
        "offline" => "离线",
        "unauthorized" => "待授权",
        _ => state,
    }
}

fn address(input: &str) -> Result<String> {
    let input = input.trim();
    let input = input.strip_prefix("tcp://").unwrap_or(input);
    if input.is_empty()
        || !input
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_:[]".contains(&byte))
        || !input.as_bytes()[0].is_ascii_alphanumeric() && !input.starts_with('[')
    {
        return Err("请输入有效的 ADB 地址".into());
    }
    if input.starts_with('[') && input.ends_with(']') {
        return Ok(format!("{input}:5555"));
    }
    if let Ok(ipv6) = input.parse::<std::net::Ipv6Addr>() {
        return Ok(format!("[{ipv6}]:5555"));
    }
    if let Some((_, port)) = input.rsplit_once(':') {
        if !port.parse::<u16>().is_ok_and(|port| port > 0) {
            return Err("ADB 端口必须是 1–65535".into());
        }
        Ok(input.into())
    } else {
        Ok(format!("{input}:5555"))
    }
}

fn file_actions() -> Vec<(String, Action)> {
    use FileAction::*;
    [
        ("刷新  R", Refresh),
        ("跳转路径  G", Path),
        ("上传文件  U", Upload),
        ("上传目录  Shift+U", UploadFolder),
        ("下载  D", Download),
        ("新建目录  N", Mkdir),
        ("重命名  F2", Rename),
        ("删除  Del", Delete),
        ("设备刷盘  S", Sync),
        ("复制  Ctrl+C", Copy),
        ("粘贴  Ctrl+V", Paste),
    ]
    .into_iter()
    .map(|(title, action)| (title.into(), Action::File(action)))
    .collect()
}

struct Capture;
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture, DisableBracketedPaste);
    }
}

pub fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let _capture = Capture;
    execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    let mut app = App::new();
    app.refresh_ports();
    loop {
        let size = terminal.size()?;
        let geometry = ui::geometry(size.into());
        app.layout_size(geometry.size());
        app.drain();
        app.tick();
        keep_visible(
            &mut app.control_scroll,
            app.control,
            geometry.control_rows(),
        );
        terminal.draw(|frame| ui::render(frame, &app))?;
        if event::poll(Duration::from_millis(25))? {
            match event::read()? {
                Event::Key(key) => app.key(key, geometry),
                Event::Mouse(mouse) => app.mouse(mouse, geometry),
                Event::Paste(text) if app.modal.is_some() => app.paste(text),
                Event::Paste(text) if !app.sidebar => {
                    if let Some(session) = app.session_mut() {
                        session.paste(text);
                    }
                }
                _ => {}
            }
        }
        if app.quitting {
            break;
        }
    }
    app.settings.save()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    #[test]
    fn clicking_outside_cancels_each_modal_without_activating_the_sidebar() {
        let geometry = ui::geometry(Rect::new(0, 0, 100, 28));
        let mut app = App::new();
        app.control = 5;
        let click = |column, row| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        for modal in [
            Modal::Pick {
                title: "确认删除".into(),
                options: vec![("取消".into(), Action::Cancel)],
                index: 0,
            },
            Modal::Edit {
                title: "远程 ADB 地址".into(),
                value: "192.168.30.1:5555".into(),
                field: Field::Address,
            },
            Modal::Upload(UploadNames {
                files: vec![(PathBuf::from("file.txt"), "新名称.txt".into())],
                index: 0,
            }),
            Modal::Help,
        ] {
            app.modal = Some(modal);
            app.mouse(click(14, 14), geometry);
            assert!(
                app.modal.is_some(),
                "clicks on the border stay in the modal"
            );
            app.mouse(click(2, 2), geometry);
            assert!(app.modal.is_none());
            assert_eq!(app.control, 5, "outside click is consumed");
        }

        app.picked("选择操作", vec![("取消".into(), Action::Cancel)]);
        app.mouse(click(30, 13), geometry);
        assert!(app.modal.is_none(), "clicking a choice still confirms it");
    }

    #[test]
    fn right_pane_dimensions_drive_sessions_and_layout_survives_small_windows() {
        let mut app = App::new();
        app.settings = Settings::default();
        let wide = ui::geometry(Rect::new(0, 0, 124, 34)).size();
        let narrow = ui::geometry(Rect::new(0, 0, 70, 20)).size();
        app.layout_size(wide);
        let mut display = Terminal::new(TestBackend::new(124, 34)).unwrap();
        display.draw(|frame| ui::render(frame, &app)).unwrap();
        assert_eq!(app.serial.screen().size(), (wide.rows, wide.cols));
        assert!(wide.cols > narrow.cols && narrow.cols < 50);
        app.layout_size(narrow);
        assert_eq!(app.serial.screen().size(), (narrow.rows, narrow.cols));
        let mut small = Terminal::new(TestBackend::new(16, 6)).unwrap();
        small.draw(|frame| ui::render(frame, &app)).unwrap();
    }
}
