//! 一次性探针：验证 keytap 在本机能否创建 CGEventTap。
//!
//! 运行：cargo run -p saladict-platform --example tap_probe
//! 预期：
//!   - 已授予「输入监控」权限 → Tap 创建成功（按 Ctrl+C 退出，会收键事件）
//!   - 未授权 → PermissionDenied（去 系统设置->隐私与安全性->输入监控 授权）
use std::time::Duration;

fn main() {
    println!("创建 CGEventTap（macOS 需要输入监控权限）…");
    match keytap::Tap::new() {
        Ok(tap) => {
            println!("✓ Tap 创建成功，监听 3 秒内按键…");
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if let Ok(ev) = tap.recv_timeout(Duration::from_millis(300)) {
                    println!("  收到事件: {:?}", ev.kind);
                }
            }
            println!("✓ 3 秒内事件通道工作正常，探针退出（Drop 即注销监听）");
        }
        Err(keytap::Error::PermissionDenied) => {
            eprintln!("✗ 权限被拒：请在 系统设置 -> 隐私与安全性 -> 输入监控 中授权后重试");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("✗ Tap 创建失败: {e}");
            std::process::exit(1);
        }
    }
}
