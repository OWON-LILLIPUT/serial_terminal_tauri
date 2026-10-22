//! Bounded, NUL-delimited directory manifests for virtual clipboard folders.
use crate::{device_files::shell_quote, file_drag::validate_windows_name};
use std::collections::HashSet;

pub const MAX_ITEMS: usize = crate::file_drag::MAX_VIRTUAL_ITEMS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub remote: String,
    pub name: String,
    pub is_dir: bool,
}

pub fn script(root: &str) -> String {
    let root = shell_quote(root);
    format!(
        r#"[ -d {root} ] && [ ! -L {root} ] || {{ echo '只能复制普通目录' >&2; exit 1; }}
cd -- {root} || exit 1
[ -r . ] && [ -x . ] || {{ echo 'Permission denied' >&2; exit 1; }}
printf 'STTREE1\000'
# Do not follow symlinks. Any link/special entry will fail manifest validation.
find . -exec sh -c '
  printf "%s\000" "$#" "$@"
  stat -c "%s %Y %a %f" -- "$@" || exit 1
  printf "\000"
' sh {{}} + || exit 1
printf 'END\000'"#
    )
}

pub fn parse(root: &str, root_name: &str, output: &[u8]) -> Result<Vec<Entry>, String> {
    let root = crate::device_files::normalize_path(root)?;
    if output.len() > 16 * 1024 * 1024 || !output.ends_with(b"\0") {
        return Err("目录清单过大或不完整".into());
    }
    validate_windows_name(root_name)?;
    let mut fields = output.split(|byte| *byte == 0);
    if fields.next() != Some(b"STTREE1".as_slice()) {
        return Err("设备未返回有效的递归目录清单".into());
    }
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let token = fields.next().ok_or("目录清单不完整")?;
        if token == b"END" {
            if fields.any(|field| !field.is_empty()) {
                return Err("目录清单包含多余数据".into());
            }
            break;
        }
        let count = std::str::from_utf8(token)
            .map_err(|_| "目录批次无效")?
            .parse::<usize>()
            .map_err(|_| "目录批次无效")?;
        if count == 0 || count > MAX_ITEMS || entries.len() + count > MAX_ITEMS {
            return Err("复制目录最多包含 10000 个项目，请选择子目录或使用下载按钮".into());
        }
        let names: Vec<&str> = (0..count)
            .map(|_| {
                std::str::from_utf8(fields.next().ok_or("目录文件名不完整")?)
                    .map_err(|_| "设备文件名不是 UTF-8")
            })
            .collect::<Result<_, _>>()?;
        let metadata = std::str::from_utf8(fields.next().ok_or("目录元数据不完整")?)
            .map_err(|_| "目录元数据无效")?;
        let rows: Vec<_> = metadata.lines().collect();
        if rows.len() != count {
            return Err("目录元数据数量不匹配".into());
        }
        for (name, row) in names.into_iter().zip(rows) {
            let relative = if name == "." {
                ""
            } else {
                name.strip_prefix("./").ok_or("目录路径无效")?
            };
            if name != "." && relative.is_empty() {
                return Err("目录路径无效".into());
            }
            for segment in relative.split('/').filter(|_| !relative.is_empty()) {
                validate_windows_name(segment)?;
            }
            let values: Vec<_> = row.split_whitespace().collect();
            if values.len() != 4
                || values[0].parse::<u64>().is_err()
                || values[1].parse::<i64>().is_err()
                || u32::from_str_radix(values[2], 8).is_err()
            {
                return Err("目录元数据无效".into());
            }
            let mode = u32::from_str_radix(values[3], 16).map_err(|_| "目录项目类型无效")?;
            let is_dir = match mode & 0xf000 {
                0x4000 => true,
                0x8000 => false,
                _ => {
                    return Err(
                        "复制的目录包含符号链接或特殊文件，暂不支持；请单独选择普通文件/目录"
                            .into(),
                    )
                }
            };
            if relative.is_empty() && !is_dir {
                return Err("复制的目录类型已变化，请重新复制".into());
            }
            let name = if relative.is_empty() {
                root_name.to_owned()
            } else {
                format!("{root_name}\\{}", relative.replace('/', "\\"))
            };
            if name.encode_utf16().count() >= 260 || !seen.insert(name.to_lowercase()) {
                return Err("目录包含过长路径或 Windows 同名冲突，请先重命名".into());
            }
            entries.push(Entry {
                remote: if relative.is_empty() {
                    root.clone()
                } else {
                    crate::device_files::normalize_path(&format!("{root}/{relative}"))?
                },
                name,
                is_dir,
            });
        }
    }
    if !entries
        .iter()
        .any(|entry| entry.name == root_name && entry.is_dir)
    {
        return Err("目录清单缺少根目录".into());
    }
    entries
        .sort_by_cached_key(|entry| (entry.name.matches('\\').count(), entry.name.to_lowercase()));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch(rows: &[(&str, &str)]) -> Vec<u8> {
        let mut out = format!("STTREE1\0{}\0", rows.len()).into_bytes();
        for (name, _) in rows {
            out.extend_from_slice(name.as_bytes());
            out.push(0);
        }
        for (_, mode) in rows {
            out.extend_from_slice(format!("0 1700000000 755 {mode}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\0END\0");
        out
    }
    #[test]
    fn nested_manifest_preserves_empty_directories_and_relative_names() {
        let entries = parse(
            "/sdcard/project",
            "project",
            &batch(&[
                (".", "41ed"),
                ("./empty", "41ed"),
                ("./子目录", "41ed"),
                ("./子目录/file.txt", "81a4"),
            ]),
        )
        .unwrap();
        assert_eq!(entries.len(), 4);
        assert!(
            entries[0].is_dir
                && entries
                    .iter()
                    .any(|entry| entry.name == "project\\empty" && entry.is_dir)
        );
        assert!(entries
            .iter()
            .any(|entry| entry.name == "project\\子目录\\file.txt"
                && entry.remote == "/sdcard/project/子目录/file.txt"
                && !entry.is_dir));
        assert!(!script("/sdcard/'folder").contains("find -L"));
        assert!(script("/sdcard/'folder").contains("'\\''"));
    }
    #[test]
    fn links_special_files_traversal_and_windows_conflicts_are_rejected() {
        for rows in [
            vec![(".", "41ed"), ("./link", "a1ff")],
            vec![(".", "41ed"), ("./pipe", "11a4")],
            vec![(".", "41ed"), ("./../escape", "81a4")],
            vec![(".", "41ed"), ("./A", "81a4"), ("./a", "81a4")],
            vec![(".", "41ed"), ("./NUL", "81a4")],
            vec![("./file", "81a4")],
            vec![(".", "81a4")],
        ] {
            assert!(parse("/sdcard/folder", "folder", &batch(&rows)).is_err());
        }
    }
    #[test]
    fn incomplete_metadata_and_overlong_paths_are_not_partial_successes() {
        let valid = batch(&[(".", "41ed")]);
        assert!(parse("/sdcard/folder", "folder", &valid[..valid.len() - 4]).is_err());
        let mut extra = valid.clone();
        extra.extend_from_slice(b"junk\0");
        assert!(parse("/sdcard/folder", "folder", &extra).is_err());
        let name = format!("./{}", "x".repeat(255));
        assert!(parse(
            "/sdcard/folder",
            "folder",
            &batch(&[(".", "41ed"), (&name, "81a4")])
        )
        .is_err());
        assert!(parse("/sdcard/folder", "folder", b"STTREE1\099999\0").is_err());
    }
}
