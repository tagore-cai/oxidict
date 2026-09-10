//! 设置窗口。
//!
//! 对应原 `src/window/Config`：左侧栏（Logo + 图标菜单，选中高亮）+
//! 右侧内容区（页标题 + 分隔线 + 滚动区），设置行沿用原版
//! `.config-item` 的「左标签右控件」布局。
//!
//! 按职责拆分：
//! - [`services`]：服务实例管理——实例列表增删移、schema 驱动的配置表单，
//!   翻译/识别/TTS/生词本四个分类页共用同一实现（原版 30 份手写
//!   Config.jsx 在这里归一成一个通用表单渲染器）；
//! - [`pages`]：快捷键、通用、历史、备份页。

mod pages;
mod services;

use crate::hotkey::HotkeyRecorderState;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::SelectState;
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::AppContext as _;
use gpui_kit::{
    div, px, App, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement, Point,
    Render, ScrollHandle, SharedString, Size, StatefulInteractiveElement, Styled, TitlebarOptions,
    Window, WindowBounds, WindowKind, WindowOptions,
};
use saladict_core::config::{config, keys};
use saladict_core::i18n::{t, t_args};
use saladict_core::ServiceKind;
use services::FormState;

/// 侧栏导航项：图标 + 标题（标题存 i18n key，渲染时再 `t()` 翻译）。
const NAV_ITEMS: [(IconName, &str); 8] = [
    (IconName::Globe, "config-nav-translate"),
    (IconName::Search, "config-nav-recognize"),
    (IconName::Play, "config-nav-tts"),
    (IconName::BookOpen, "config-nav-collection"),
    (IconName::Settings2, "config-nav-hotkey"),
    (IconName::Palette, "config-nav-general"),
    (IconName::HardDrive, "config-nav-backup"),
    (IconName::Calendar, "config-nav-history"),
];

/// 服务分类的展示名（i18n key），按导航顺序。
const KIND_TABS: [(ServiceKind, &str); 4] = [
    (ServiceKind::Translate, "config-nav-translate"),
    (ServiceKind::Recognize, "config-nav-recognize"),
    (ServiceKind::Tts, "config-nav-tts"),
    (ServiceKind::Collection, "config-nav-collection"),
];

/// 前四项对应的服务分类。
pub(super) fn nav_kind(nav: usize) -> Option<ServiceKind> {
    match nav {
        0 => Some(ServiceKind::Translate),
        1 => Some(ServiceKind::Recognize),
        2 => Some(ServiceKind::Tts),
        3 => Some(ServiceKind::Collection),
        _ => None,
    }
}

pub struct ConfigWindow {
    nav: usize,
    /// 每个服务分类的「添加服务」下拉（切页或列表变动时重建）。
    add_state: Option<(ServiceKind, Entity<SelectState<Vec<&'static str>>>)>,
    form: Option<FormState>,
    scroll: ScrollHandle,
    // 快捷键：(i18n key, 录制槽实体, 配置键名)
    hotkey_inputs: Vec<(&'static str, Entity<HotkeyRecorderState>, &'static str)>,
    clipboard_monitor: bool,
    dark_mode: bool,
    proxy_enable: bool,
    proxy_host: Entity<InputState>,
    proxy_port: Entity<InputState>,
    saved_hint: bool,
    // 代理认证
    proxy_username: Entity<InputState>,
    proxy_password: Entity<InputState>,
    no_proxy: Entity<InputState>,
    // 高级：HTTP 服务端口 / 开发者模式（对齐原版 Advance 页）
    server_port: Entity<InputState>,
    dev_mode: bool,
    // 外观与系统集成（对齐原版 General 页）
    transparent: bool,
    hide_dock: bool,
    // 备份
    webdav_url: Entity<InputState>,
    webdav_username: Entity<InputState>,
    webdav_password: Entity<InputState>,
    backup_status: Option<String>,
}

impl ConfigWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hotkey_inputs = [
            ("config-hotkey-selection", "selection_translate"),
            ("config-hotkey-input", "input_translate"),
            ("config-hotkey-ocr", "ocr_recognize"),
            ("config-hotkey-ocr-translate", "ocr_translate"),
        ]
        .into_iter()
        .map(|(i18n_key, name)| {
            let full = format!("{}{}", keys::HOTKEY_PREFIX, name);
            let value = config().get::<String>(&full).unwrap_or_default();
            // 录制的组合键实时写回 config.json，并立即重新注册（录入即生效，
            // 空值注销）。映射与注册统一走 hotkey::apply_hotkey。
            let key_name = full.clone();
            let rec = cx.new(|cx| {
                HotkeyRecorderState::new(window, cx).on_change(move |v, cx| {
                    // 冲突检测：组合键已绑到其它动作时拒绝写入，通知用户
                    // （原版 isRegistered + toast 的对等实现）。
                    if !v.is_empty() {
                        if let Some(other) = saladict_platform::hotkey::find_conflict(v, name) {
                            crate::notify::open_notify(
                                cx,
                                t_args("config-hotkey-conflict", &[("name", &other)]),
                            );
                            return;
                        }
                    }
                    let _ = config().set(&key_name, &v.to_string());
                    // 录入即生效：注册（非空）/注销（空）。
                    crate::hotkey::apply_hotkey(name, v);
                })
            });
            rec.update(cx, |r, cx| r.set_value(value, cx));
            (i18n_key, rec, name)
        })
        .collect();

        let proxy_host = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::PROXY_HOST)
                    .unwrap_or_else(|| "127.0.0.1".into()),
            )
        });
        let proxy_port = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config().get_or(keys::PROXY_PORT, 1087).to_string())
        });
        let proxy_username = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::PROXY_USERNAME)
                    .unwrap_or_default(),
            )
        });
        let proxy_password = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::PROXY_PASSWORD)
                    .unwrap_or_default(),
            )
        });
        let no_proxy = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config().get::<String>(keys::NO_PROXY).unwrap_or_default())
        });
        let server_port = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config().get_or(keys::SERVER_PORT, 60606).to_string())
        });

        let webdav_url = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config().get::<String>(keys::WEBDAV_URL).unwrap_or_default())
        });
        let webdav_username = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::WEBDAV_USERNAME)
                    .unwrap_or_default(),
            )
        });
        let webdav_password = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::WEBDAV_PASSWORD)
                    .unwrap_or_default(),
            )
        });

        let mut this = Self {
            nav: 0,
            add_state: None,
            form: None,
            scroll: ScrollHandle::new(),
            hotkey_inputs,
            clipboard_monitor: config().get_or(keys::CLIPBOARD_MONITOR, false),
            dark_mode: {
                let theme: String = config().get_or(keys::APP_THEME, "system".into());
                theme == "dark"
            },
            proxy_enable: config().get_or(keys::PROXY_ENABLE, false),
            proxy_host,
            proxy_port,
            proxy_username,
            proxy_password,
            no_proxy,
            server_port,
            dev_mode: config().get_or(keys::DEV_MODE, false),
            transparent: config().get_or(keys::TRANSPARENT, true),
            hide_dock: config().get_or(keys::HIDE_DOCK_ICON, false),
            saved_hint: false,
            webdav_url,
            webdav_username,
            webdav_password,
            backup_status: None,
        };
        this.refresh_add_state(window, cx);
        this
    }
}

impl Render for ConfigWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 先取出颜色（Hsla 是 Copy），避免借用跨越 &mut 调用。
        let (sidebar_bg, sidebar_fg, sidebar_accent, sidebar_accent_fg, sidebar_border) = {
            let t = cx.theme();
            (
                t.colors.sidebar,
                t.colors.sidebar_foreground,
                t.colors.sidebar_accent,
                t.colors.sidebar_accent_foreground,
                t.colors.sidebar_border,
            )
        };
        let (fg, muted_fg, border, bg) = {
            let t = cx.theme();
            (
                t.colors.foreground,
                t.colors.muted_foreground,
                t.colors.border,
                t.colors.background,
            )
        };

        // ── 侧栏 ──（transparent 开启时压低背景不透明度，毛玻璃透出）
        let logo_char: String = t("app-name").chars().take(1).collect();
        let logo = h_flex()
            .gap_2()
            .items_center()
            .px_1()
            .child(
                div()
                    .size(px(34.))
                    .rounded_lg()
                    .bg(cx.theme().colors.primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(cx.theme().colors.primary_foreground)
                            .child(SharedString::from(logo_char)),
                    ),
            )
            .child(
                v_flex()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .child(SharedString::from(t("app-name"))),
                    )
                    .child(div().text_size(px(10.)).text_color(muted_fg).child(
                        SharedString::from(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    )),
            );

        let menu =
            v_flex()
                .gap_1()
                .children(NAV_ITEMS.iter().enumerate().map(|(ix, (icon, label))| {
                    let selected = self.nav == ix;
                    h_flex()
                        .id(("nav", ix))
                        .w_full()
                        .px_2()
                        .py_1()
                        .gap_2()
                        .items_center()
                        .rounded_md()
                        .when(selected, |this| {
                            this.bg(sidebar_accent).text_color(sidebar_accent_fg)
                        })
                        .when(!selected, |this| this.text_color(sidebar_fg))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.nav = ix;
                            this.form = None;
                            this.saved_hint = false;
                            this.refresh_add_state(window, cx);
                        }))
                        .child(Icon::new(icon.clone()).small())
                        .child(div().text_size(px(13.)).child(SharedString::from(t(label))))
                }));

        let sidebar = v_flex()
            .w(px(190.))
            .h_full()
            .bg(crate::window_root_bg(sidebar_bg))
            .border_r_1()
            .border_color(sidebar_border)
            .p_2()
            .gap_3()
            .child(logo)
            .child(menu)
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(muted_fg)
                    .child(SharedString::from(t("app-tagline"))),
            );

        // ── 内容区 ──
        let page_title = if self.nav < 4 {
            match nav_kind(self.nav).unwrap() {
                ServiceKind::Translate => KIND_TABS[0].1,
                ServiceKind::Recognize => KIND_TABS[1].1,
                ServiceKind::Tts => KIND_TABS[2].1,
                ServiceKind::Collection => KIND_TABS[3].1,
            }
        } else {
            NAV_ITEMS[self.nav].1
        };

        let content: gpui_kit::AnyElement = match self.nav {
            0..=3 => self.render_service_page(cx),
            4 => self.render_hotkey_page(cx),
            5 => self.render_general_page(cx),
            6 => self.render_backup_page(window, cx),
            7 => self.render_history_page(cx),
            _ => self.render_general_page(cx),
        };

        v_flex()
            .size_full()
            .flex_row()
            .text_color(fg)
            .child(sidebar)
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .bg(crate::window_root_bg(bg))
                    .child(
                        // 页头：标题 + 拖拽区
                        h_flex()
                            .id("titlebar")
                            .w_full()
                            .h(px(44.))
                            .px_3()
                            .items_center()
                            .border_b_1()
                            .border_color(border)
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .child(SharedString::from(t(page_title))),
                            ),
                    )
                    .child(
                        div()
                            .id("config-content")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(content),
                    ),
            )
    }
}

/// 打开设置窗口（820x560 居中）。
pub fn open_config(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point {
                x: px(310.0),
                y: px(140.0),
            },
            size: Size {
                width: px(820.0),
                height: px(560.0),
            },
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(t("config-title").into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: false,
        show: true,
        kind: WindowKind::Normal,
        window_background: crate::window_background_appearance(),
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| ConfigWindow::new(window, cx));
        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
    })
    .expect("打开设置窗口失败");
}
