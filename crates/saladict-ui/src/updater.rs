//! 更新窗口。
//!
//! 对应原 `src/window/Updater`：显示当前版本，「检查更新」按钮拉取 GitHub
//! latest release 接口，「前往下载」按钮用系统浏览器打开发布页。

use gpui_kit::AppContext as _;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{ActiveTheme, Disableable, IconName, Sizable};
use gpui_kit::component::Root;
use gpui_kit::{
    div, px, App, AnyElement, AsyncApp, Bounds, ClickEvent, Context, FontWeight, IntoElement,
    InteractiveElement, ParentElement, Point, Render, SharedString, Size, Styled,
    StatefulInteractiveElement, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions,
};
use saladict_net::get_value;

pub struct UpdaterWindow {
    current_version: String,
    checking: bool,
    latest: Option<String>,
    body: Option<String>,
    error: Option<String>,
}

impl UpdaterWindow {
    pub fn new(current_version: String) -> Self {
        Self {
            current_version,
            checking: false,
            latest: None,
            body: None,
            error: None,
        }
    }

    fn check_update(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.checking = true;
        self.error = None;
        self.latest = None;
        self.body = None;
        cx.notify();

        let url =
            "https://api.github.com/repos/allentown521/saladict/releases/latest".to_string();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = get_value(url).await;
            let _ = this.update(cx, |this, cx| {
                this.checking = false;
                match result {
                    Ok(v) => {
                        let tag = v
                            .get("tag_name")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        let body = v
                            .get("body")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        if tag.is_empty() {
                            this.error = Some("未获取到版本信息".to_string());
                        } else {
                            this.latest = Some(tag);
                            this.body = Some(body);
                        }
                    }
                    Err(e) => {
                        this.error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_status(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        if let Some(error) = &self.error {
            return div()
                .text_color(theme.colors.danger)
                .text_size(px(12.))
                .child(SharedString::from(error.clone()))
                .into_any_element();
        }
        if let Some(latest) = &self.latest {
            return v_flex()
                .gap_1()
                .child(
                    div()
                        .text_color(theme.colors.foreground)
                        .text_size(px(13.))
                        .child(SharedString::from(format!("最新版本：{latest}"))),
                )
                .child(
                    div()
                        .text_color(theme.colors.muted_foreground)
                        .text_size(px(12.))
                        .child(SharedString::from(
                            self.body.clone().unwrap_or_default(),
                        )),
                )
                .into_any_element();
        }
        div()
            .text_color(theme.colors.muted_foreground)
            .text_size(px(12.))
            .child("点击「检查更新」查看是否有新版本")
            .into_any_element()
    }
}

impl Render for UpdaterWindow {
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
                            .child(SharedString::from(format!(
                                "沙拉翻译 · v{}",
                                self.current_version
                            ))),
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
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .px_4()
                    .py_3()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("check")
                                    .primary()
                                    .small()
                                    .label("检查更新")
                                    .disabled(self.checking)
                                    .on_click(cx.listener(Self::check_update)),
                            )
                            .child(if self.checking {
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme.colors.muted_foreground)
                                    .child("检查中…")
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.colors.border)
                            .p_2()
                            .child(self.render_status(cx)),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .child(
                                Button::new("download")
                                    .ghost()
                                    .small()
                                    .label("前往下载")
                                    .icon(IconName::ExternalLink)
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.open_url(
                                            "https://github.com/allentown521/saladict/releases/latest",
                                        );
                                    })),
                            ),
                    ),
            )
    }
}

/// 打开更新窗口。
///
/// 600x400，居中（此处用固定原点），显示当前版本并支持检查更新。
pub fn open_updater(cx: &mut App) {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point {
                x: px(360.),
                y: px(200.),
            },
            size: Size {
                width: px(600.),
                height: px(400.),
            },
        })),
        titlebar: Some(TitlebarOptions {
            title: Some("沙拉翻译 · 更新".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        ..Default::default()
    };

    let _ = cx.open_window(options, |window, cx| {
        let view = cx.new(|_cx| UpdaterWindow::new(current_version.clone()));
        cx.new(|cx| Root::new(view, window, cx))
    });
}
