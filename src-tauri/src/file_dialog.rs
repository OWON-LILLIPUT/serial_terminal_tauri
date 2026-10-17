use std::path::PathBuf;

#[cfg(windows)]
pub fn choose(owner: isize, mode: &str, name: &str) -> Result<Option<PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_DONTADDTORECENT,
        OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    if matches!(mode, "upload-directory" | "download-directory") {
        return choose_folder(owner);
    }
    if !matches!(mode, "upload-file" | "download-file") {
        return Err("不支持的文件选择模式".into());
    }
    let mut buffer = vec![0u16; 32768];
    if mode == "download-file" {
        // Device filenames are not necessarily legal Windows filenames.
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_control() || "<>:\"/\\|?*".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        for (slot, unit) in buffer.iter_mut().zip(
            safe_name
                .trim_end_matches(['.', ' '])
                .encode_utf16()
                .take(255),
        ) {
            *slot = unit;
        }
    }
    let title: Vec<u16> = if mode == "upload-file" {
        "选择上传文件"
    } else {
        "保存设备文件"
    }
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let filter: Vec<u16> = "所有文件\0*.*\0\0".encode_utf16().collect();
    let mut options: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    options.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    options.hwndOwner = owner;
    options.lpstrFile = buffer.as_mut_ptr();
    options.nMaxFile = buffer.len() as u32;
    options.lpstrTitle = title.as_ptr();
    options.lpstrFilter = filter.as_ptr();
    options.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_DONTADDTORECENT;
    if mode == "upload-file" {
        options.Flags |= OFN_FILEMUSTEXIST;
    }
    let accepted = unsafe {
        if mode == "upload-file" {
            GetOpenFileNameW(&mut options)
        } else {
            GetSaveFileNameW(&mut options)
        }
    };
    if accepted == 0 {
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 {
            Ok(None)
        } else {
            Err(format!("文件选择失败: {error}"))
        };
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..end],
    ))))
}

#[cfg(windows)]
fn choose_folder(owner: isize) -> Result<Option<PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::Shell::{BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS},
    };
    // The cached windows-sys 0.52 manifest omits Shell_Common, so declare only
    // these two stable shell32 ABI functions rather than replacing the dependency.
    #[repr(C)]
    struct BrowseInfo {
        owner: isize,
        root: *const std::ffi::c_void,
        display_name: *mut u16,
        title: *const u16,
        flags: u32,
        callback: *const std::ffi::c_void,
        parameter: isize,
        image: i32,
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHBrowseForFolderW(info: *const BrowseInfo) -> *mut std::ffi::c_void;
        fn SHGetPathFromIDListW(item: *const std::ffi::c_void, path: *mut u16) -> i32;
    }
    let initialized = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    if initialized < 0 {
        return Err("无法初始化文件夹选择器".into());
    }
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _com = ComGuard;
    let title: Vec<u16> = "选择本地文件夹".encode_utf16().chain(Some(0)).collect();
    let mut display = [0u16; 260];
    let info = BrowseInfo {
        owner,
        root: std::ptr::null(),
        display_name: display.as_mut_ptr(),
        title: title.as_ptr(),
        flags: BIF_NEWDIALOGSTYLE | BIF_RETURNONLYFSDIRS,
        callback: std::ptr::null(),
        parameter: 0,
        image: 0,
    };
    let item = unsafe { SHBrowseForFolderW(&info) };
    if item.is_null() {
        return Ok(None);
    }
    let mut buffer = [0u16; 260];
    let accepted = unsafe { SHGetPathFromIDListW(item, buffer.as_mut_ptr()) };
    unsafe {
        CoTaskMemFree(item as *const _);
    }
    if accepted == 0 {
        return Err("无法读取所选文件夹路径".into());
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..end],
    ))))
}

#[cfg(not(windows))]
pub fn choose(_owner: isize, _mode: &str, _name: &str) -> Result<Option<PathBuf>, String> {
    Err("当前文件选择器仅支持 Windows".into())
}
