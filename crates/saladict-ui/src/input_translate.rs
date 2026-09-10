//! 输入翻译独立窗口。
//!
//! 原版按快捷键弹出独立小窗（320x180），带 Textarea + 翻译按钮 + 结果显示。
//! 按 Enter 翻译，Esc 关闭。与主翻译窗口独立，不共享状态。

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::AppContext as _;
use gpui_kit::{
    div, px, App, Bounds, Context, Entity, IntoElement, ParentElement, Point, Render, SharedString,
    Size, Styled, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions,
};
use saladict_core::config::config;
use saladict_core::i18n::t;
use saladict_core::TranslateRequest;
use saladict_services::spawn_translate;
use std::sync::Arc;

enum InputCardState {
    Idle,
    Loading,
    Done(String),
    Failed(String),
}

pub struct InputTranslateWindow {
    source: Entity<InputState>,
    state: InputCardState,
}

impl InputTranslateWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let source =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("translate-source-placeholder")));

        // Enter 翻译。
        cx.subscribe_in(&source, window, |this, _, event, _window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = this.source.read(cx).value().trim().to_string();
                if !text.is_empty() {
                    this.do_translate(text, cx);
                }
            }
        })
        .detach();

        Self {
            source,
            state: InputCardState::Idle,
        }
    }

    fn do_translate(&mut self, text: String, cx: &mut Context<Self>) {
        let store = config();
        let from = store.source_language();
        let to = store.target_language();
        let list = store.service_list(saladict_core::config::keys::TRANSLATE_SERVICE_LIST);
        let Some(instance) = list.first().cloned() else {
            self.state = InputCardState::Failed(t("recognize-no-service"));
            cx.notify();
            return;
        };

        self.state = InputCardState::Loading;
        cx.notify();

        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let req = TranslateRequest::new(text, from, to).with_stream(Arc::new(move |delta| {
            let _ = stream_tx.send(delta);
        }));
        let task = spawn_translate(&instance, req);

        // 流式增量
        cx.spawn(async move |this, cx| {
            while let Some(delta) = stream_rx.recv().await {
                let text = delta.trim_end_matches('_').to_string();
                let _ = this.update(cx, |this, cx| {
                    this.state = InputCardState::Done(text);
                    cx.notify();
                });
            }
        })
        .detach();

        // 终态
        cx.spawn(async move |this, cx| {
            let state = match task.await {
                Ok(Ok(result)) => InputCardState::Done(result.as_text()),
                Ok(Err(e)) => InputCardState::Failed(e.to_string()),
                Err(e) => InputCardState::Failed(format!("{e}")),
            };
            let _ = this.update(cx, |this, cx| {
                this.state = state;
                cx.notify();
            });
        })
        .detach();
    }

    fn do_translate_click(
        &mut self,
        _: &gpui_kit::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.source.read(cx).value().trim().to_string();
        if !text.is_empty() {
            self.do_translate(text, cx);
        }
    }
}

impl Render for InputTranslateWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let result_area: gpui_kit::AnyElement = match &self.state {
            InputCardState::Idle => div()
                .text_size(px(12.))
                .text_color(theme.colors.muted_foreground)
                .child(t("translate-idle-hint"))
                .into_any_element(),
            InputCardState::Loading => h_flex()
                .gap_2()
                .child(Spinner::new().small())
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme.colors.muted_foreground)
                        .child(t("app-translating")),
                )
                .into_any_element(),
            InputCardState::Done(text) => div()
                .text_size(px(13.))
                .text_color(theme.colors.foreground)
                .child(SharedString::from(text.clone()))
                .into_any_element(),
            InputCardState::Failed(msg) => div()
                .text_size(px(12.))
                .text_color(theme.colors.danger)
                .child(SharedString::from(msg.clone()))
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            .p_2()
            .gap_2()
            .child(Input::new(&self.source).small())
            .child(result_area)
            .child(
                h_flex().justify_end().child(
                    Button::new("translate")
                        .primary()
                        .xsmall()
                        .label(t("translate-action"))
                        .on_click(cx.listener(Self::do_translate_click)),
                ),
            )
    }
}

/// 打开输入翻译窗口（320x180，屏幕右下角附近）。
pub fn open_input_translate(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point {
                x: px(880.0),
                y: px(420.0),
            },
            size: Size {
                width: px(320.0),
                height: px(180.0),
            },
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(t("translate-title").into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| InputTranslateWindow::new(window, cx));
        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
    })
    .expect("打开输入翻译窗口失败");
}
