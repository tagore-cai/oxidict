//! 系统shell辅助：用平台默认方式打开路径/目录。

/// 用系统默认方式打开一个路径（目录用文件管理器，文件用关联应用）。
pub fn open_path(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(|_| ())
}
