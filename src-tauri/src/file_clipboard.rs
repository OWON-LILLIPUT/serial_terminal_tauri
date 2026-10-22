//! Native file clipboard only. Text is never interpreted as a file list.
use crate::file_drag::{virtual_files, DragOperation};
use std::{cell::RefCell, ffi::c_void, mem::size_of, sync::Arc};
use windows::{
    core::{Interface, HRESULT},
    Win32::{
        Foundation::{S_FALSE, S_OK},
        System::{
            Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize, ReleaseStgMedium,
                CF_HDROP,
            },
        },
        UI::Shell::DROPFILES,
    },
};

// The generated Result<()> wrapper loses S_FALSE. Only S_OK means ownership;
// treating S_FALSE as success would erase somebody else's clipboard at exit.
#[link(name = "ole32")]
extern "system" {
    #[link_name = "OleIsCurrentClipboard"]
    fn ole_is_current_clipboard(object: *mut c_void) -> HRESULT;
}

struct Apartment;
impl Apartment {
    fn enter() -> Result<Self, String> {
        unsafe { OleInitialize(None) }.map_err(|error| format!("初始化文件剪贴板失败: {error}"))?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}

struct ClipboardOwner {
    object: IDataObject,
    // Keep the source apartment alive for delayed rendering, not just the IPC.
    _apartment: Apartment,
}
impl ClipboardOwner {
    fn is_current(&self) -> bool {
        unsafe { ole_is_current_clipboard(self.object.as_raw()) == S_OK }
    }
}
impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        if self.is_current() {
            // Never flush here: it could download all remote files on exit.
            let _ = unsafe { OleSetClipboard(None::<&IDataObject>) };
        }
    }
}
thread_local! { static OWNER: RefCell<Option<ClipboardOwner>> = const { RefCell::new(None) }; }

/// Must run on the window's UI/STA thread. No file contents are fetched here.
pub fn publish(operation: Arc<DragOperation>) -> Result<(), String> {
    let apartment = Apartment::enter()?;
    let object = virtual_files(operation).map_err(|error| error.to_string())?;
    unsafe { OleSetClipboard(&object) }.map_err(|error| format!("写入文件剪贴板失败: {error}"))?;
    let previous = OWNER.with(|slot| {
        slot.replace(Some(ClipboardOwner {
            object,
            _apartment: apartment,
        }))
    });
    drop(previous); // Drop outside RefCell borrow in case OLE reenters.
    Ok(())
}

pub fn prune() {
    let object = OWNER.with(|slot| slot.borrow().as_ref().map(|owner| owner.object.clone()));
    let stale = object
        .as_ref()
        .is_some_and(|object| unsafe { ole_is_current_clipboard(object.as_raw()) == S_FALSE });
    drop(object);
    if stale {
        shutdown();
    }
}

/// Only clears our own virtual files, preserving clipboard data copied elsewhere.
pub fn shutdown() {
    let owner = OWNER.with(|slot| slot.take());
    drop(owner);
}

/// Worker STA: a slow third-party clipboard source must not block the UI.
pub fn read_paths() -> Result<Vec<String>, String> {
    let _apartment = Apartment::enter()?;
    let object =
        unsafe { OleGetClipboard() }.map_err(|error| format!("读取文件剪贴板失败: {error}"))?;
    paths_from_object(&object)
}

pub(crate) fn paths_from_object(object: &IDataObject) -> Result<Vec<String>, String> {
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    };
    if unsafe { object.QueryGetData(&format) }.is_err() {
        return Ok(Vec::new());
    }
    let mut medium = unsafe { object.GetData(&format) }
        .map_err(|error| format!("读取剪贴板文件列表失败: {error}"))?;
    let result = (|| {
        if medium.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err("剪贴板文件列表格式无效".into());
        }
        let memory = unsafe { medium.u.hGlobal };
        let size = unsafe { GlobalSize(memory) };
        if !(size_of::<DROPFILES>()..=16 * 1024 * 1024).contains(&size) {
            return Err("剪贴板文件列表大小无效".into());
        }
        let pointer = unsafe { GlobalLock(memory) };
        if pointer.is_null() {
            return Err("无法读取剪贴板文件列表".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
        let _ = unsafe { GlobalUnlock(memory) };
        parse_hdrop(&bytes)
    })();
    unsafe {
        ReleaseStgMedium(&mut medium);
    }
    result
}

fn parse_hdrop(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() < size_of::<DROPFILES>() {
        return Err("剪贴板文件列表不完整".into());
    }
    let header = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<DROPFILES>()) };
    let offset = header.pFiles as usize;
    if !header.fWide.as_bool() {
        return Err("请在 Windows 资源管理器中重新复制文件后粘贴（需要 Unicode 文件列表）".into());
    }
    if offset < size_of::<DROPFILES>() || offset >= bytes.len() || offset % 2 != 0 {
        return Err("剪贴板文件列表偏移无效".into());
    }
    let mut paths = Vec::new();
    let mut name = Vec::new();
    for unit in bytes[offset..].chunks_exact(2) {
        let unit = u16::from_le_bytes([unit[0], unit[1]]);
        if unit == 0 {
            if name.is_empty() {
                if paths.is_empty() {
                    return Err("剪贴板文件列表为空".into());
                }
                return Ok(paths);
            }
            paths.push(String::from_utf16(&name).map_err(|_| "剪贴板文件路径编码无效")?);
            name.clear();
            if paths.len() > 256 {
                return Err("一次请粘贴 1–256 个文件或文件夹".into());
            }
        } else {
            name.push(unit);
            if name.len() > 32767 {
                return Err("剪贴板文件路径过长".into());
            }
        }
    }
    Err("剪贴板文件列表未正确结束".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dropfiles(paths: &[&str]) -> Vec<u8> {
        let mut bytes = vec![0; size_of::<DROPFILES>()];
        let offset = bytes.len() as u32;
        bytes[..4].copy_from_slice(&offset.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for path in paths {
            for unit in path.encode_utf16().chain(Some(0)) {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }
    #[test]
    fn unicode_file_lists_preserve_spaces_and_multiple_paths() {
        assert_eq!(
            parse_hdrop(&dropfiles(&[r"C:\mock\中文 空格.txt", r"D:\test.bin"])).unwrap(),
            [r"C:\mock\中文 空格.txt", r"D:\test.bin"]
        );
    }
    #[test]
    fn malformed_and_oversized_file_lists_are_rejected() {
        assert!(parse_hdrop(&[]).is_err());
        assert!(parse_hdrop(&dropfiles(&[])).is_err());
        let mut bytes = dropfiles(&[r"C:\file.txt"]);
        bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_hdrop(&bytes).is_err());
        let mut bytes = dropfiles(&[r"C:\file.txt"]);
        bytes.truncate(bytes.len() - 2);
        assert!(parse_hdrop(&bytes).is_err());
        assert!(parse_hdrop(&dropfiles(&vec![r"C:\f"; 257])).is_err());
        let mut bytes = dropfiles(&[r"C:\file.txt"]);
        bytes[16..20].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_hdrop(&bytes).is_err());
        let mut bytes = dropfiles(&[r"C:\file.txt"]);
        bytes[20..22].copy_from_slice(&0xd800u16.to_le_bytes());
        assert!(parse_hdrop(&bytes).is_err());
    }
}
