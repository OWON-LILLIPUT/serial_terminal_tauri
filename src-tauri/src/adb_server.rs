use serde::Serialize;
use std::{
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::Mutex,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdbServerPhase {
    #[default]
    Idle,
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AdbServerStatus {
    pub phase: AdbServerPhase,
    pub message: String,
    pub revision: u64,
}

#[derive(Default)]
pub struct AdbServerManager {
    startup_lock: Mutex<()>,
    status: Mutex<AdbServerStatus>,
}

impl AdbServerManager {
    pub fn snapshot(&self) -> Result<AdbServerStatus, String> {
        self.status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| "ADB 初始化状态锁已损坏".to_string())
    }

    pub fn publish(
        &self,
        phase: AdbServerPhase,
        message: String,
        notify: impl FnOnce(AdbServerStatus),
    ) -> Result<(), String> {
        let snapshot = {
            let mut status = self
                .status
                .lock()
                .map_err(|_| "ADB 初始化状态锁已损坏".to_string())?;
            status.phase = phase;
            status.message = message;
            status.revision += 1;
            status.clone()
        };
        notify(snapshot);
        Ok(())
    }

    pub fn ensure_started(
        &self,
        probe: impl FnOnce() -> bool,
        start: impl FnOnce() -> Result<(), String>,
        notify: impl Fn(AdbServerStatus),
    ) -> Result<(), String> {
        let observed_revision = self.snapshot()?.revision;
        let _guard = self
            .startup_lock
            .lock()
            .map_err(|_| "ADB server 启动锁已损坏".to_string())?;
        let status = self.snapshot()?;

        // Calls already waiting for an attempt share its failure, rather than each
        // starting another full timeout. A later user action may start a new attempt.
        if status.revision != observed_revision && status.phase == AdbServerPhase::Failed {
            return Err(status.message);
        }
        if status.phase == AdbServerPhase::Ready && probe() {
            return Ok(());
        }

        self.publish(AdbServerPhase::Starting, "ADB 正在启动".into(), &notify)?;
        match start() {
            Ok(()) => self.publish(AdbServerPhase::Ready, String::new(), notify),
            Err(error) => {
                self.publish(AdbServerPhase::Failed, error.clone(), notify)?;
                Err(error)
            }
        }
    }
}

pub fn server_is_running() -> bool {
    let Some(endpoint) = server_endpoint(
        env::var("ADB_SERVER_SOCKET").ok().as_deref(),
        env::var("ANDROID_ADB_SERVER_ADDRESS").ok().as_deref(),
        env::var("ANDROID_ADB_SERVER_PORT").ok().as_deref(),
    ) else {
        // Let adb start-server handle non-TCP sockets or unsupported configuration.
        return false;
    };
    server_responds_at(&endpoint)
}

fn server_endpoint(socket: Option<&str>, host: Option<&str>, port: Option<&str>) -> Option<String> {
    if let Some(socket) = socket {
        let tcp = socket.strip_prefix("tcp:")?;
        if tcp.parse::<u16>().is_ok() {
            return Some(format!("127.0.0.1:{tcp}"));
        }
        return Some(tcp.to_string());
    }
    let host = host.unwrap_or("127.0.0.1");
    let port = port.unwrap_or("5037").parse::<u16>().ok()?;
    if host.contains(':') && !host.starts_with('[') {
        Some(format!("[{host}]:{port}"))
    } else {
        Some(format!("{host}:{port}"))
    }
}

fn server_responds_at(endpoint: &str) -> bool {
    let timeout = Duration::from_millis(150);
    let Ok(addresses) = endpoint.to_socket_addrs() else {
        return false;
    };
    for address in addresses {
        let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
            continue;
        };
        if stream.set_read_timeout(Some(timeout)).is_err()
            || stream.set_write_timeout(Some(timeout)).is_err()
        {
            continue;
        }
        // A read-only ADB smart-socket request verifies the server, not just an
        // open port. It never starts a daemon or changes a device's state.
        if stream.write_all(b"000Chost:version").is_err() {
            continue;
        }
        let mut response = [0; 4];
        if stream.read_exact(&mut response).is_ok() && &response == b"OKAY" {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc, Arc, Barrier,
        },
        thread,
    };

    #[test]
    fn status_reads_do_not_wait_for_slow_startup() {
        let manager = Arc::new(AdbServerManager::default());
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let worker_manager = Arc::clone(&manager);
        let worker = thread::spawn(move || {
            worker_manager.ensure_started(
                || false,
                || {
                    started_tx.send(()).unwrap();
                    finish_rx.recv().unwrap();
                    Ok(())
                },
                |_| {},
            )
        });
        started_rx.recv().unwrap();
        let (status_tx, status_rx) = mpsc::channel();
        thread::spawn(move || status_tx.send(manager.snapshot()).unwrap());
        let status = status_rx.recv_timeout(Duration::from_millis(500));
        finish_tx.send(()).unwrap();
        worker.join().unwrap().unwrap();
        assert_eq!(status.unwrap().unwrap().phase, AdbServerPhase::Starting);
    }

    #[test]
    fn concurrent_cold_start_runs_once() {
        let manager = Arc::new(AdbServerManager::default());
        let running = Arc::new(AtomicBool::new(false));
        let starts = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let running = Arc::clone(&running);
                let starts = Arc::clone(&starts);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    manager.ensure_started(
                        || running.load(Ordering::SeqCst),
                        || {
                            starts.fetch_add(1, Ordering::SeqCst);
                            assert_eq!(manager.snapshot().unwrap().phase, AdbServerPhase::Starting);
                            thread::sleep(Duration::from_millis(50));
                            running.store(true, Ordering::SeqCst);
                            Ok(())
                        },
                        |_| {},
                    )
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(manager.snapshot().unwrap().phase, AdbServerPhase::Ready);
    }

    #[test]
    fn stopped_server_is_restarted_and_failure_can_be_retried() {
        let manager = AdbServerManager::default();
        let starts = AtomicUsize::new(0);
        let start = || {
            starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        };
        manager.ensure_started(|| false, start, |_| {}).unwrap();
        manager.ensure_started(|| true, start, |_| {}).unwrap();
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        manager
            .ensure_started(|| false, || Err("startup failed".into()), |_| {})
            .unwrap_err();
        assert_eq!(manager.snapshot().unwrap().phase, AdbServerPhase::Failed);
        manager.ensure_started(|| false, start, |_| {}).unwrap();
        assert_eq!(starts.load(Ordering::SeqCst), 2);
        assert_eq!(manager.snapshot().unwrap().phase, AdbServerPhase::Ready);
    }

    #[test]
    fn probe_requires_adb_response_and_detects_closed_server() {
        for reply in [*b"OKAY", *b"FAIL"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = listener.local_addr().unwrap().to_string();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 16];
                stream.read_exact(&mut request).unwrap();
                assert_eq!(&request, b"000Chost:version");
                stream.write_all(&reply).unwrap();
            });
            assert_eq!(server_responds_at(&endpoint), &reply == b"OKAY");
            server.join().unwrap();
            assert!(!server_responds_at(&endpoint));
        }
    }

    #[test]
    fn endpoint_honors_server_configuration() {
        assert_eq!(server_endpoint(None, None, None).unwrap(), "127.0.0.1:5037");
        assert_eq!(
            server_endpoint(Some("tcp:1234"), None, None).unwrap(),
            "127.0.0.1:1234"
        );
        assert_eq!(
            server_endpoint(Some("tcp:[::1]:1234"), None, None).unwrap(),
            "[::1]:1234"
        );
        assert_eq!(
            server_endpoint(None, Some("::1"), Some("1234")).unwrap(),
            "[::1]:1234"
        );
        assert!(server_endpoint(Some("localfilesystem:/tmp/adb.sock"), None, None).is_none());
    }
}
