//! 剪切板监听。
//!
//! 原实现（src-tauri/src/clipboard.rs）用 Tauri 的 ClipboardManager 轮询，
//! 这里换成 `arboard`，逻辑一致：定时轮询文本，内容变化时回调，并做去抖。

use arboard::Clipboard;
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 开始监听剪切板。文本发生变化时调用 `callback`。
///
/// 返回一个停止句柄，drop 掉不会停止，需要调用 `stop()`。
pub fn start_monitor(callback: Arc<dyn Fn(String) + Send + Sync>) -> MonitorHandle {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("oxidict-clipboard".into())
        .spawn(move || {
            let Ok(mut clipboard) = Clipboard::new() else {
                log::error!("无法打开剪切板，监听未启动");
                return;
            };
            let mut last: Option<String> = None;
            loop {
                // 停止信号优先：收到就退出线程。
                if matches!(rx.try_recv(), Ok(())) {
                    break;
                }
                if let Ok(text) = clipboard.get_text() {
                    let changed = last.as_ref() != Some(&text);
                    // 跳过空串与过短内容，避免误触发。
                    if changed && text.trim().len() >= 2 {
                        last = Some(text.clone());
                        callback(text);
                    } else if changed {
                        last = Some(text);
                    }
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        })
        .expect("failed to spawn clipboard monitor thread");

    MonitorHandle { stop: tx }
}

pub struct MonitorHandle {
    stop: std::sync::mpsc::Sender<()>,
}

impl MonitorHandle {
    pub fn stop(&self) {
        let _ = self.stop.send(());
    }
}
