//! 备份与恢复。
//!
//! 与原 `backup.rs` 逻辑一致：把 config.json + history.db + plugins/ 打包为
//! zip，支持本地导出/导入与 WebDAV 云端同步。
//! WebDAV 用 raw HTTP 实现（PUT/GET/MKCOL/DELETE），避免 reqwest_dav 依赖。

use crate::config::config;
use crate::{Error, Result};
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;

/// 配置目录下的关键文件打包为 zip 字节。
pub fn pack() -> Result<Vec<u8>> {
    let dir = config().app_dir();
    let config_path = dir.join("config.json");
    let database_path = dir.join("history.db");
    let plugin_path = dir.join("plugins");

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file("config.json", options)
            .map_err(|e| Error::Plugin(format!("zip 写 config.json 失败: {e}")))?;
        zip.write_all(&std::fs::read(&config_path)?)?;

        if database_path.exists() {
            zip.start_file("history.db", options)
                .map_err(|e| Error::Plugin(format!("zip 写 history.db 失败: {e}")))?;
            zip.write_all(&std::fs::read(&database_path)?)?;
        }

        if plugin_path.exists() {
            for entry in WalkDir::new(&plugin_path) {
                let entry = entry.map_err(|e| Error::Plugin(e.to_string()))?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let file_name = match path.strip_prefix(&dir).ok().and_then(|p| p.to_str()) {
                    Some(v) => v,
                    None => return Err(Error::Plugin("strip_prefix 或路径非 UTF-8".into())),
                };
                zip.start_file(file_name, options)
                    .map_err(|e| Error::Plugin(format!("zip 写 {file_name} 失败: {e}")))?;
                zip.write_all(&std::fs::read(path)?)?;
            }
        }
        zip.finish()
            .map_err(|e| Error::Plugin(format!("zip finish: {e}")))?;
    }
    Ok(buf.into_inner())
}

/// 解压 zip 到配置目录（覆盖现有文件）。
///
/// 覆盖前先把会被替换的用户数据另存为 `*.pre-restore.bak`：恢复是破坏性的
/// 不可撤销操作（整份替换 `config.json` 与 `history.db`），一旦用户选错备份，
/// 这是唯一的回退路径。
///
/// 路径安全由 `zip` 的 `ZipArchive::extract` 负责——它内部用 `enclosed_name()`
/// 拒绝 `..` 与绝对路径，因此无需在此重复校验。
pub fn unpack(data: &[u8]) -> Result<()> {
    let dir = config().app_dir();
    snapshot_before_restore(&dir);
    let mut archive = ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|e| Error::Plugin(format!("打开备份包失败: {e}")))?;
    archive
        .extract(&dir)
        .map_err(|e| Error::Plugin(format!("解压备份失败: {e}")))?;
    Ok(())
}

/// 把即将被恢复覆盖的文件另存为 `.pre-restore.bak`。
///
/// 尽力而为：单个文件快照失败只记日志，不阻断恢复（否则用户可能被一个只读的
/// 历史库卡住，连备份都恢复不了）。
fn snapshot_before_restore(dir: &Path) {
    for name in ["config.json", "history.db"] {
        let src = dir.join(name);
        if !src.exists() {
            continue;
        }
        let dst = dir.join(format!("{name}.pre-restore.bak"));
        if let Err(e) = std::fs::copy(&src, &dst) {
            log::error!("恢复前快照 {name} 失败（继续恢复）: {e}");
        }
    }
}

/// 导出备份到本地文件。
pub fn export_to_file(path: &str) -> Result<()> {
    let data = pack()?;
    std::fs::write(path, data)?;
    Ok(())
}

/// 从本地文件导入备份。
pub fn import_from_file(path: &str) -> Result<()> {
    let data = std::fs::read(path)?;
    unpack(&data)
}
