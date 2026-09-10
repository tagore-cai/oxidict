//! 备份与恢复。
//!
//! 与原 `backup.rs` 逻辑一致：把 config.json + history.db + plugins/ 打包为
//! zip，支持本地导出/导入与 WebDAV 云端同步。
//! WebDAV 用 raw reqwest 实现（PUT/GET/MKCOL/DELETE），避免 reqwest_dav 依赖。

use crate::config::config;
use crate::{Error, Result};
use std::io::Write;
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
pub fn unpack(data: &[u8]) -> Result<()> {
    let dir = config().app_dir();
    let mut archive = ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|e| Error::Plugin(format!("打开备份包失败: {e}")))?;
    archive
        .extract(&dir)
        .map_err(|e| Error::Plugin(format!("解压备份失败: {e}")))?;
    Ok(())
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
