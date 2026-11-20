#[cfg(not(windows))]
use crate::SERIAL_READ_TIMEOUT_MS;
use crate::{adb_server, adb_shell_protocol, SerialConfig};
use adb_shell_protocol::TerminalSize;
use std::{
    env,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, String>;

const ADB_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

pub fn ports() -> Result<Vec<String>> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|port| port.port_name).collect())
        .map_err(|error| error.to_string())
}

pub fn find_tool(name: &str) -> Result<PathBuf> {
    let file = format!("{name}{}", env::consts::EXE_SUFFIX);
    let beside = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file)));
    let paths = env::var_os("PATH").unwrap_or_default();
    beside
        .into_iter()
        .chain(env::split_paths(&paths).map(|dir| dir.join(&file)))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("未找到 {file}，请放到程序同级目录或加入 PATH"))
}

fn command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    if let Some(dir) = executable.parent() {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

#[derive(Default)]
pub struct Adb {
    server: adb_server::AdbServerManager,
    pub closing: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub state: String,
}

impl Device {
    pub fn online(&self) -> bool {
        self.state == "device"
    }
}

impl Adb {
    fn ensure_started(&self) -> Result<PathBuf> {
        let path = find_tool("adb")?;
        self.server.ensure_started(
            adb_server::server_is_running,
            || {
                self.output(&path, &["start-server"], Duration::from_secs(20))
                    .map(|_| ())
            },
            |_| (),
        )?;
        Ok(path)
    }

    pub fn run(&self, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
        let path = self.ensure_started()?;
        self.output(&path, args, timeout)
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let output = self.run(&["devices"], ADB_TIMEOUT)?;
        Ok(String::from_utf8_lossy(&output)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let id = fields.next()?;
                let state = fields.next()?;
                if !line.contains('\t') || id.starts_with('*') || id.chars().any(char::is_control) {
                    return None;
                }
                Some(Device {
                    id: id.into(),
                    state: state.into(),
                })
            })
            .collect())
    }

    pub fn execute(&self, args: &[&str]) -> Result<String> {
        let bytes = self.run(args, ADB_TIMEOUT)?;
        Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
    }

    pub fn sync_serial(&self, id: &str, size: TerminalSize) -> Result<()> {
        let TerminalSize { rows, cols } = size;
        let script = format!(
            r#"tty=$(getprop ro.boot.console)
case "$tty" in ""|*[!a-zA-Z0-9_]*) echo '无法识别 Android 串口控制台' >&2; exit 1 ;; esac
[ -c "/dev/$tty" ] || exit 1
set -- -F "/dev/$tty" rows {rows} cols {cols}
stty "$@" 2>/dev/null || su 0 stty "$@""#
        );
        self.execute(&["-s", id, "shell", &script]).map(|_| ())
    }

    pub fn scrcpy(&self, id: &str, codec: &str, bitrate: &str) -> Result<ManagedChild> {
        let adb = self.ensure_started()?;
        let scrcpy = find_tool("scrcpy")?;
        let mut command = command(&scrcpy);
        command
            .args([
                "--serial",
                id,
                "--video-codec",
                codec,
                "--video-bit-rate",
                bitrate,
            ])
            .env("ADB", adb)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        ManagedChild::spawn(&mut command)
    }

    fn output(&self, path: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
        if self.closing.load(Ordering::Relaxed) {
            return Err("程序正在退出".into());
        }
        let mut command = command(path);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = ManagedChild::spawn_unmanaged(&mut command)?;
        let stdout = child.child.stdout.take().ok_or("ADB 输出管道不可用")?;
        let stderr = child.child.stderr.take().ok_or("ADB 错误管道不可用")?;
        let read_stdout = thread::spawn(move || capture(stdout, MAX_OUTPUT));
        let read_stderr = thread::spawn(move || capture(stderr, 128 * 1024));
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if self.closing.load(Ordering::Relaxed) || Instant::now() >= deadline {
                return Err(format!(
                    "ADB 操作取消或超时：{}",
                    args.first().copied().unwrap_or("")
                ));
            }
            thread::sleep(Duration::from_millis(15));
        };
        let output = Output {
            status,
            stdout: read_stdout.join().map_err(|_| "读取 ADB 输出失败")??,
            stderr: read_stderr.join().map_err(|_| "读取 ADB 错误失败")??,
        };
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "ADB {}：{} {}",
                output.status,
                stdout.trim(),
                stderr.trim()
            ))
        }
    }
}

fn capture(input: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut contents = Vec::new();
    input
        .take(limit + 1)
        .read_to_end(&mut contents)
        .map_err(|error| error.to_string())?;
    if contents.len() as u64 > limit {
        return Err("ADB 输出超过限制".into());
    }
    Ok(contents)
}

pub struct ManagedChild {
    child: Child,
    #[cfg(windows)]
    _job: Option<ChildJob>,
}

impl ManagedChild {
    fn spawn(command: &mut Command) -> Result<Self> {
        let child = command.spawn().map_err(|error| error.to_string())?;
        Ok(Self {
            #[cfg(windows)]
            _job: ChildJob::attach(&child),
            child,
        })
    }

    fn spawn_unmanaged(command: &mut Command) -> Result<Self> {
        let child = command.spawn().map_err(|error| error.to_string())?;
        Ok(Self {
            child,
            #[cfg(windows)]
            _job: None,
        })
    }

    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.running() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(windows)]
struct ChildJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ChildJob {
    fn attach(child: &Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle == 0 {
            return None;
        }
        let job = Self(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0
            || unsafe { AssignProcessToJobObject(handle, child.as_raw_handle() as _) } == 0
        {
            return None;
        }
        Some(job)
    }
}

#[cfg(windows)]
impl Drop for ChildJob {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

pub enum SessionEvent {
    Output(u64, Vec<u8>),
    Ended(u64, Result<()>),
}

struct Input {
    bytes: Vec<u8>,
    size: TerminalSize,
}

type SerialReader = Box<dyn FnOnce(&mut dyn FnMut(&[u8]), &AtomicBool) -> io::Result<()> + Send>;
type SerialWriter = Box<dyn FnMut(Input, &AtomicBool) -> io::Result<()> + Send>;

pub struct Connection {
    input: Option<Sender<Input>>,
    stop: Arc<AtomicBool>,
    socket: Option<TcpStream>,
    child: Option<ManagedChild>,
    workers: Vec<JoinHandle<()>>,
}

impl Connection {
    pub fn serial(config: SerialConfig, id: u64, events: Sender<SessionEvent>) -> Result<Self> {
        let (reader, writer) = serial_pair(&config)?;
        Ok(Self::spawn(id, events, None, None, reader, writer))
    }

    pub fn shell(
        adb: &Adb,
        device: &str,
        size: TerminalSize,
        id: u64,
        events: Sender<SessionEvent>,
    ) -> Result<Self> {
        let executable = adb.ensure_started()?;
        if let Some(endpoint) = adb_server::tcp_endpoint() {
            if let Some(stream) = adb_shell_protocol::open(&endpoint, device, size)
                .map_err(|error| error.to_string())?
            {
                let mut reader = stream.try_clone().map_err(|error| error.to_string())?;
                let mut writer = stream.try_clone().map_err(|error| error.to_string())?;
                let mut current = size;
                return Ok(Self::spawn(
                    id,
                    events,
                    Some(stream),
                    None,
                    move |emit, _| {
                        adb_shell_protocol::read_output(&mut reader, emit).and_then(|status| {
                            if status == 0 {
                                Ok(())
                            } else {
                                Err(io::Error::other(format!("ADB Shell 退出码 {status}")))
                            }
                        })
                    },
                    move |input, _| {
                        adb_shell_protocol::update_and_write(
                            &mut writer,
                            &mut current,
                            input.size,
                            &input.bytes,
                        )
                    },
                ));
            }
        }

        let mut command = command(&executable);
        command
            .args(["-s", device, "shell", "-tt"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = ManagedChild::spawn(&mut command)?;
        let mut stdout = child
            .child
            .stdout
            .take()
            .ok_or("ADB Shell 输出管道不可用")?;
        let mut stdin = child.child.stdin.take().ok_or("ADB Shell 输入管道不可用")?;
        Ok(Self::spawn(
            id,
            events,
            None,
            Some(child),
            move |emit, stop| read_loop(stop, emit, |buffer| stdout.read(buffer)),
            move |input, _| stdin.write_all(&input.bytes),
        ))
    }

    fn spawn(
        id: u64,
        events: Sender<SessionEvent>,
        socket: Option<TcpStream>,
        child: Option<ManagedChild>,
        read: impl FnOnce(&mut dyn FnMut(&[u8]), &AtomicBool) -> io::Result<()> + Send + 'static,
        mut write: impl FnMut(Input, &AtomicBool) -> io::Result<()> + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (send, receive) = mpsc::channel();
        let reader_events = events.clone();
        let reader_stop = stop.clone();
        let reader = thread::spawn(move || {
            let result = read(
                &mut |bytes| {
                    let _ = reader_events.send(SessionEvent::Output(id, bytes.to_vec()));
                },
                &reader_stop,
            );
            if !reader_stop.load(Ordering::Relaxed) {
                let _ = reader_events.send(SessionEvent::Ended(
                    id,
                    result.map_err(|error| error.to_string()),
                ));
            }
        });
        let writer_stop = stop.clone();
        let writer = thread::spawn(move || {
            while let Ok(input) = receive.recv() {
                if writer_stop.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(error) = write(input, &writer_stop) {
                    if !writer_stop.load(Ordering::Relaxed) {
                        let _ = events.send(SessionEvent::Ended(id, Err(error.to_string())));
                    }
                    break;
                }
            }
        });
        Self {
            input: Some(send),
            stop,
            socket,
            child,
            workers: vec![reader, writer],
        }
    }

    pub fn write(&self, bytes: Vec<u8>, size: TerminalSize) {
        if let Some(send) = &self.input {
            let _ = send.send(Input { bytes, size });
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.input.take();
        if let Some(socket) = self.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        self.child.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn read_loop(
    stop: &AtomicBool,
    emit: &mut dyn FnMut(&[u8]),
    mut read: impl FnMut(&mut [u8]) -> io::Result<usize>,
) -> io::Result<()> {
    let mut buffer = [0; 4096];
    while !stop.load(Ordering::Relaxed) {
        match read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => emit(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn serial_pair(config: &SerialConfig) -> Result<(SerialReader, SerialWriter)> {
    let (mut reader, mut writer) = crate::windows_serial::open_serial_pair(config)?;
    Ok((
        Box::new(move |emit: &mut dyn FnMut(&[u8]), stop: &AtomicBool| {
            read_loop(stop, emit, |buffer| reader.read(buffer, stop))
        }),
        Box::new(move |input: Input, stop: &AtomicBool| writer.write_all(&input.bytes, stop)),
    ))
}

#[cfg(not(windows))]
fn serial_pair(config: &SerialConfig) -> Result<(SerialReader, SerialWriter)> {
    use serialport::{DataBits, Parity, StopBits};
    let mut writer = serialport::new(&config.port_name, config.baud_rate)
        .timeout(Duration::from_millis(SERIAL_READ_TIMEOUT_MS))
        .data_bits(match config.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        })
        .parity(match config.parity.as_str() {
            "odd" => Parity::Odd,
            "even" => Parity::Even,
            _ => Parity::None,
        })
        .stop_bits(if config.stop_bits == 2 {
            StopBits::Two
        } else {
            StopBits::One
        })
        .open()
        .map_err(|error| error.to_string())?;
    let mut reader = writer.try_clone().map_err(|error| error.to_string())?;
    Ok((
        Box::new(move |emit: &mut dyn FnMut(&[u8]), stop: &AtomicBool| {
            read_loop(stop, emit, |buffer| reader.read(buffer))
        }),
        Box::new(move |input: Input, _: &AtomicBool| writer.write_all(&input.bytes)),
    ))
}
