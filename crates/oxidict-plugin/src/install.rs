//! 安装 `.potext` 插件。
//!
//! 与老版本一致：`.potext` 就是一个 zip，含 `info.json` 与 `main.js`。
//! 装完不需要任何改写——运行时直接 eval 原始 `main.js`。

use crate::{Error, Result};
use oxidict_core::ServiceKind;
use oxidict_core::config::ConfigStore;
use std::fs::File;
use std::path::Path;

/// 安装一个 `.potext`，返回插件 id。
///
/// 校验规则沿用上游：包名必须以 `plugin` 开头，包内必须有 `main.js`。
/// 校验对象统一为**最终落盘的目录名**（即 `info.id`，缺省时取去掉 `.potext`
/// 后缀的文件名）。早期实现只校验文件名、而加载侧按目录名过滤，导致 `info.id`
/// 不以 `plugin` 开头的包能装成功却永远不会被加载。
pub fn install(potext: &Path, kind: ServiceKind, store: &ConfigStore) -> Result<String> {
    let file_name = potext
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Plugin("插件文件名不合法".into()))?;

    let staging = std::env::temp_dir().join(format!("oxidict-plugin-{}", file_name));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    let file = File::open(potext)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Plugin(format!("不是合法的插件包: {}", e)))?;
    archive
        .extract(&staging)
        .map_err(|e| Error::Plugin(format!("解包失败: {}", e)))?;

    if !staging.join("main.js").exists() {
        return Err(Error::Plugin("插件包内缺少 main.js".into()));
    }

    let info: serde_json::Value = match std::fs::read_to_string(staging.join("info.json")) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    };

    let plugin_id = info
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| file_name.trim_end_matches(".potext").to_string());

    // plugin_id 直接参与目标路径拼接，必须拒绝路径分隔符与 `..`：否则一个
    // 恶意 info.json（`"id": "../../../.ssh"`）就能把文件写到插件目录之外。
    if plugin_id.is_empty()
        || plugin_id.contains('/')
        || plugin_id.contains('\\')
        || plugin_id.contains("..")
    {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(Error::Plugin(format!("插件 id 不合法: {plugin_id}")));
    }

    if !plugin_id.starts_with("plugin") {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(Error::Plugin(format!(
            "插件 id 必须以 plugin 开头，收到的是 {plugin_id}"
        )));
    }

    let target = store.plugin_dir().join(kind.dir_name()).join(&plugin_id);
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::create_dir_all(&target)?;
    copy_dir(&staging, &target)?;
    let _ = std::fs::remove_dir_all(&staging);

    Ok(plugin_id)
}

pub fn uninstall(plugin_id: &str, kind: ServiceKind, store: &ConfigStore) -> Result<()> {
    let dir = store.plugin_dir().join(kind.dir_name()).join(plugin_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    crate::runtime::unload(plugin_id);
    Ok(())
}

/// 列出某类型下已安装的插件 id。
///
/// 目录名即插件 id（安装时已校验以 `plugin` 开头），这里按目录列举，
/// 与 [`install`] 的校验对象保持一致。
pub fn installed(kind: ServiceKind, store: &ConfigStore) -> Vec<String> {
    let dir = store.plugin_dir().join(kind.dir_name());
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir()
                && let Some(name) = entry.file_name().to_str()
                && name.starts_with("plugin")
            {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.map_err(|e| Error::Plugin(e.to_string()))?;
        let rel = entry.path().strip_prefix(from).unwrap_or(Path::new(""));
        let dest = to.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}
