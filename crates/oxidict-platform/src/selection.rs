//! 取当前焦点的选中文本。

use oxidict_core::Result;

/// 读取当前焦点元素里被选中的文本。
///
/// macOS 上 `selection` crate 内部通过 Accessibility（`AXUIElement`）直接读取焦点元素的
/// 选中文本；当目标应用不支持 AX 时，`selection` crate 会退回「复制 + 读剪切板 + 还原」
/// 的模拟方式。因此本函数天然满足「优先 AX、失败退回剪切板模拟」的平台要求，且零额外依赖。
pub fn selected_text() -> Result<String> {
    let text = selection::get_text();
    if text.trim().is_empty() {
        Err(oxidict_core::Error::Platform(
            "没有取到选中文本（目标应用可能未授权辅助功能）".into(),
        ))
    } else {
        Ok(text)
    }
}
