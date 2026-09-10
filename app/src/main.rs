//! oxidict：gpui-kit + Rust 重写版的可执行入口。
//!
//! 启动顺序：配置装载 -> 内置服务注册 -> 外部调用 HTTP 服务（独立线程）->
//! 剪切板监听与全局快捷键 -> GPUI 主窗口（阻塞在事件循环上）。

use oxidict_core::config::ConfigStore;
use oxidict_core::{Result, TranslateRequest, TranslateResult};
use std::sync::Arc;

/// 用启用列表里的第一个实例翻译一段文本，并按需写入历史库。
///
/// CLI 与本地 HTTP 服务共用这一个入口：服务选择交给
/// [`oxidict_services::translate_first_enabled_blocking`]，本函数只负责
/// 组装请求与写历史。
fn translate_once(req: TranslateRequest) -> Result<(String, TranslateResult)> {
    let store = oxidict_core::config::config();
    let from = req.from;
    let to = req.to;
    let text = req.text.clone();
    let (instance, result) = oxidict_services::translate_first_enabled_blocking(req)?;

    // 写入历史库（history_disable 为 true 时跳过）。
    if !store.get_or(oxidict_core::config::keys::HISTORY_DISABLE, false) {
        let _ = oxidict_core::history::History::global().add(
            &text,
            from.code(),
            to.code(),
            &instance,
            &result.as_text(),
        );
    }
    Ok((instance, result))
}

fn run_cli_translate(text: &str) {
    ConfigStore::init().expect("初始化配置失败");
    oxidict_services::init_builtin_services();
    oxidict_core::history::History::init_default().expect("历史库初始化失败");

    let store = oxidict_core::config::config();
    let req = TranslateRequest::new(text, store.source_language(), store.target_language());
    match translate_once(req) {
        Ok((instance, result)) => {
            println!("[{instance}] {}", result.as_text());
        }
        Err(e) => {
            eprintln!("翻译失败: {e}");
            std::process::exit(1);
        }
    }
}

/// 日志初始化：RUST_LOG 过滤（缺省 warn），stdout + 文件双输出。
/// 文件落在 `<config_dir>/<APP_ID>/logs/oxidict.log`，托盘「查看日志」即打开该目录。
fn init_logger() {
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Warn);
    let log_dir = oxidict_core::config::default_app_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("logs");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("日志目录创建失败: {e}");
    }
    let file = fern::log_file(log_dir.join("oxidict.log")).ok();
    let dispatch = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout());
    let dispatch = match file {
        Some(f) => dispatch.chain(f),
        None => dispatch,
    };
    if let Err(e) = dispatch.apply() {
        eprintln!("日志初始化失败: {e}");
    }
}

/// 判断指定 PID 的进程是否还活着（单实例锁的脏锁检测用）。
///
/// `kill(pid, 0)` 是 POSIX 约定的存在性探测：signal 为 0 时不投递任何信号，
/// 只做存在性与权限检查。
///
/// 注意 `EPERM`（进程存在但无权限探测）也会返回 -1，此时会误判为「进程已死」
/// 并清理锁文件。沙箱/容器场景下若发生，表现为单实例保护偶发失效，可接受。
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // `pid_t` 是有符号 32 位；PID 超出 i32 直接判定为不存在，避免 `as` 截断成负数。
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: 调用 libc::kill 且 signal = 0，不投递信号、不触碰任何用户空间
    // 内存，因此没有别名、初始化、对齐或生命周期上的前置条件；返回值仅与
    // 进程存在性/权限相关，不存在 UB 可能。
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Windows 上 `libc` 不提供 `kill`，这里保守判定为「存活」。
///
/// 后果：进程崩溃残留的锁文件不会被自动清理，需用户手工删除
/// `<runtime_dir>/oxidict-app.lock`。要做到与 Unix 对等，需在 `app` 引入
/// `windows` crate，用 `OpenProcess` + `GetExitCodeProcess` 判定
/// `STILL_ACTIVE`（本仓库的 windows 依赖目前只在 oxidict-platform 里）。
#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    true
}

fn main() {
    init_logger();

    // 0. CLI 模式：`oxidict --translate "文本"` 直接翻译并输出，用于端到端验证。
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--translate" {
        run_cli_translate(&args[2]);
        return;
    }

    // 0.5 单实例保护：锁文件 + PID 检测（脏锁自动清理）。
    let lock_path = oxidict_core::config::runtime_lock_path();
    let lock_created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path);
    match lock_created {
        Ok(_) => {
            // 成功创建，写入 PID 供后续脏锁检测。
            let _ = std::fs::write(&lock_path, std::process::id().to_string());
        }
        Err(_) => {
            // 文件已存在，检查是否为脏锁（PID 对应进程已死）。
            let stale = std::fs::read_to_string(&lock_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .map(|pid| pid != std::process::id() && !process_alive(pid))
                .unwrap_or(true); // 读不到或解析失败视为脏锁

            if stale {
                let _ = std::fs::remove_file(&lock_path);
                if std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)
                    .is_ok()
                {
                    let _ = std::fs::write(&lock_path, std::process::id().to_string());
                } else {
                    eprintln!("无法创建锁文件，退出。");
                    std::process::exit(0);
                }
            } else {
                eprintln!("检测到另一个 oxidict 实例正在运行，退出。");
                std::process::exit(0);
            }
        }
    }

    // 1. 配置：沿用老版本 config.json，老用户配置直接迁移。
    let store = ConfigStore::init().expect("初始化配置失败");

    // 1.2 语言：按 app_language 初始化 Fluent 本地化。
    oxidict_core::i18n::init_from_config();
    log::info!("配置目录: {}", store.app_dir().display());

    // 1.3 Dock 图标：按 hide_dock_icon 应用（macOS Accessory 策略；主线程）。
    oxidict_platform::dock::apply_from_config();

    // 1.5 历史库：与老版本同一个 history.db。
    oxidict_core::history::History::init_default().expect("历史库初始化失败");

    // 2. 注册内置服务（翻译 30 / OCR 15 / TTS 3 / 生词本 2）。
    oxidict_services::init_builtin_services();

    // 2.5 加载已安装的 .potext 插件（headless QuickJS VM，不依赖窗口）。
    let plugin_loaded = oxidict_plugin::load_installed(&store);
    if !plugin_loaded.is_empty() {
        log::info!("插件加载成功: {:?}", plugin_loaded);
    }

    let services = oxidict_services::services();
    log::info!(
        "内置服务就绪: translate {} / recognize {} / tts {} / collection {}",
        services.translators().len(),
        services.recognizers().len(),
        services.tts_list().len(),
        services.collectors().len()
    );

    // 3. 外部调用 HTTP 服务（tiny_http 阻塞线程，翻译回调走全局 tokio runtime）。
    //    回调与 CLI 走同一个 [`translate_once`]：服务选择、历史写入只有一份实现。
    let ctx = oxidict_server::ServerContext::new(
        Arc::new(oxidict_platform::selection::selected_text),
        Arc::new(|req: TranslateRequest| -> Result<TranslateResult> {
            translate_once(req).map(|(_instance, result)| result)
        }),
    );
    if let Err(e) = oxidict_server::start_from_config(ctx) {
        // 端口被占用（例如老版本同时在跑）不应阻塞主程序。
        log::warn!("外部调用服务未启动: {e}");
    }

    // 4. 剪切板监听与全局快捷键（配置开关控制）。
    // 统一通过托盘命令 channel 桥接：热键/托盘菜单/配置开关都只负责「发命令」，
    // 真正的状态变更在 GPUI 命令循环里执行，保证都在 GPUI 执行器上。
    let (tray_tx, tray_rx) =
        tokio::sync::mpsc::unbounded_channel::<oxidict_ui::tray::TrayCommand>();

    // 配置开关：开机若已启用剪切板监听，发一条 ToggleClipboardMonitor 让循环去开。
    if store.get_or(oxidict_core::config::keys::CLIPBOARD_MONITOR, false) {
        let _ = tray_tx.send(oxidict_ui::tray::TrayCommand::ToggleClipboardMonitor);
        log::info!("已请求开启剪切板监听");
    }
    for (name, cfg_key) in [
        (
            "selection_translate",
            oxidict_core::config::keys::HOTKEY_SELECTION_TRANSLATE,
        ),
        (
            "input_translate",
            oxidict_core::config::keys::HOTKEY_INPUT_TRANSLATE,
        ),
        (
            "ocr_recognize",
            oxidict_core::config::keys::HOTKEY_OCR_RECOGNIZE,
        ),
        (
            "ocr_translate",
            oxidict_core::config::keys::HOTKEY_OCR_TRANSLATE,
        ),
    ] {
        if let Some(keys) = store.get::<String>(cfg_key).filter(|s| !s.is_empty()) {
            // 生效链路唯一入口：映射与注册都在 oxidict_ui::hotkey 内，
            // 设置窗口改键后热更新走的也是同一个函数。
            oxidict_ui::hotkey::apply_hotkey(name, &keys);
        }
    }

    // 5. GPUI 事件循环（阻塞主线程直到退出）。
    //    自定义资产源：先查品牌 logo，找不到回退 gpui-kit 内建资产。
    gpui_kit::application()
        .with_assets(oxidict_ui::logos::CombinedAssets::new())
        .run(move |cx| {
            gpui_kit::init(cx);
            oxidict_ui::launch(cx, tray_tx, tray_rx);
        });
}
