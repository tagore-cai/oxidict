//! 自动启动登录（macOS LaunchAgent plist 方案）。
//!
//! 原版用 `tauri-plugin-autostart`，这里直接操作 LaunchAgent plist，
//! 零额外依赖。Windows 用注册表 Run 键，Linux 用 .desktop 文件。

use oxidict_core::{Error, Result};
use std::path::PathBuf;

/// 当前可执行文件路径。
fn exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// LaunchAgent plist 路径。
fn plist_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library")
            .join("LaunchAgents")
            .join("net.oxidict.app.plist")
    })
}

/// 启用开机自启。
pub fn enable() -> Result<()> {
    let exe = exe_path().ok_or_else(|| Error::Config("无法定位可执行文件".into()))?;
    let plist = plist_path().ok_or_else(|| Error::Config("无法定位 LaunchAgents 目录".into()))?;

    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let label = "net.oxidict.app";
    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#,
        exe.display()
    );

    std::fs::write(&plist, plist_content)?;
    Ok(())
}

/// 禁用开机自启。
pub fn disable() -> Result<()> {
    let plist = plist_path().ok_or_else(|| Error::Config("无法定位 LaunchAgents 目录".into()))?;
    if plist.exists() {
        std::fs::remove_file(&plist)?;
    }
    Ok(())
}

/// 查询是否已启用。
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}
