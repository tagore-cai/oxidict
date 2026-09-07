//! 全局 tokio 运行时。
//!
//! 服务层全部是 tokio 生态（reqwest 的 IO 依赖 tokio 反应器），而 GPUI 自带
//! 独立执行器、不注入 tokio 上下文。所以这里持有一个进程级 runtime：
//! UI 侧用 [`handle`] 拿句柄，`handle.spawn(...)` 后在 GPUI 的 future 里
//! await `JoinHandle`——JoinHandle 本身不依赖 tokio 线程局部，可以跨执行器等待。

use once_cell::sync::Lazy;
use std::future::Future;
use tokio::runtime::{Handle, Runtime};

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .expect("无法创建全局 tokio 运行时")
});

/// 全局运行时句柄。首次调用会初始化 runtime。
pub fn handle() -> Handle {
    RUNTIME.handle().clone()
}

/// 把一个 future 丢到全局 runtime 上执行，返回可跨执行器等待的 JoinHandle。
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    <F as Future>::Output: Send + 'static,
{
    handle().spawn(future)
}
