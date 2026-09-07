//! 识别窗口。
//!
//! 对应原 `src/window/Recognize`：截图后 OCR。上部图片区（本版本 gpui 无 `img`
//! 元素，用占位框 + 文件大小文本代替），下部结果文本区，底部「开始识别」与
//! 「复制结果」按钮。
//!
//! 注意：三个薄窗口文件互不引用，各自独立。

use gpui_kit::AppContext as _;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{ActiveTheme, Disableable, IconName, Sizable};
use gpui_kit::component::Root;
use gpui_kit::{
    div, px, App, AnyElement, Bounds, ClickEvent, ClipboardItem, Context, FontWeight, IntoElement,
    InteractiveElement, ParentElement, Point, Render, SharedString, Size, Styled,
    StatefulInteractiveElement, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions,
};
use saladict_core::config::{config, keys};
use saladict_core::i18n::{t, t_args};
use saladict_core::{Language, RecognizeRequest};
use saladict_services::spawn_recognize;

pub struct RecognizeWindow {
    image: Option<Vec<u8>>,
    language: Language,
    instances: Vec<String>,
    text: String,
    recognizing: bool,
    error: Option<String>,
}

impl RecognizeWindow {
    pub fn new(image: Option<Vec<u8>>) -> Self {
        let store = config();
        let language = store.source_language();
        let instances = store.service_list(keys::RECOGNIZE_SERVICE_LIST);
        Self {
            image,
            language,
            instances,
            text: String::new(),
            recognizing: false,
            error: None,
        }
    }

    fn start_recognize(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let image = match &self.image {
            Some(img) => img.clone(),
            None => {
                self.error = Some(t("recognize-no-image-error"));
                cx.notify();
                return;
            }
        };
        let instance = match self.instances.first().cloned() {
            Some(i) => i,
            None => {
                self.error = Some(t("recognize-no-service"));
                cx.notify();
                return;
            }
        };
        let language = self.language;
        let req = RecognizeRequest {
            image,
            language,
            config: Default::default(),
        };

        self.recognizing = true;
        self.error = None;
        cx.notify();

        let task = spawn_recognize(&instance, req);
        cx.spawn(async move |this, cx: &mut gpui_kit::AsyncApp| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.recognizing = false;
                match result {
                    Ok(Ok(text)) => {
                        this.text = text;
                        this.error = None;
                    }
                    Ok(Err(e)) => {
                        this.error = Some(e.to_string());
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        this.error = Some(t_args("task-cancelled", &[("error", &msg)]));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn copy_result(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
        }
    }

    /// 把识别结果送入翻译窗口并触发翻译。
    fn send_to_translate(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.text.is_empty() {
            crate::translate_text(&self.text, cx);
        }
    }

    fn render_image(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        match &self.image {
            Some(bytes) => {
                let bytes_label = bytes.len().to_string();
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.colors.border)
                    .bg(theme.colors.muted)
                    .text_color(theme.colors.muted_foreground)
                    .text_size(px(12.))
                    .child(SharedString::from(t_args(
                        "recognize-image-bytes",
                        &[("bytes", &bytes_label)],
                    )))
                    .into_any_element()
            }
            None => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .rounded_md()
                .border_1()
                .border_color(theme.colors.border)
                .text_color(theme.colors.muted_foreground)
                .text_size(px(12.))
                .child(SharedString::from(t("recognize-no-image")))
                .into_any_element(),
        }
    }
}

impl Render for RecognizeWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            .child(
                h_flex()
                    .id("titlebar")
                    .w_full()
                    .h(px(38.))
                    .px_2()
                    .items_center()
                    .justify_between()
                    .bg(theme.colors.muted)
                    .border_b_1()
                    .border_color(theme.colors.border)
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(t("recognize-window-title"))),
                    )
                    .child(
                        Button::new("close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(|_, window, _| {
                                window.remove_window();
                            }),
                    ),
            )
            // 上部图片区
            .child(
                div()
                    .h(px(180.))
                    .flex_none()
                    .m_2()
                    .child(self.render_image(cx)),
            )
            // 下部结果文本区
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .mx_2()
                    .mb_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.colors.border)
                    .p_2()
                    .text_size(px(12.))
                    .text_color(theme.colors.foreground)
                    .child(SharedString::from(self.text.clone())),
            )
            // 底部操作行
            .child(
                h_flex()
                    .px_2()
                    .pb_2()
                    .justify_between()
                    .child(
                        Button::new("recognize")
                            .primary()
                            .small()
                            .label(SharedString::from(t("recognize-action")))
                            .icon(IconName::Search)
                            .disabled(self.recognizing || self.image.is_none())
                            .on_click(cx.listener(Self::start_recognize)),
                    )
                    .child(
                        Button::new("copy")
                            .ghost()
                            .small()
                            .label(SharedString::from(t("recognize-copy")))
                            .icon(IconName::Copy)
                            .disabled(self.text.is_empty())
                            .on_click(cx.listener(Self::copy_result)),
                    )
                    .child(
                        Button::new("translate")
                            .secondary()
                            .small()
                            .label(SharedString::from(t("recognize-to-translate")))
                            .icon(IconName::BookOpen)
                            .disabled(self.text.is_empty())
                            .on_click(cx.listener(Self::send_to_translate)),
                    ),
            )
    }
}

/// 打开识别窗口并带入一张截图。
///
/// 640x420，正常窗口。图片后续由截图流程传入；无图时显示占位框。
pub fn open_recognize_with_image(cx: &mut App, image: Vec<u8>) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point {
                x: px(140.),
                y: px(120.),
            },
            size: Size {
                width: px(640.),
                height: px(420.),
            },
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from(t("recognize-window-title"))),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        ..Default::default()
    };

    let _ = cx.open_window(options, |window, cx| {
        let view = cx.new(|_cx| RecognizeWindow::new(Some(image.clone())));
        cx.new(|cx| Root::new(view, window, cx))
    });
}
