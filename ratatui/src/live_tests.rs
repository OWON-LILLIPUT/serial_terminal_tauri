use crate::{
    adb_shell_protocol::TerminalSize,
    backend::{Adb, Connection, SessionEvent},
    files,
};
use std::{
    env, fs,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

fn output_until(receiver: &Receiver<SessionEvent>, predicate: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut output = Vec::new();
    while let Ok(event) = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        match event {
            SessionEvent::Output(_, bytes) => {
                output.extend(bytes);
                if predicate(&String::from_utf8_lossy(&output)) {
                    return String::from_utf8_lossy(&output).into_owned();
                }
            }
            SessionEvent::Ended(_, result) => panic!("Terminal session closed early: {result:?}"),
        }
    }
    panic!(
        "timed out waiting for the ADB shell; got {:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
#[ignore = "requires an idle, connected serial console and an explicit port name"]
fn serial_console_survives_idle_and_releases_port() {
    let config = crate::SerialConfig {
        port_name: env::var("SERIAL_TERMINAL_SMOKE_PORT").expect("set SERIAL_TERMINAL_SMOKE_PORT"),
        ..crate::SerialConfig::default()
    };
    let (send, receive) = mpsc::channel();
    let connection = Connection::serial(config.clone(), 1, send.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    for event in receive.try_iter() {
        if let SessionEvent::Ended(_, result) = event {
            panic!("Serial disconnected while idle: {result:?}");
        }
    }
    let size = TerminalSize { rows: 24, cols: 80 };
    // Redraw the Android console prompt without submitting the current line.
    connection.write(vec![12], size);
    output_until(&receive, |output| {
        output.contains("2253005:") || output.contains("console:")
    });
    drop(connection);
    drop(Connection::serial(config, 2, send).expect("port was not released on close"));
}

#[test]
#[ignore = "requires an explicit online Android device ID"]
fn adb_shell_and_file_operations() {
    let device =
        env::var("SERIAL_TERMINAL_SMOKE_DEVICE").expect("set SERIAL_TERMINAL_SMOKE_DEVICE");
    let adb = Adb::default();
    assert!(adb
        .devices()
        .unwrap()
        .iter()
        .any(|entry| entry.id == device && entry.online()));
    let (send, receive) = mpsc::channel();
    let wide = TerminalSize {
        rows: 24,
        cols: 100,
    };
    let narrow = TerminalSize { rows: 18, cols: 48 };
    let shell = Connection::shell(&adb, &device, wide, 1, send).unwrap();
    shell.write(b"stty size\r".to_vec(), wide);
    output_until(&receive, |output| {
        output.lines().any(|line| line.trim() == "24 100")
    });
    shell.write(b"stty size\r".to_vec(), narrow);
    output_until(&receive, |output| {
        output.lines().any(|line| line.trim() == "18 48")
    });
    shell.write("printf '%s\\n' 中文\r".as_bytes().to_vec(), narrow);
    output_until(&receive, |output| {
        output.lines().any(|line| line.trim() == "中文")
    });
    shell.write(
        format!(": {} /system/etc/selinu\t", "a".repeat(72)).into_bytes(),
        narrow,
    );
    output_until(&receive, |output| output.contains("/system/etc/selinux/"));
    drop(shell);

    let remote = adb
        .execute(&[
            "-s",
            &device,
            "shell",
            "mktemp",
            "-d",
            "/data/local/tmp/serial-ratatui-smoke.XXXXXX",
        ])
        .unwrap();
    const PREFIX: &str = "/data/local/tmp/serial-ratatui-smoke.";
    assert!(remote.strip_prefix(PREFIX).is_some_and(
        |name| name.len() == 6 && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
    ));
    struct Remote<'a> {
        adb: &'a Adb,
        device: &'a str,
        path: &'a str,
    }
    impl Drop for Remote<'_> {
        fn drop(&mut self) {
            let _ = self
                .adb
                .execute(&["-s", self.device, "shell", "rm", "-r", "--", self.path]);
        }
    }
    let _cleanup = Remote {
        adb: &adb,
        device: &device,
        path: &remote,
    };
    let local = tempfile::tempdir().unwrap();
    let source = local.path().join("上传 payload.bin");
    fs::write(&source, b"ratatui smoke\x00\xff").unwrap();
    let folder = local.path().join("样例");
    fs::create_dir_all(folder.join("空目录")).unwrap();
    fs::create_dir_all(folder.join("子目录")).unwrap();
    fs::copy(&source, folder.join("子目录").join("内容.bin")).unwrap();
    fs::copy(&source, folder.join("payload.bin")).unwrap();
    files::mkdir(&adb, &device, &remote, "temp").unwrap();
    files::upload(
        &adb,
        &device,
        &remote,
        &[
            (source.clone(), ".upload 测试.bin".into()),
            (folder, "folder 样例".into()),
        ],
    )
    .unwrap();
    let entries = files::list(&adb, &device, &remote).unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.name == "temp" && entry.directory));
    let uploaded = entries
        .iter()
        .find(|entry| entry.name == ".upload 测试.bin")
        .unwrap();
    files::rename(&adb, &device, &uploaded.path, "download 测试.bin").unwrap();
    let entries = files::list(&adb, &device, &remote).unwrap();
    let renamed = entries
        .iter()
        .find(|entry| entry.name == "download 测试.bin")
        .unwrap();
    let destination = local.path().join("received.bin");
    files::download(&adb, &device, renamed, &destination).unwrap();
    assert_eq!(fs::read(&source).unwrap(), fs::read(destination).unwrap());
    let uploaded_folder = entries
        .iter()
        .find(|entry| entry.name == "folder 样例" && entry.directory)
        .unwrap();
    let directory = local.path().join("received folder");
    files::download(&adb, &device, uploaded_folder, &directory).unwrap();
    assert!(directory.join("空目录").is_dir());
    assert_eq!(
        fs::read(&source).unwrap(),
        fs::read(directory.join("子目录").join("内容.bin")).unwrap()
    );
    assert_eq!(
        fs::read(&source).unwrap(),
        fs::read(directory.join("payload.bin")).unwrap()
    );
    files::delete(&adb, &device, &entries).unwrap();
    assert!(files::list(&adb, &device, &remote).unwrap().is_empty());
}
