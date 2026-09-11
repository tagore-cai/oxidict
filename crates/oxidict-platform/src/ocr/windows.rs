//! Windows 系统 OCR：使用 `windows` crate 调用 `Windows.Media.Ocr`。
//!
//! 注意：本分支无法在本机（macOS）完整编译（`rusqlite` 的 bundled C 依赖需要
//! Windows 工具链），但针对 `windows` 0.62 的调用形状已在隔离 crate 中对
//! `x86_64-pc-windows-msvc` 做过类型校验（cargo check 通过），完整编译仍由
//! Windows 环境承担。
//! `windows::Error` 实现了 `std::error::Error`，故可在 `anyhow::Result` 内直接用 `?`。

use oxidict_core::{Language, Result};

pub fn system_ocr(image: &[u8], lang: Language) -> Result<String> {
    fn inner(image: &[u8], lang: Language) -> anyhow::Result<String> {
        use windows::Globalization::Language as WinLanguage;
        use windows::Graphics::Imaging::BitmapDecoder;
        use windows::Media::Ocr::OcrEngine;
        use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
        use windows::core::HSTRING;

        // 刻意保持 runtime class 类型而不提前 upcast 成接口：windows 0.62 的
        // `Param<T>` 依赖 `CanInto`（实现类 → 接口）来做参数转换，若变量被标注为
        // `IRandomAccessStream`，`&stream` 就无法满足 `Param<IOutputStream>`，
        // `DataWriter::CreateDataWriter` 会编译失败。
        let stream = InMemoryRandomAccessStream::new()?;
        {
            let writer = DataWriter::CreateDataWriter(&stream)?;
            writer.WriteBytes(image)?;
            // 0.62 起异步等待由 windows_future 的 `join()` 提供（旧版为 `get()`）。
            writer.StoreAsync()?.join()?;
            writer.FlushAsync()?.join()?;
        }
        stream.Seek(0u64)?;

        // 0.62 起工厂方法名去掉了 WithStream 后缀，直接 `CreateAsync`。
        let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
        let bitmap = decoder.GetSoftwareBitmapAsync()?.join()?;

        let engine = if lang == Language::Auto {
            OcrEngine::TryCreateFromUserProfileLanguages()?
        } else {
            let wl = WinLanguage::CreateLanguage(&HSTRING::from(lang.code()))?;
            OcrEngine::TryCreateFromLanguage(&wl)?
        };

        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        Ok(result.Text()?.to_string_lossy().to_string())
    }

    inner(image, lang).map_err(|e| oxidict_core::Error::Platform(e.to_string()))
}
