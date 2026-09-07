//! saladict：gpui-kit + Rust 重写版的可执行入口。
//!
//! 启动顺序：配置装载 -> 内置服务注册 -> 外部调用 HTTP 服务（独立线程）->
//! 剪切板监听与全局快捷键 -> GPUI 主窗口（阻塞在事件循环上）。

use saladict_core::config::ConfigStore;
use saladict_core::{Error, Result, TranslateRequest, TranslateResult};
use saladict_services::Translator;
use std::sync::Arc;

/// 用启用列表里的第一个实例翻译一段文本（CLI 与 HTTP 服务共用）。
fn translate_once(text: &str) -> Result<(String, TranslateResult)> {
    let store = saladict_core::config::config();
    let list = store.service_list(saladict_core::config::keys::TRANSLATE_SERVICE_LIST);
    let instance = list
        .first()
        .ok_or_else(|| Error::Service("翻译服务列表为空".into()))?;
    let (svc, cfg) = saladict_services::services().resolve_translator(instance, &store)?;
    let from = store.source_language();
    let to = store.target_language();
    let req = TranslateRequest::new(text, from, to).with_config(cfg);
    let result = saladict_core::runtime::handle().block_on(svc.translate(req))?;

    // 写入历史库，与老版本行为一致。
    let _ = saladict_core::history::History::global().add(
        text,
        from.code(),
        to.code(),
        instance,
        &result.as_text(),
    );
    Ok((instance.to_string(), result))
}

fn run_cli_translate(text: &str) {
    ConfigStore::init().expect("初始化配置失败");
    saladict_services::init_builtin_services();
    saladict_core::history::History::init_default().expect("历史库初始化失败");

    match translate_once(text) {
        Ok((instance, result)) => {
            println!("[{instance}] {}", result.as_text());
        }
        Err(e) => {
            eprintln!("翻译失败: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // 0. CLI 模式：`saladict --translate "文本"` 直接翻译并输出，用于端到端验证。
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--translate" {
        run_cli_translate(&args[2]);
        return;
    }

    // 0.5 单实例保护：锁文件 + PID 检测（脏锁自动清理）。
    let lock_path = dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("saladict-app.lock");
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
                .map(|pid| pid != std::process::id() && unsafe { libc::kill(pid as i32, 0) != 0 })
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
                eprintln!("检测到另一个 saladict 实例正在运行，退出。");
                std::process::exit(0);
            }
        }
    }

    // 1. 配置：沿用老版本 config.json，老用户配置直接迁移。
    let store = ConfigStore::init().expect("初始化配置失败");

    // 1.2 语言：按 app_language 初始化 Fluent 本地化。
    saladict_core::i18n::init_from_config();
    log::info!("配置目录: {}", store.app_dir().display());

    // 1.5 历史库：与老版本同一个 history.db。
    saladict_core::history::History::init_default().expect("历史库初始化失败");

    // 2. 注册内置服务（翻译 30 / OCR 15 / TTS 3 / 生词本 2）。
    saladict_services::init_builtin_services();
    let services = saladict_services::services();
    log::info!(
        "内置服务就绪: translate {} / recognize {} / tts {} / collection {}",
        services.translators().len(),
        services.recognizers().len(),
        services.tts_list().len(),
        services.collectors().len()
    );

    // 3. 外部调用 HTTP 服务（tiny_http 阻塞线程，翻译回调走全局 tokio runtime）。
    let ctx = saladict_server::ServerContext::new(
        Arc::new(|| saladict_platform::selection::selected_text()),
        Arc::new(|req: TranslateRequest| -> Result<TranslateResult> {
            let text = req.text.clone();
            let from = req.from;
            let to = req.to;
            let (instance, result) = {
                let store = saladict_core::config::config();
                let list = store.service_list(saladict_core::config::keys::TRANSLATE_SERVICE_LIST);
                let instance = list
                    .first()
                    .ok_or_else(|| Error::Service("翻译服务列表为空".into()))?
                    .to_string();
                let (svc, cfg) = saladict_services::services().resolve_translator(&instance, &store)?;
                (
                    instance.clone(),
                    saladict_core::runtime::handle().block_on(svc.translate(req.with_config(cfg)))?,
                )
            };
            let _ = saladict_core::history::History::global().add(
                &text,
                from.code(),
                to.code(),
                &instance,
                &result.as_text(),
            );
            Ok(result)
        }),
    );
    if let Err(e) = saladict_server::start_from_config(ctx) {
        // 端口被占用（例如老版本同时在跑）不应阻塞主程序。
        log::warn!("外部调用服务未启动: {e}");
    }

    // 4. 剪切板监听与全局快捷键（配置开关控制）。
    // 统一通过托盘命令 channel 桥接：热键/托盘菜单/配置开关都只负责「发命令」，
    // 真正的状态变更在 GPUI 命令循环里执行，保证都在 GPUI 执行器上。
    let (tray_tx, tray_rx) =
        tokio::sync::mpsc::unbounded_channel::<saladict_ui::tray::TrayCommand>();

    // 配置开关：开机若已启用剪切板监听，发一条 ToggleClipboardMonitor 让循环去开。
    if store.get_or(saladict_core::config::keys::CLIPBOARD_MONITOR, false) {
        let _ = tray_tx.send(saladict_ui::tray::TrayCommand::ToggleClipboardMonitor);
        log::info!("已请求开启剪切板监听");
    }
    for (name, cfg_key) in [
        ("selection_translate", "hotkey_selection_translate"),
        ("input_translate", "hotkey_input_translate"),
        ("ocr_recognize", "hotkey_ocr_recognize"),
        ("ocr_translate", "hotkey_ocr_translate"),
    ] {
        if let Some(keys) = store.get::<String>(cfg_key).filter(|s| !s.is_empty()) {
            let cmd = match name {
                "selection_translate" => saladict_ui::tray::TrayCommand::SelectionTranslate,
                "input_translate" => saladict_ui::tray::TrayCommand::InputTranslate,
                "ocr_recognize" => saladict_ui::tray::TrayCommand::OcrRecognize,
                "ocr_translate" => saladict_ui::tray::TrayCommand::OcrTranslate,
                _ => continue,
            };
            let tx = tray_tx.clone();
            saladict_platform::hotkey::register(name, &keys, Arc::new(move || {
                let _ = tx.send(cmd);
            }));
            log::info!("已注册快捷键 {keys} -> {name}");
        }
    }

    // 5. GPUI 事件循环（阻塞主线程直到退出）。
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            saladict_ui::launch(cx, tray_tx, tray_rx);
        });
}
