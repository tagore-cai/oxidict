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
use crate::tokens::Text;
use gpui_kit::AppContext as _;
use gpui_kit::base::{IndexPath, h_flex, v_flex};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement, Point, Render,
    ScrollHandle, SharedString, Size, StatefulInteractiveElement, Styled, TitlebarOptions, Window,
    WindowBounds, WindowKind, WindowOptions, div, px,
};
use oxidict_core::ServiceKind;
use oxidict_core::config::{config, keys};
use oxidict_core::i18n::{t, t_args};
use oxidict_net::backup_backend::{KIND_LOCAL_FOLDER, KIND_WEBDAV};
use services::FormState;

/// 界面字号的可选档位（对应 `app_font_size` 的像素值）。
///
/// 用固定档位而不是自由输入：本工程大量使用固定像素内边距，放一个任意数值
/// 进去很容易把布局挤爆（tokens 侧另有 [0.75, 1.6] 的缩放夹紧做兜底）。
const FONT_SIZE_STEPS: [f32; 4] = [14.0, 16.0, 18.0, 20.0];

/// 由已配置字号推下拉下标；不在档位内时取最接近的一档。
fn font_size_index(configured: f32) -> usize {
    let mut best = 0;
    let mut best_delta = f32::MAX;
    for (ix, step) in FONT_SIZE_STEPS.iter().enumerate() {
        let delta = (step - configured).abs();
        if delta < best_delta {
            best_delta = delta;
            best = ix;
        }
    }
    best
}

/// 主题选项的 i18n key，下标即选项序号：0=跟随系统，1=浅色，2=深色。
const THEME_KEYS: [&str; 3] = [
    "config-theme-system",
    "config-theme-light",
    "config-theme-dark",
];

/// 侧栏导航项：图标 + 标题（标题存 i18n key，渲染时再 `t()` 翻译）。
/// 历史页每次展示的条数，也是「加载更多」的步长。
///
/// 用 u32 是因为 `History::list/search` 的 limit 形参就是 u32，
/// 免得在调用点来回转换。
pub(super) const HISTORY_PAGE: u32 = 100;

const NAV_ITEMS: [(IconName, &str); 9] = [
    (IconName::Globe, "config-nav-translate"),
    (IconName::Search, "config-nav-recognize"),
    (IconName::Play, "config-nav-tts"),
    (IconName::BookOpen, "config-nav-collection"),
    (IconName::Settings2, "config-nav-hotkey"),
    (IconName::Palette, "config-nav-general"),
    (IconName::HardDrive, "config-nav-backup"),
    (IconName::Calendar, "config-nav-history"),
    // NOTE：`IconName` 是 gpui-component 的固定子集（完整 Lucide 目录在 gpui-kit-assets），
    // 子集里没有 Puzzle 之类的名字，这里先用一个已验证存在、且不与其它导航项重复的图标。
    (IconName::Replace, "config-nav-custom"),
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
    /// 主题三态选择（跟随系统 / 浅色 / 深色），变更即写回并应用。
    theme_select: Entity<SelectState<Vec<SharedString>>>,
    /// 界面语言选择。20 个 locale 一直都在，但此前没有任何入口能切换。
    language_select: Entity<SelectState<Vec<&'static str>>>,
    /// 界面字体族（对应 `app_font`，此前是死键）。
    font_family: Entity<InputState>,
    /// 界面字号（对应 `app_font_size`，此前是死键）。
    font_size: Entity<SelectState<Vec<SharedString>>>,
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
    /// 备份落点选择：0=WebDAV，1=本地目录（顺序即 `BACKUP_TYPE` 的映射）。
    backup_kind: Entity<SelectState<Vec<SharedString>>>,
    /// 本地目录后端的目标目录。
    backup_folder: Entity<InputState>,
    webdav_url: Entity<InputState>,
    webdav_username: Entity<InputState>,
    webdav_password: Entity<InputState>,
    backup_status: Option<String>,
    /// 云端备份列表（拉取成功后填充；`None` 表示尚未拉取）。
    remote_backups: Option<Vec<String>>,
    /// 待用户确认的恢复操作（`None` 表示当前没有待确认项）。
    pending_restore: Option<PendingRestore>,
    // 自定义服务（配置驱动，见 oxidict-services 的 translate::custom）
    custom_id: Entity<InputState>,
    custom_endpoint: Entity<InputState>,
    custom_method: Entity<SelectState<Vec<&'static str>>>,
    custom_auth: Entity<SelectState<Vec<SharedString>>>,
    custom_auth_name: Entity<InputState>,
    custom_body: Entity<InputState>,
    custom_extract: Entity<InputState>,
    custom_tls: Entity<SelectState<Vec<SharedString>>>,
    /// 保存/校验结果提示（错误直接展示给用户）。
    custom_status: Option<String>,

    /// 历史页已展示的条数上限（「加载更多」按步长递增）。
    history_limit: u32,
    /// 历史页搜索框（空表示列出最近记录）。
    history_query: Entity<InputState>,
}

/// 待确认的恢复来源。
///
/// 恢复会整份替换本地 `config.json` 与 `history.db`，属于破坏性操作，
/// 必须先让用户看清「将恢复哪一份」再确认，不能静默取最后一个。
pub(super) enum PendingRestore {
    /// 云端备份文件名（确认后才下载）。
    Remote(String),
    /// 配置目录下的 `oxidict-backup.zip`。
    Local,
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
                    if !v.is_empty()
                        && let Some(other) = oxidict_platform::hotkey::find_conflict(v, name)
                    {
                        crate::notify::open_notify(
                            cx,
                            t_args("config-hotkey-conflict", &[("name", &other)]),
                        );
                        return;
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

        // 备份后端：顺序固定为 WebDAV(0) / 本地目录(1)，与 `backup_kind_value` 对应。
        let backup_kind_ix = match config()
            .get_or(keys::BACKUP_TYPE, String::from(KIND_WEBDAV))
            .as_str()
        {
            KIND_LOCAL_FOLDER => 1,
            _ => 0,
        };
        let backup_kind = cx.new(|cx| {
            SelectState::new(
                vec![
                    SharedString::from(t("config-backup-type-webdav")),
                    SharedString::from(t("config-backup-type-local")),
                ],
                Some(IndexPath::new(backup_kind_ix)),
                window,
                cx,
            )
        });
        let backup_folder = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::BACKUP_FOLDER)
                    .unwrap_or_default(),
            )
        });

        // 主题三态：跟随系统 / 浅色 / 深色。变更即写回配置并应用；
        // "system" 由 `apply_saved_theme` 按当前窗口外观推导。
        // 早期是二态开关：默认值 "system" 一旦被用户碰过就永远选不回来。
        //
        // 选项文案用 `SharedString` 而不是 `&'static str`：`t()` 是运行时查表、
        // 拿不到 `'static`，而 `Vec<SharedString>` 同样实现了 SearchableListDelegate。
        let theme_ix = match config()
            .get_or(keys::APP_THEME, String::from("system"))
            .as_str()
        {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        };
        let theme_labels: Vec<SharedString> = THEME_KEYS
            .iter()
            .map(|k| SharedString::from(t(k)))
            .collect();
        let theme_select =
            cx.new(|cx| SelectState::new(theme_labels, Some(IndexPath::new(theme_ix)), window, cx));
        cx.subscribe_in(
            &theme_select,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, window, cx| {
                // SelectEvent 只有 Confirm 一个变体，模式不可拒绝，直接解构。
                let SelectEvent::Confirm(_) = event;
                let theme = match this.theme_select.read(cx).selected_index(cx).map(|p| p.row) {
                    Some(1) => "light",
                    Some(2) => "dark",
                    _ => "system",
                };
                let _ = config().set(keys::APP_THEME, &theme);
                crate::apply_saved_theme(window, cx);
            },
        )
        .detach();

        // 界面语言：列出全部 20 个 locale。配置里的语言码可能是 `zh_cn` 这类
        // 下划线写法（对齐原版），归一成 BCP47 再比较；选定后立即切换运行时
        // 语言并写回配置。
        let language_options: Vec<&'static str> = oxidict_core::i18n::LANGUAGES
            .iter()
            .map(|(_, name)| *name)
            .collect();
        let normalize = |s: &str| s.replace('_', "-").to_ascii_lowercase();
        let current = config().get_or(keys::APP_LANGUAGE, String::from("zh_cn"));
        let lang_ix = oxidict_core::i18n::LANGUAGES
            .iter()
            .position(|(code, _)| normalize(code) == normalize(&current))
            .unwrap_or(0);
        let language_select = cx.new(|cx| {
            SelectState::new(language_options, Some(IndexPath::new(lang_ix)), window, cx)
        });
        cx.subscribe_in(
            &language_select,
            window,
            |this, _, event: &SelectEvent<Vec<&'static str>>, _window, cx| {
                let SelectEvent::Confirm(_) = event;
                let Some(row) = this
                    .language_select
                    .read(cx)
                    .selected_index(cx)
                    .map(|p| p.row)
                else {
                    return;
                };
                let Some((code, _)) = oxidict_core::i18n::LANGUAGES.get(row) else {
                    return;
                };
                oxidict_core::i18n::set_language(code);
                let _ = config().set(keys::APP_LANGUAGE, &code.to_string());
                cx.notify();
            },
        )
        .detach();

        // 界面字体族：空串 / "default" 表示用系统字体。
        let font_family = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("config-font-system"))
                .default_value(config().get_or(keys::APP_FONT, String::new()))
        });
        // 只在「回车 / 失焦」时应用：字体族要改字符串，若每次输入都重建主题会很卡。
        cx.subscribe_in(
            &font_family,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let family = this.font_family.read(cx).value().trim().to_string();
                    let _ = config().set(keys::APP_FONT, &family);
                    crate::apply_saved_theme(window, cx);
                }
            },
        )
        .detach();

        // 界面字号（对应此前完全不生效的 `app_font_size`）。
        let configured_size: f32 = config().get_or(keys::APP_FONT_SIZE, crate::tokens::BASE_PX);
        let font_size = cx.new(|cx| {
            SelectState::new(
                FONT_SIZE_STEPS
                    .iter()
                    .map(|s| SharedString::from(format!("{s:.0}")))
                    .collect::<Vec<_>>(),
                Some(IndexPath::new(font_size_index(configured_size))),
                window,
                cx,
            )
        });
        cx.subscribe_in(
            &font_size,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, window, cx| {
                let SelectEvent::Confirm(_) = event;
                let Some(row) = this.font_size.read(cx).selected_index(cx).map(|p| p.row) else {
                    return;
                };
                let Some(px_value) = FONT_SIZE_STEPS.get(row) else {
                    return;
                };
                let _ = config().set(keys::APP_FONT_SIZE, px_value);
                crate::apply_saved_theme(window, cx);
                cx.notify();
            },
        )
        .detach();

        // 「跟随系统」实时生效：gpui 没有「外观变化」回调，借窗口重新激活的时机
        // 重算——用户在系统里切换深浅色后切回应用即生效，不必重启。
        cx.observe_window_activation(window, |_, window, cx| {
            if !window.is_window_active() {
                return;
            }
            if config().get_or(keys::APP_THEME, String::from("system")) != "system" {
                return;
            }
            crate::apply_saved_theme(window, cx);
        })
        .detach();

        // 自定义服务表单。文本字段全为单行输入：进阶用法（headers/fields/流式）
        // 仍写 config.json，表单只覆盖"纯映射服务"的典型形态。
        let custom_id = cx.new(|cx| InputState::new(window, cx));
        let custom_endpoint = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://api.example.com/v1/translate")
        });
        let custom_method =
            cx.new(|cx| SelectState::new(vec!["GET", "POST"], Some(IndexPath::new(1)), window, cx));
        let custom_auth = cx.new(|cx| {
            SelectState::new(
                vec![
                    SharedString::from(t("config-custom-auth-none")),
                    SharedString::from(t("config-custom-auth-bearer")),
                    SharedString::from(t("config-custom-auth-header")),
                    SharedString::from(t("config-custom-auth-query")),
                ],
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let custom_auth_name = cx.new(|cx| InputState::new(window, cx).placeholder("X-Api-Key"));
        let custom_body =
            cx.new(|cx| InputState::new(window, cx).placeholder(r#"{"q":"{{text}}"}"#));
        let custom_extract = cx.new(|cx| InputState::new(window, cx).placeholder("data.0.text"));
        // 指纹选项复用服务 schema 的同一份措辞（口径一致）。
        let custom_tls = cx.new(|cx| {
            SelectState::new(
                oxidict_services::TLS_OPTIONS
                    .iter()
                    .map(|(_, label)| SharedString::from(*label))
                    .collect::<Vec<_>>(),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        cx.subscribe_in(
            &custom_auth,
            window,
            |_, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                // 鉴权方式变了要重绘：「参数名」这一行只在后两种方式下出现。
                let SelectEvent::Confirm(_) = event;
                cx.notify();
            },
        )
        .detach();

        // 历史页搜索框：输入变化即重绘（查询在 render 时读取）。
        let history_query =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("config-history-search")));
        cx.subscribe_in(&history_query, window, |_, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();

        let mut this = Self {
            nav: 0,
            add_state: None,
            form: None,
            scroll: ScrollHandle::new(),
            hotkey_inputs,
            clipboard_monitor: config().get_or(keys::CLIPBOARD_MONITOR, false),
            theme_select,
            language_select,
            font_family,
            font_size,
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
            backup_kind,
            backup_folder,
            backup_status: None,
            remote_backups: None,
            pending_restore: None,
            history_limit: HISTORY_PAGE,
            history_query,
            custom_id,
            custom_endpoint,
            custom_method,
            custom_auth,
            custom_auth_name,
            custom_body,
            custom_extract,
            custom_tls,
            custom_status: None,
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
                            .text_size(Text::Logo.size())
                            .text_color(cx.theme().colors.primary_foreground)
                            .child(SharedString::from(logo_char)),
                    ),
            )
            .child(
                v_flex()
                    .child(
                        div()
                            .text_size(Text::Label.size())
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .child(SharedString::from(t("app-name"))),
                    )
                    .child(div().text_size(Text::Xs.size()).text_color(muted_fg).child(
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
                        .child(
                            div()
                                .text_size(Text::Label.size())
                                .child(SharedString::from(t(label))),
                        )
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
                    .text_size(Text::Xs.size())
                    .text_color(muted_fg)
                    .child(SharedString::from(t("app-tagline"))),
            );

        // ── 内容区 ──
        let page_title = if self.nav < 4 {
            // 不变量：此分支 self.nav < 4，nav_kind 对 0..4 必返回 Some。
            match nav_kind(self.nav).expect("nav < 4 时必有 ServiceKind") {
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
            8 => self.render_custom_page(window, cx),
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
                                    .text_size(Text::Heading.size())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 字号下拉的「回显」必须落到合法档位：配置里是档位值就精确命中，
    /// 是手改的中间值（如 17）则取最接近的一档，绝不能越界。
    #[test]
    fn font_size_index_maps_configured_value_to_a_valid_step() {
        assert_eq!(font_size_index(14.0), 0);
        assert_eq!(font_size_index(16.0), 1);
        assert_eq!(font_size_index(18.0), 2);
        assert_eq!(font_size_index(20.0), 3);
        // 中间值与越界值都落到最近的合法档位。
        assert_eq!(font_size_index(17.0), 1, "17 更接近 16");
        assert_eq!(font_size_index(19.4), 3, "19.4 更接近 20");
        assert_eq!(font_size_index(0.0), 0);
        assert_eq!(font_size_index(999.0), 3);
        for v in [0.0, 12.0, 16.0, 17.3, 25.0, 999.0] {
            assert!(font_size_index(v) < FONT_SIZE_STEPS.len(), "{v} 越界");
        }
    }
}
