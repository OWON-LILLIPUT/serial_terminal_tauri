use super::*;

const WAIT: Duration = Duration::from_secs(2);

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct WorkerFlags {
    reader_dropped: Arc<AtomicBool>,
    writer_dropped: Arc<AtomicBool>,
    writer_woken: Arc<AtomicBool>,
}

impl WorkerFlags {
    fn start(&self, state: &mut SerialState) {
        let reader_stop = Arc::new(AtomicBool::new(false));
        let reader_flag = Arc::clone(&reader_stop);
        let reader_resource = DropFlag(Arc::clone(&self.reader_dropped));
        state.reader = Some(thread::spawn(move || {
            let _resource = reader_resource;
            while !reader_flag.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(1));
            }
        }));
        state.reader_stop = Some(reader_stop);

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let writer_stop = Arc::new(AtomicBool::new(false));
        let writer_flag = Arc::clone(&writer_stop);
        let writer_resource = DropFlag(Arc::clone(&self.writer_dropped));
        let woken = Arc::clone(&self.writer_woken);
        state.writer = Some(thread::spawn(move || {
            let _resource = writer_resource;
            // Keep a timeout so a broken wakeup fails rather than hangs tests.
            if rx.recv_timeout(WAIT).is_ok() {
                woken.store(true, Ordering::SeqCst);
            }
            assert!(writer_flag.load(Ordering::Relaxed));
        }));
        state.writer_stop = Some(writer_stop);
        state.writer_tx = Some(tx);
    }

    fn assert_released(&self) {
        assert!(self.reader_dropped.load(Ordering::SeqCst));
        assert!(self.writer_dropped.load(Ordering::SeqCst));
        assert!(self.writer_woken.load(Ordering::SeqCst));
    }
}

fn assert_empty(state: &SerialState) {
    assert!(state.reader_stop.is_none());
    assert!(state.reader.is_none());
    assert!(state.writer_stop.is_none());
    assert!(state.writer_tx.is_none());
    assert!(state.writer.is_none());
}

#[test]
fn disconnect_releases_both_workers_even_with_an_inflight_sender() {
    let manager = SerialManager::default();
    let flags = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| flags.start(state)).unwrap();
    let inflight_sender = manager.inner.lock().unwrap().writer_tx.clone().unwrap();
    close_serial_manager(&manager).unwrap();
    flags.assert_released();
    assert!(inflight_sender.send(vec![1]).is_err());
    let state = manager.inner.lock().unwrap();
    assert!(!state.closing);
    assert_empty(&state);
}

#[test]
fn normal_disconnect_still_allows_reconnecting() {
    let manager = SerialManager::default();
    let first = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| first.start(state)).unwrap();
    close_serial_manager(&manager).unwrap();
    first.assert_released();
    let second = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| second.start(state)).unwrap();
    shutdown_serial_manager(&manager);
    second.assert_released();
}

#[test]
fn shutdown_is_idempotent_and_rejects_new_driver_opens() {
    let manager = SerialManager::default();
    let flags = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| flags.start(state)).unwrap();
    shutdown_serial_manager(&manager);
    shutdown_serial_manager(&manager);
    close_serial_manager(&manager).unwrap();
    flags.assert_released();
    let result = connect_serial_with(
        &manager,
        || -> Result<(), String> { panic!("driver must not be opened during shutdown") },
        |_, ()| panic!("worker must not start during shutdown"),
    );
    assert!(result.unwrap_err().contains("正在关闭"));
    let state = manager.inner.lock().unwrap();
    assert!(state.closing);
    assert_empty(&state);
}

#[test]
fn closing_during_a_slow_open_drops_late_handles_without_starting_workers() {
    for shutdown in [false, true] {
        let manager = Arc::new(SerialManager::default());
        let pending_manager = Arc::clone(&manager);
        let dropped = Arc::new(AtomicBool::new(false));
        let resource = DropFlag(Arc::clone(&dropped));
        let (opened_tx, opened_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let pending = thread::spawn(move || {
            connect_serial_with(
                &pending_manager,
                || {
                    opened_tx.send(()).unwrap();
                    resume_rx.recv_timeout(WAIT).unwrap();
                    Ok(resource)
                },
                |_, _| panic!("cancelled connection must not start workers"),
            )
        });
        opened_rx.recv_timeout(WAIT).unwrap();
        if shutdown {
            shutdown_serial_manager(&manager);
        } else {
            close_serial_manager(&manager).unwrap();
        }
        resume_tx.send(()).unwrap();
        assert_eq!(pending.join().unwrap().unwrap_err(), "串口连接已取消");
        assert!(dropped.load(Ordering::SeqCst));
        assert_empty(&manager.inner.lock().unwrap());
        if !shutdown {
            connect_serial_with(&manager, || Ok(()), |_, ()| {}).unwrap();
        }
    }
}

#[test]
fn newer_connection_is_not_overwritten_by_an_older_slow_open() {
    let manager = Arc::new(SerialManager::default());
    let pending_manager = Arc::clone(&manager);
    let dropped = Arc::new(AtomicBool::new(false));
    let resource = DropFlag(Arc::clone(&dropped));
    let (opened_tx, opened_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let pending = thread::spawn(move || {
        connect_serial_with(
            &pending_manager,
            || {
                opened_tx.send(()).unwrap();
                resume_rx.recv_timeout(WAIT).unwrap();
                Ok(resource)
            },
            |_, _| panic!("superseded connection must not start workers"),
        )
    });
    opened_rx.recv_timeout(WAIT).unwrap();
    let current = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| current.start(state)).unwrap();
    resume_tx.send(()).unwrap();
    assert!(pending.join().unwrap().is_err());
    assert!(dropped.load(Ordering::SeqCst));
    assert!(manager.inner.lock().unwrap().writer_tx.is_some());
    shutdown_serial_manager(&manager);
    current.assert_released();
}

#[test]
fn failed_open_releases_previous_connection_and_can_retry() {
    let manager = SerialManager::default();
    let flags = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| flags.start(state)).unwrap();
    let result = connect_serial_with(
        &manager,
        || Err::<(), _>("simulated driver failure".to_string()),
        |_, ()| panic!("failed open must not start workers"),
    );
    assert_eq!(result.unwrap_err(), "simulated driver failure");
    flags.assert_released();
    assert_empty(&manager.inner.lock().unwrap());
    connect_serial_with(&manager, || Ok(()), |_, ()| {}).unwrap();
}

fn poison(manager: &Arc<SerialManager>) {
    let manager = Arc::clone(manager);
    assert!(thread::spawn(move || {
        let _state = manager.inner.lock().unwrap();
        panic!("intentional test mutex poison");
    })
    .join()
    .is_err());
}

#[test]
fn shutdown_still_cleans_workers_after_a_poisoned_state_lock() {
    let manager = Arc::new(SerialManager::default());
    let flags = WorkerFlags::default();
    connect_serial_with(&manager, || Ok(()), |state, ()| flags.start(state)).unwrap();
    poison(&manager);
    shutdown_serial_manager(&manager);
    flags.assert_released();
    assert!(manager.inner.is_poisoned());
    let state = manager
        .inner
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert_empty(&state);
}

#[test]
fn dropping_manager_cleans_workers_with_or_without_a_poisoned_lock() {
    for poisoned in [false, true] {
        let manager = Arc::new(SerialManager::default());
        let flags = WorkerFlags::default();
        connect_serial_with(&manager, || Ok(()), |state, ()| flags.start(state)).unwrap();
        if poisoned {
            poison(&manager);
        }
        drop(manager);
        flags.assert_released();
    }
}
