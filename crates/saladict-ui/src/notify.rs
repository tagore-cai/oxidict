//! 通知窗口。
//!
//! 对应原 `src/window/Notify`：一个 always-on-top 的小窗，显示一条消息，
//! 3 秒后自动关闭，也可手动点关闭按钮立即关闭。

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::Root;
use gpui_kit::component::{ActiveTheme, IconName, Sizable};
use gpui_kit::AppContext as _;
use gpui_kit::{
    div, px, App, AsyncApp, Bounds, Context, FontWeight, IntoElement, ParentElement, Point, Render,
    SharedString, Size, Styled, Window, WindowBounds, WindowKind, WindowOptions,
};
use saladict_core::i18n::t;
use std::time::Duration;

pub struct NotifyWindow {
    message: String,
}

impl NotifyWindow {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl Render for NotifyWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .bg(crate::window_root_bg(theme.colors.background))
            .text_color(theme.colors.foreground)
            .rounded_md()
            .p_3()
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(t("app-name"))),
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
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .text_size(px(12.))
                    .text_color(theme.colors.foreground)
                    .child(SharedString::from(self.message.clone())),
            )
    }
}

/// 打开通知窗口。
///
/// 小尺寸 320x88、置顶（PopUp），显示 `message`，3 秒后自动关闭。
pub fn open_notify(cx: &mut App, message: String) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point {
                x: px(100.),
                y: px(100.),
            },
            size: Size {
                width: px(320.),
                height: px(88.),
            },
        })),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        window_background: crate::window_background_appearance(),
        ..Default::default()
    };

    let handle = cx
        .open_window(options, |window, cx| {
            let view = cx.new(|_cx| NotifyWindow::new(message.clone()));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .unwrap_or_else(|_| panic!("{}", t("notify-error-open")));

    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let _ = handle.update(cx, |_, window, _| {
            window.remove_window();
        });
    })
    .detach();
}
