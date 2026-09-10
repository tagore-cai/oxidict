//! Windows 系统 OCR：使用 `windows` crate 调用 `Windows.Media.Ocr`。
//!
//! 注意：本分支无法在本机（macOS）验证，仅保证代码正确，编译由 CI/Windows 环境承担。
//! `windows::Error` 实现了 `std::error::Error`，故可在 `anyhow::Result` 内直接用 `?`。

use oxidict_core::{Language, Result};

pub fn system_ocr(image: &[u8], lang: Language) -> Result<String> {
    fn inner(image: &[u8], lang: Language) -> anyhow::Result<String> {
        use windows::Globalization::Language as WinLanguage;
        use windows::Graphics::Imaging::BitmapDecoder;
        use windows::Media::Ocr::OcrEngine;
        use windows::Storage::Streams::{
            DataWriter, IRandomAccessStream, InMemoryRandomAccessStream,
        };
        use windows::core::HSTRING;

        // 把图像字节灌入内存流，再解码为 SoftwareBitmap。
        let stream: IRandomAccessStream = InMemoryRandomAccessStream::new()?.into();
        {
            let writer = DataWriter::CreateDataWriter(stream.clone())?;
            writer.WriteBytes(image)?;
            writer.StoreAsync()?.get()?;
            writer.FlushAsync()?.get()?;
        }
        stream.Seek(0u64)?;

        let decoder = BitmapDecoder::CreateWithStreamAsync(&stream)?.get()?;
        let bitmap = decoder.GetSoftwareBitmapAsync()?.get()?;

        let engine = if lang == Language::Auto {
            OcrEngine::TryCreateFromUserProfileLanguages()?
        } else {
            let wl = WinLanguage::CreateLanguage(&HSTRING::from(lang.code()))?;
            OcrEngine::TryCreateFromLanguage(&wl)?
        };

        let result = engine.RecognizeAsync(&bitmap)?.get()?;
        Ok(result.Text()?.to_string_lossy().to_string())
    }

    inner(image, lang).map_err(|e| oxidict_core::Error::Platform(e.to_string()))
}
