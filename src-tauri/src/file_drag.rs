//! The OLE input loop belongs to the WebView UI thread. ADB content is read
//! lazily through thread-safe streams, with explicit asynchronous completion.
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom},
    mem::{size_of, ManuallyDrop},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex, OnceLock, Weak,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{implement, Ref, Result, BOOL, HRESULT, PWSTR},
    Win32::{
        Foundation::*,
        Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        System::{
            Com::*,
            DataExchange::RegisterClipboardFormatW,
            Memory::{
                GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
            },
            Ole::*,
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, GetCapture, ReleaseCapture, VK_LBUTTON},
            Shell::*,
            WindowsAndMessaging::{KillTimer, SendMessageW, SetTimer, WM_CANCELMODE},
        },
    },
};

static STAGE_NUMBER: AtomicU64 = AtomicU64::new(0);
const CANCELLED: &str = "已取消传输";
pub struct DragPayload {
    pub directory: PathBuf,
    pub path: PathBuf,
}
impl DragPayload {
    pub fn create() -> std::result::Result<Self, String> {
        let number = STAGE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "serial-terminal-drag-{}-{stamp}-{number}",
            std::process::id()
        ));
        fs::create_dir(&directory).map_err(|e| format!("创建拖出临时目录失败: {e}"))?;
        Ok(Self {
            path: directory.join("payload"),
            directory,
        })
    }
}
impl Drop for DragPayload {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub fn validate_windows_name(name: &str) -> std::result::Result<(), String> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.encode_utf16().count() >= 260
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.chars().count() == 4
            && matches!(stem.chars().last(), Some('1'..='9' | '¹' | '²' | '³')))
    {
        return Err("该文件名不兼容 Windows，请先在设备端重命名或使用下载按钮另存为".into());
    }
    Ok(())
}

type PayloadResult = std::result::Result<Arc<DragPayload>, String>;
type Loader = Box<dyn Fn(usize) -> PayloadResult + Send + Sync>;
#[derive(Default)]
struct Completion {
    drag: Option<std::result::Result<(), String>>,
    asynchronous: bool,
    end: Option<std::result::Result<(), String>>,
}
pub struct DragOperation {
    names: Vec<String>,
    loader: Loader,
    files: Vec<OnceLock<PayloadResult>>,
    pub cancel: Arc<AtomicBool>,
    async_mode: AtomicBool,
    content_requested: AtomicBool,
    completion: Mutex<Completion>,
    changed: Condvar,
    failure: Mutex<Option<String>>,
}
impl DragOperation {
    pub fn new(
        names: Vec<String>,
        cancel: Arc<AtomicBool>,
        loader: Loader,
    ) -> std::result::Result<Arc<Self>, String> {
        if names.is_empty() || names.len() > 256 {
            return Err("一次请拖出 1–256 个普通文件".into());
        }
        for name in &names {
            validate_windows_name(name)?;
        }
        Ok(Arc::new(Self {
            files: (0..names.len()).map(|_| OnceLock::new()).collect(),
            names,
            loader,
            cancel,
            async_mode: AtomicBool::new(true),
            content_requested: AtomicBool::new(false),
            completion: Mutex::new(Completion::default()),
            changed: Condvar::new(),
            failure: Mutex::new(None),
        }))
    }
    fn payload(&self, index: usize) -> Result<Arc<DragPayload>> {
        self.check_cancel()?;
        let result = self
            .files
            .get(index)
            .ok_or(windows::core::Error::from(DV_E_LINDEX))?
            .get_or_init(|| (self.loader)(index));
        self.check_cancel()?;
        match result {
            Ok(payload) => Ok(Arc::clone(payload)),
            Err(error) => {
                self.record_error(error.clone());
                Err(E_FAIL.into())
            }
        }
    }
    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(STG_E_REVERTED.into())
        } else {
            Ok(())
        }
    }
    fn record_error(&self, message: String) {
        if let Ok(mut failure) = self.failure.lock() {
            failure.get_or_insert(message);
        }
        self.changed.notify_all();
    }
    fn end_drag(&self, result: std::result::Result<(), String>) {
        if let Ok(mut completion) = self.completion.lock() {
            completion.drag = Some(result);
        }
        self.changed.notify_all();
    }
    /// Called only through WebviewWindow::run_on_main_thread.
    pub fn drag(self: &Arc<Self>) -> std::result::Result<(), String> {
        let result = self.drag_inner();
        self.end_drag(result.clone());
        result
    }
    fn drag_inner(self: &Arc<Self>) -> std::result::Result<(), String> {
        unsafe {
            OleInitialize(None).map_err(|e| format!("初始化拖放失败: {e}"))?;
        }
        let _ole = OleApartment;
        if self.cancel.load(Ordering::SeqCst) || !left_pressed() {
            return Err(format!("{CANCELLED}：鼠标已松开"));
        }
        let object = virtual_files(Arc::clone(self)).map_err(|e| e.to_string())?;
        let source: IDropSource = CopySource {
            cancel: Arc::clone(&self.cancel),
        }
        .into();
        let _watchdog = DragWatchdog::new(self)?;
        unsafe {
            let _ = ReleaseCapture();
        }
        let mut effect = DROPEFFECT_NONE;
        let result = unsafe { DoDragDrop(&object, &source, DROPEFFECT_COPY, &mut effect) };
        if self.cancel.load(Ordering::SeqCst) || result == DRAGDROP_S_CANCEL {
            return Err(CANCELLED.into());
        }
        result.ok().map_err(|e| format!("拖出下载失败: {e}"))?;
        if result != DRAGDROP_S_DROP || effect != DROPEFFECT_COPY {
            return Err(format!("{CANCELLED}：未投放到可接收文件的位置"));
        }
        Ok(())
    }
    /// Wait on a worker: the Shell may finish copying after the input loop exits.
    pub fn wait(&self) -> std::result::Result<(), String> {
        let started = Instant::now();
        let mut completion = self.completion.lock().map_err(|_| "拖放状态不可用")?;
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(CANCELLED.into());
            }
            if completion.drag.is_some() {
                if let Some(error) = self.failure.lock().map_err(|_| "拖放状态不可用")?.clone()
                {
                    return Err(error);
                }
            }
            if let Some(Err(error)) = &completion.drag {
                return Err(error.clone());
            }
            if let Some(Ok(())) = &completion.drag {
                if !completion.asynchronous || completion.end.is_some() {
                    if let Some(result) = &completion.end {
                        result.clone()?;
                    }
                    if self.files.iter().all(|file| file.get().is_none()) {
                        return Err(format!("{CANCELLED}：目标没有读取文件"));
                    }
                    return Ok(());
                }
            }
            // Does not impose a short timeout on slow ADB startup or large files.
            if started.elapsed() >= Duration::from_secs(24 * 60 * 60) {
                self.cancel.store(true, Ordering::SeqCst);
                return Err("目标复制操作未结束，已取消".into());
            }
            completion = self
                .changed
                .wait_timeout(completion, Duration::from_millis(100))
                .map_err(|_| "拖放状态不可用")?
                .0;
        }
    }
}

struct OleApartment;
impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}
fn left_pressed() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}
fn continue_drag(escape: bool, cancelled: bool, left: bool) -> HRESULT {
    if escape || cancelled {
        DRAGDROP_S_CANCEL
    } else if !left {
        DRAGDROP_S_DROP
    } else {
        S_OK
    }
}
#[implement(IDropSource)]
struct CopySource {
    cancel: Arc<AtomicBool>,
}
impl IDropSource_Impl for CopySource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, _: MODIFIERKEYS_FLAGS) -> HRESULT {
        continue_drag(
            escape.as_bool(),
            self.cancel.load(Ordering::SeqCst),
            left_pressed(),
        )
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

thread_local! { static DRAG_CONTEXT: RefCell<Option<(Weak<DragOperation>, Option<Instant>)>> = const { RefCell::new(None) }; }
struct DragWatchdog(usize);
impl DragWatchdog {
    fn new(operation: &Arc<DragOperation>) -> std::result::Result<Self, String> {
        if DRAG_CONTEXT.with(|context| context.borrow().is_some()) {
            return Err("已有原生拖动正在进行".into());
        }
        DRAG_CONTEXT
            .with(|context| *context.borrow_mut() = Some((Arc::downgrade(operation), None)));
        let id = unsafe { SetTimer(None, 0, 50, Some(drag_timer)) };
        if id == 0 {
            DRAG_CONTEXT.with(|context| *context.borrow_mut() = None);
            return Err("启动拖动释放检测失败".into());
        }
        Ok(Self(id))
    }
}
impl Drop for DragWatchdog {
    fn drop(&mut self) {
        unsafe {
            let _ = KillTimer(None, self.0);
        }
        DRAG_CONTEXT.with(|context| *context.borrow_mut() = None);
    }
}
unsafe extern "system" fn drag_timer(_: HWND, _: u32, _: usize, _: u32) {
    let cancel_capture = DRAG_CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let Some((operation, released)) = context.as_mut() else {
            return false;
        };
        let Some(operation) = operation.upgrade() else {
            return false;
        };
        if operation.cancel.load(Ordering::SeqCst) {
            return true;
        }
        if left_pressed() {
            *released = None;
            return false;
        }
        let elapsed = released.get_or_insert_with(Instant::now).elapsed();
        elapsed > Duration::from_millis(500)
            && !operation.content_requested.load(Ordering::SeqCst)
            && !operation
                .completion
                .lock()
                .map(|state| state.asynchronous)
                .unwrap_or(true)
    });
    if cancel_capture {
        let capture = unsafe { GetCapture() };
        if !capture.is_invalid() {
            unsafe {
                SendMessageW(capture, WM_CANCELMODE, None, None);
            }
        }
    }
}

fn format(id: u16, medium: TYMED, index: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index,
        tymed: medium.0 as u32,
        ..Default::default()
    }
}
fn clipboard(name: windows::core::PCWSTR) -> Result<u16> {
    let id = unsafe { RegisterClipboardFormatW(name) };
    if id == 0 {
        Err(windows::core::Error::from_win32())
    } else {
        Ok(id as u16)
    }
}
fn global(bytes: &[u8]) -> Result<STGMEDIUM> {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1))?;
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(E_OUTOFMEMORY.into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
        let _ = GlobalUnlock(memory);
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: memory },
            ..Default::default()
        })
    }
}
fn descriptors(names: &[String]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + names.len() * size_of::<FILEDESCRIPTORW>());
    bytes.extend_from_slice(&(names.len() as u32).to_le_bytes());
    for name in names {
        let mut filename = [0u16; 260];
        for (slot, value) in filename.iter_mut().zip(name.encode_utf16()) {
            *slot = value;
        }
        let descriptor = FILEDESCRIPTORW {
            dwFlags: (FD_ATTRIBUTES.0 | FD_PROGRESSUI.0 | FD_UNICODE.0) as u32,
            dwFileAttributes: FILE_ATTRIBUTE_NORMAL.0,
            cFileName: filename,
            ..Default::default()
        };
        bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                (&descriptor as *const FILEDESCRIPTORW).cast(),
                size_of::<FILEDESCRIPTORW>(),
            )
        });
    }
    bytes
}

// All fields are Send + Sync. Agile marshaling dispatches Explorer's extraction
// to worker threads instead of the source UI thread.
#[implement(IDataObject, IDataObjectAsyncCapability)]
struct VirtualFiles {
    operation: Arc<DragOperation>,
    descriptor: u16,
    contents: u16,
    preferred: u16,
    private: Mutex<HashMap<(u16, i32), Vec<u8>>>,
}
fn virtual_files(operation: Arc<DragOperation>) -> Result<IDataObject> {
    Ok(VirtualFiles {
        operation,
        descriptor: clipboard(CFSTR_FILEDESCRIPTORW)?,
        contents: clipboard(CFSTR_FILECONTENTS)?,
        preferred: clipboard(CFSTR_PREFERREDDROPEFFECT)?,
        private: Mutex::new(HashMap::new()),
    }
    .into())
}
impl IDataObject_Impl for VirtualFiles_Impl {
    fn GetData(&self, input: *const FORMATETC) -> Result<STGMEDIUM> {
        self.QueryGetData(input).ok()?;
        let input = unsafe { &*input };
        if input.cfFormat == self.descriptor {
            return global(&descriptors(&self.operation.names));
        }
        if input.cfFormat == self.preferred {
            return global(&DROPEFFECT_COPY.0.to_le_bytes());
        }
        if input.cfFormat == self.contents {
            self.operation.check_cancel()?;
            self.operation
                .content_requested
                .store(true, Ordering::SeqCst);
            // Returning a lazy stream must never wait for ADB.
            let stream: IStream = FileStream {
                cursor: Mutex::new(Cursor::default()),
                operation: Arc::clone(&self.operation),
                index: input.lindex as usize,
            }
            .into();
            return Ok(STGMEDIUM {
                tymed: TYMED_ISTREAM.0 as u32,
                u: STGMEDIUM_0 {
                    pstm: ManuallyDrop::new(Some(stream)),
                },
                ..Default::default()
            });
        }
        let private = self
            .private
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        global(
            private
                .get(&(input.cfFormat, input.lindex))
                .ok_or(windows::core::Error::from(DV_E_FORMATETC))?,
        )
    }
    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn QueryGetData(&self, input: *const FORMATETC) -> HRESULT {
        if input.is_null() {
            return E_POINTER;
        }
        let input = unsafe { &*input };
        if input.dwAspect != DVASPECT_CONTENT.0 {
            return DV_E_DVASPECT;
        }
        let medium = if input.cfFormat == self.descriptor || input.cfFormat == self.preferred {
            TYMED_HGLOBAL
        } else if input.cfFormat == self.contents {
            if input.lindex < 0 || input.lindex as usize >= self.operation.names.len() {
                return DV_E_LINDEX;
            }
            TYMED_ISTREAM
        } else if self
            .private
            .lock()
            .map(|data| data.contains_key(&(input.cfFormat, input.lindex)))
            .unwrap_or(false)
        {
            TYMED_HGLOBAL
        } else {
            return DV_E_FORMATETC;
        };
        if input.tymed & medium.0 as u32 == 0 {
            DV_E_TYMED
        } else {
            S_OK
        }
    }
    fn GetCanonicalFormatEtc(&self, input: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
        if input.is_null() || output.is_null() {
            return E_POINTER;
        }
        unsafe {
            *output = *input;
            (*output).ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }
    fn SetData(
        &self,
        input: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> Result<()> {
        if input.is_null() || medium.is_null() {
            return Err(E_POINTER.into());
        }
        let input = unsafe { &*input };
        let medium_ref = unsafe { &*medium };
        if medium_ref.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(DV_E_TYMED.into());
        }
        let memory = unsafe { medium_ref.u.hGlobal };
        let size = unsafe { GlobalSize(memory) };
        if size > 16 * 1024 * 1024 {
            return Err(E_OUTOFMEMORY.into());
        }
        let mut private = self
            .private
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        if private.len() >= 64 && !private.contains_key(&(input.cfFormat, input.lindex)) {
            return Err(E_OUTOFMEMORY.into());
        }
        let pointer = unsafe { GlobalLock(memory) };
        if pointer.is_null() {
            return Err(E_OUTOFMEMORY.into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
        unsafe {
            let _ = GlobalUnlock(memory);
        }
        private.insert((input.cfFormat, input.lindex), bytes);
        if release.as_bool() {
            let mut owned = unsafe { std::ptr::read(medium) };
            unsafe {
                ReleaseStgMedium(&mut owned);
            }
        }
        Ok(())
    }
    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let mut formats = vec![
            format(self.descriptor, TYMED_HGLOBAL, -1),
            format(self.contents, TYMED_ISTREAM, 0),
            format(self.preferred, TYMED_HGLOBAL, -1),
        ];
        for &(id, index) in self
            .private
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .keys()
        {
            formats.push(format(id, TYMED_HGLOBAL, index));
        }
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }
    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<'_, IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
    fn DUnadvise(&self, _: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}
impl IDataObjectAsyncCapability_Impl for VirtualFiles_Impl {
    fn SetAsyncMode(&self, value: BOOL) -> Result<()> {
        self.operation
            .async_mode
            .store(value.as_bool(), Ordering::SeqCst);
        Ok(())
    }
    fn GetAsyncMode(&self) -> Result<BOOL> {
        Ok(self.operation.async_mode.load(Ordering::SeqCst).into())
    }
    fn StartOperation(&self, _: Ref<'_, IBindCtx>) -> Result<()> {
        self.operation
            .completion
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .asynchronous = true;
        self.operation.changed.notify_all();
        Ok(())
    }
    fn InOperation(&self) -> Result<BOOL> {
        let completion = self
            .operation
            .completion
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        Ok((completion.asynchronous && completion.end.is_none()).into())
    }
    fn EndOperation(&self, result: HRESULT, _: Ref<'_, IBindCtx>, effect: u32) -> Result<()> {
        let outcome = if result == E_ABORT || result == HRESULT(0x800704c7u32 as i32) {
            Err(CANCELLED.into())
        } else if result.is_err() {
            Err(format!(
                "目标复制失败: {}",
                windows::core::Error::from(result)
            ))
        } else if effect & DROPEFFECT_COPY.0 == 0 {
            Err(CANCELLED.into())
        } else {
            Ok(())
        };
        self.operation
            .completion
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .end = Some(outcome);
        self.operation.changed.notify_all();
        Ok(())
    }
}

#[derive(Default)]
struct Cursor {
    file: Option<fs::File>,
    position: u64,
}
// Field order closes file handles before deleting cached payload directories.
#[implement(IStream, ISequentialStream)]
struct FileStream {
    cursor: Mutex<Cursor>,
    operation: Arc<DragOperation>,
    index: usize,
}
impl ISequentialStream_Impl for FileStream_Impl {
    fn Read(&self, buffer: *mut std::ffi::c_void, size: u32, read: *mut u32) -> HRESULT {
        if !read.is_null() {
            unsafe {
                *read = 0;
            }
        }
        if size == 0 {
            return S_OK;
        }
        if buffer.is_null() {
            return E_POINTER;
        }
        let result = (|| -> Result<u32> {
            let payload = self.operation.payload(self.index)?;
            let mut cursor = self
                .cursor
                .lock()
                .map_err(|_| windows::core::Error::from(STG_E_READFAULT))?;
            if cursor.file.is_none() {
                cursor.file = Some(fs::File::open(&payload.path).map_err(|e| {
                    self.operation.record_error(format!("读取缓存失败: {e}"));
                    windows::core::Error::from(STG_E_READFAULT)
                })?);
            }
            let position = cursor.position;
            let file = cursor.file.as_mut().unwrap();
            file.seek(SeekFrom::Start(position))
                .map_err(|_| windows::core::Error::from(STG_E_SEEKERROR))?;
            let count = file
                .read(unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), size as usize) })
                .map_err(|_| windows::core::Error::from(STG_E_READFAULT))?;
            cursor.position += count as u64;
            Ok(count as u32)
        })();
        match result {
            Ok(count) => {
                if !read.is_null() {
                    unsafe {
                        *read = count;
                    }
                }
                if count < size {
                    S_FALSE
                } else {
                    S_OK
                }
            }
            Err(error) => error.code(),
        }
    }
    fn Write(&self, _: *const std::ffi::c_void, _: u32, written: *mut u32) -> HRESULT {
        if !written.is_null() {
            unsafe {
                *written = 0;
            }
        }
        STG_E_ACCESSDENIED
    }
}
impl IStream_Impl for FileStream_Impl {
    fn Seek(&self, offset: i64, origin: STREAM_SEEK, output: *mut u64) -> Result<()> {
        let end = if origin == STREAM_SEEK_END {
            Some(
                fs::metadata(&self.operation.payload(self.index)?.path)
                    .map_err(|_| windows::core::Error::from(STG_E_READFAULT))?
                    .len(),
            )
        } else {
            None
        };
        let mut cursor = self
            .cursor
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?;
        let base = if origin == STREAM_SEEK_SET {
            0
        } else if origin == STREAM_SEEK_CUR {
            cursor.position
        } else if let Some(end) = end {
            end
        } else {
            return Err(STG_E_INVALIDFUNCTION.into());
        };
        let next = (base as i128) + offset as i128;
        if next < 0 || next > u64::MAX as i128 {
            return Err(STG_E_INVALIDFUNCTION.into());
        }
        cursor.position = next as u64;
        if !output.is_null() {
            unsafe {
                *output = cursor.position;
            }
        }
        Ok(())
    }
    fn SetSize(&self, _: u64) -> Result<()> {
        Err(STG_E_ACCESSDENIED.into())
    }
    fn CopyTo(
        &self,
        target: Ref<'_, IStream>,
        count: u64,
        read: *mut u64,
        written: *mut u64,
    ) -> Result<()> {
        let target = target
            .as_ref()
            .ok_or(windows::core::Error::from(E_POINTER))?;
        let mut buffer = [0u8; 65536];
        let mut consumed = 0;
        let mut produced = 0;
        unsafe {
            if !read.is_null() {
                *read = 0;
            }
            if !written.is_null() {
                *written = 0;
            }
        }
        while consumed < count {
            let mut chunk = 0;
            self.Read(
                buffer.as_mut_ptr().cast(),
                (count - consumed).min(buffer.len() as u64) as u32,
                &mut chunk,
            )
            .ok()?;
            if chunk == 0 {
                break;
            }
            consumed += chunk as u64;
            let mut output = 0;
            let result = unsafe { target.Write(buffer.as_ptr().cast(), chunk, Some(&mut output)) };
            produced += output as u64;
            unsafe {
                if !read.is_null() {
                    *read = consumed;
                }
                if !written.is_null() {
                    *written = produced;
                }
            }
            result.ok()?;
            if output != chunk {
                return Err(STG_E_MEDIUMFULL.into());
            }
        }
        Ok(())
    }
    fn Commit(&self, _: &STGC) -> Result<()> {
        Ok(())
    }
    fn Revert(&self) -> Result<()> {
        Err(STG_E_INVALIDFUNCTION.into())
    }
    fn LockRegion(&self, _: u64, _: u64, _: &LOCKTYPE) -> Result<()> {
        Err(STG_E_INVALIDFUNCTION.into())
    }
    fn UnlockRegion(&self, _: u64, _: u64, _: u32) -> Result<()> {
        Err(STG_E_INVALIDFUNCTION.into())
    }
    fn Stat(&self, output: *mut STATSTG, flags: &STATFLAG) -> Result<()> {
        if output.is_null() {
            return Err(E_POINTER.into());
        }
        let payload = self.operation.payload(self.index)?;
        let size = fs::metadata(&payload.path)
            .map_err(|_| windows::core::Error::from(STG_E_READFAULT))?
            .len();
        let mut stat = STATSTG {
            r#type: STGTY_STREAM.0 as u32,
            cbSize: size,
            grfMode: STGM_READ,
            ..Default::default()
        };
        if *flags != STATFLAG_NONAME {
            let name: Vec<u16> = self.operation.names[self.index]
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let pointer = unsafe { CoTaskMemAlloc(name.len() * 2) }.cast::<u16>();
            if pointer.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(name.as_ptr(), pointer, name.len());
            }
            stat.pwcsName = PWSTR(pointer);
        }
        unsafe {
            *output = stat;
        }
        Ok(())
    }
    fn Clone(&self) -> Result<IStream> {
        let position = self
            .cursor
            .lock()
            .map_err(|_| windows::core::Error::from(E_FAIL))?
            .position;
        Ok(FileStream {
            cursor: Mutex::new(Cursor {
                position,
                file: None,
            }),
            operation: Arc::clone(&self.operation),
            index: self.index,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use windows::core::Interface;

    fn apartment() -> OleApartment {
        unsafe {
            OleInitialize(None).unwrap();
        }
        OleApartment
    }
    fn fixture() -> (
        Arc<DragOperation>,
        Arc<AtomicUsize>,
        Arc<Mutex<Vec<PathBuf>>>,
    ) {
        let count = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&count);
        let directories = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&directories);
        let operation = DragOperation::new(
            vec!["中文.txt".into(), "second.bin".into()],
            Arc::new(AtomicBool::new(false)),
            Box::new(move |index| {
                calls.fetch_add(1, Ordering::SeqCst);
                let payload = Arc::new(DragPayload::create()?);
                fs::write(
                    &payload.path,
                    if index == 0 {
                        b"first\0binary".as_slice()
                    } else {
                        b"second\0bytes".as_slice()
                    },
                )
                .map_err(|e| e.to_string())?;
                captured.lock().unwrap().push(payload.directory.clone());
                Ok(payload)
            }),
        )
        .unwrap();
        (operation, count, directories)
    }
    fn contents(object: &IDataObject, index: i32) -> IStream {
        unsafe {
            let mut medium = object
                .GetData(&format(
                    clipboard(CFSTR_FILECONTENTS).unwrap(),
                    TYMED_ISTREAM,
                    index,
                ))
                .unwrap();
            let stream = medium.u.pstm.as_ref().unwrap().clone();
            ReleaseStgMedium(&mut medium);
            stream
        }
    }
    fn read(stream: &IStream) -> Vec<u8> {
        let mut buffer = [0u8; 128];
        let mut count = 0;
        unsafe {
            stream
                .Read(
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    Some(&mut count),
                )
                .ok()
                .unwrap();
        }
        buffer[..count as usize].to_vec()
    }
    #[test]
    fn windows_names_are_safe() {
        for name in [
            "",
            "..",
            "a/b",
            "a\\b",
            "a:b",
            "file.",
            "file ",
            "NUL.txt",
            "COM1.log",
            "COM¹.txt",
            "LPT9",
            "con .txt",
            "a\0b",
        ] {
            assert!(validate_windows_name(name).is_err(), "{name:?}");
        }
        assert!(validate_windows_name(&"字".repeat(260)).is_err());
        assert!(validate_windows_name("中文 空格 ' $文件.txt").is_ok());
        assert!(DragOperation::new(
            vec![],
            Arc::new(AtomicBool::new(false)),
            Box::new(|_| unreachable!())
        )
        .is_err());
    }
    #[test]
    fn descriptors_and_getdata_are_immediate_and_multiple_indexes_are_independent() {
        let _ole = apartment();
        let (operation, count, directories) = fixture();
        let object = virtual_files(Arc::clone(&operation)).unwrap();
        unsafe {
            let mut medium = object
                .GetData(&format(
                    clipboard(CFSTR_FILEDESCRIPTORW).unwrap(),
                    TYMED_HGLOBAL,
                    -1,
                ))
                .unwrap();
            let pointer = GlobalLock(medium.u.hGlobal);
            assert_eq!(std::ptr::read_unaligned(pointer.cast::<u32>()), 2);
            let second = std::ptr::read_unaligned(
                pointer
                    .cast::<u8>()
                    .add(4 + size_of::<FILEDESCRIPTORW>())
                    .cast::<FILEDESCRIPTORW>(),
            );
            let name = second.cFileName;
            assert_eq!(String::from_utf16_lossy(&name[..10]), "second.bin");
            let _ = GlobalUnlock(medium.u.hGlobal);
            ReleaseStgMedium(&mut medium);
            let mut invalid = format(clipboard(CFSTR_FILECONTENTS).unwrap(), TYMED_ISTREAM, -1);
            assert_eq!(object.QueryGetData(&invalid), DV_E_LINDEX);
            invalid.lindex = 2;
            assert_eq!(object.QueryGetData(&invalid), DV_E_LINDEX);
        }
        let first = contents(&object, 0);
        let second = contents(&object, 1);
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "neither descriptor nor GetData may wait for ADB"
        );
        assert_eq!(read(&first), b"first\0binary");
        assert_eq!(read(&second), b"second\0bytes");
        let again = contents(&object, 0);
        assert_eq!(read(&again), b"first\0binary");
        assert_eq!(count.load(Ordering::SeqCst), 2, "one download per file");
        let clone = unsafe { first.Clone().unwrap() };
        unsafe {
            clone.Seek(0, STREAM_SEEK_SET, None).unwrap();
        }
        assert_eq!(read(&clone), b"first\0binary");
        drop(first);
        drop(second);
        drop(again);
        drop(operation);
        drop(object);
        assert!(directories.lock().unwrap().iter().all(|path| path.exists()));
        drop(clone);
        assert!(directories
            .lock()
            .unwrap()
            .iter()
            .all(|path| !path.exists()));
    }
    #[test]
    fn streams_copy_seek_stat_and_cancel_without_empty_success() {
        let _ole = apartment();
        let (operation, _, _) = fixture();
        let object = virtual_files(Arc::clone(&operation)).unwrap();
        let stream = contents(&object, 0);
        unsafe {
            let target =
                StructuredStorage::CreateStreamOnHGlobal(HGLOBAL::default(), true).unwrap();
            let mut copied = 0;
            stream
                .CopyTo(&target, 128, None, Some(&mut copied))
                .unwrap();
            assert_eq!(copied, 12);
            target.Seek(0, STREAM_SEEK_SET, None).unwrap();
            assert_eq!(read(&target), b"first\0binary");
            assert!(stream.Seek(-1, STREAM_SEEK_SET, None).is_err());
            stream.Seek(-6, STREAM_SEEK_END, None).unwrap();
            assert_eq!(read(&stream), b"binary");
            let mut stat = STATSTG::default();
            stream.Stat(&mut stat, STATFLAG_DEFAULT).unwrap();
            assert_eq!(stat.cbSize, 12);
            assert!(!stat.pwcsName.is_null());
            CoTaskMemFree(Some(stat.pwcsName.0.cast()));
            operation.cancel.store(true, Ordering::SeqCst);
            let mut byte = 0u8;
            assert_eq!(
                stream.Read((&mut byte as *mut u8).cast(), 1, None),
                STG_E_REVERTED
            );
            assert!(stream.CopyTo(&target, 1, None, None).is_err());
        }
    }
    #[test]
    fn asynchronous_copy_waits_for_endoperation_after_mouse_drag_exits() {
        let _ole = apartment();
        let (operation, _, _) = fixture();
        let object = virtual_files(Arc::clone(&operation)).unwrap();
        let asynchronous: IDataObjectAsyncCapability = object.cast().unwrap();
        unsafe {
            assert!(asynchronous.GetAsyncMode().unwrap().as_bool());
            asynchronous.StartOperation(None::<&IBindCtx>).unwrap();
        }
        operation.end_drag(Ok(()));
        let waiting = Arc::clone(&operation);
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || sender.send(waiting.wait()).unwrap());
        assert!(
            receiver.recv_timeout(Duration::from_millis(150)).is_err(),
            "drop acceptance isn't copy completion"
        );
        assert_eq!(read(&contents(&object, 0)), b"first\0binary");
        unsafe {
            asynchronous
                .EndOperation(S_OK, None::<&IBindCtx>, DROPEFFECT_COPY.0)
                .unwrap();
        }
        assert!(receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .is_ok());
        worker.join().unwrap();
        unsafe {
            assert!(!asynchronous.InOperation().unwrap().as_bool());
        }
    }
    #[test]
    fn cancellation_and_failed_receiver_end_do_not_hang() {
        let _ole = apartment();
        let (operation, _, _) = fixture();
        operation.end_drag(Err("已取消传输：未投放".into()));
        assert!(operation.wait().unwrap_err().starts_with(CANCELLED));
        let (operation, _, _) = fixture();
        let object = virtual_files(Arc::clone(&operation)).unwrap();
        let asynchronous: IDataObjectAsyncCapability = object.cast().unwrap();
        unsafe {
            asynchronous.StartOperation(None::<&IBindCtx>).unwrap();
            asynchronous
                .EndOperation(E_ACCESSDENIED, None::<&IBindCtx>, DROPEFFECT_COPY.0)
                .unwrap();
        }
        operation.end_drag(Ok(()));
        assert!(operation.wait().unwrap_err().contains("目标复制失败"));
        let (operation, _, _) = fixture();
        operation.cancel.store(true, Ordering::SeqCst);
        assert!(operation.wait().unwrap_err().starts_with(CANCELLED));
        assert_eq!(continue_drag(false, false, false), DRAGDROP_S_DROP);
        assert_eq!(continue_drag(false, false, true), S_OK);
        assert_eq!(continue_drag(true, false, true), DRAGDROP_S_CANCEL);
        assert_eq!(continue_drag(false, true, true), DRAGDROP_S_CANCEL);
    }
    #[test]
    fn cached_failure_and_concurrent_content_requests_are_safe() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<DragOperation>();
        send_sync::<VirtualFiles>();
        send_sync::<FileStream>();
        let (operation, count, _) = fixture();
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let operation = Arc::clone(&operation);
                std::thread::spawn(move || operation.payload(0).unwrap().path.clone())
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().file_name().unwrap(), "payload");
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let operation = DragOperation::new(
            vec!["failure.txt".into()],
            Arc::new(AtomicBool::new(false)),
            Box::new(|_| Err("读取设备失败".into())),
        )
        .unwrap();
        assert!(operation.payload(0).is_err());
        assert!(operation.payload(0).is_err());
        operation.end_drag(Ok(()));
        assert_eq!(operation.wait().unwrap_err(), "读取设备失败");
    }
    #[test]
    fn agile_com_marshaling_reads_files_on_another_apartment_without_ui_pump() {
        let _ole = apartment();
        let (operation, _, _) = fixture();
        let object = virtual_files(operation).unwrap();
        assert!(object.cast::<IAgileObject>().is_ok());
        let transport = unsafe {
            Marshal::CoMarshalInterThreadInterfaceInStream(&IDataObject::IID, &object).unwrap()
        };
        let raw = transport.into_raw() as usize;
        let worker = std::thread::spawn(move || unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let transport = ManuallyDrop::new(IStream::from_raw(raw as *mut std::ffi::c_void));
            let object: IDataObject =
                StructuredStorage::CoGetInterfaceAndReleaseStream(&*transport).unwrap();
            let stream = contents(&object, 1);
            let bytes = read(&stream);
            drop(stream);
            drop(object);
            CoUninitialize();
            bytes
        });
        assert_eq!(worker.join().unwrap(), b"second\0bytes");
    }
}
