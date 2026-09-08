//! 设置窗口。
//!
//! 对应原 `src/window/Config`：左侧栏（Logo + 图标菜单，选中高亮）+
//! 右侧内容区（页标题 + 分隔线 + 滚动区），设置行沿用原版
//! `.config-item` 的「左标签右控件」布局。
//!
//! 服务配置表单由服务的 `config_schema()` 声明式驱动——原版 30 份手写
//! Config.jsx 在这里归一成一个通用表单渲染器。

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputContentType, InputState};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable};
use gpui_kit::base::IndexPath;
use gpui_kit::{
    div, px, App, Bounds,  Context, Entity, IntoElement, InteractiveElement, ParentElement, Point,
    Render, ScrollHandle, SharedString, Size, StatefulInteractiveElement, Styled, TitlebarOptions,
    Window, WindowBounds, WindowKind, WindowOptions,
};
use saladict_core::config::{config, keys, parse_instance};
use saladict_core::i18n::{t, t_args};
use saladict_core::schema::ConfigField;
use saladict_core::{Error, ServiceKind};
use saladict_services::services;

/// 侧栏导航项：图标 + 标题（标题存 i18n key，渲染时再 `t()` 翻译）。
const NAV_ITEMS: [(IconName, &str); 7] = [
    (IconName::Globe, "config-nav-translate"),
    (IconName::Search, "config-nav-recognize"),
    (IconName::Play, "config-nav-tts"),
    (IconName::BookOpen, "config-nav-collection"),
    (IconName::Settings2, "config-nav-hotkey"),
    (IconName::Palette, "config-nav-general"),
    (IconName::HardDrive, "config-nav-backup"),
];

/// 服务分类的展示名（i18n key），按导航顺序。
const KIND_TABS: [(ServiceKind, &str); 4] = [
    (ServiceKind::Translate, "config-nav-translate"),
    (ServiceKind::Recognize, "config-nav-recognize"),
    (ServiceKind::Tts, "config-nav-tts"),
    (ServiceKind::Collection, "config-nav-collection"),
];

/// 前四项对应的服务分类。
fn nav_kind(nav: usize) -> Option<ServiceKind> {
    match nav {
        0 => Some(ServiceKind::Translate),
        1 => Some(ServiceKind::Recognize),
        2 => Some(ServiceKind::Tts),
        3 => Some(ServiceKind::Collection),
        _ => None,
    }
}

/// 添加/编辑服务的表单状态。字段实体在表单打开时创建。
struct FormState {
    kind: ServiceKind,
    service_id: String,
    schema: Vec<ConfigField>,
    instance_name: Entity<InputState>,
    text_fields: Vec<(&'static str, Entity<InputState>, bool)>,
    select_fields: Vec<(&'static str, Entity<SelectState<Vec<&'static str>>>, Vec<&'static str>)>,
    switch_fields: Vec<(&'static str, bool)>,
    number_fields: Vec<(&'static str, Entity<InputState>, f64)>,
    error: Option<String>,
}

pub struct ConfigWindow {
    nav: usize,
    /// 每个服务分类的「添加服务」下拉（切页或列表变动时重建）。
    add_state: Option<(ServiceKind, Entity<SelectState<Vec<&'static str>>>)>,
    form: Option<FormState>,
    scroll: ScrollHandle,
    // 通用设置：(i18n key, 输入框实体, 配置键名)
    hotkey_inputs: Vec<(&'static str, Entity<InputState>, &'static str)>,
    clipboard_monitor: bool,
    dark_mode: bool,
    autostart: bool,
    proxy_enable: bool,
    proxy_host: Entity<InputState>,
    proxy_port: Entity<InputState>,
    saved_hint: bool,
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
            (
                i18n_key,
                cx.new(|cx| InputState::new(window, cx).default_value(value)),
                name,
            )
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

        let webdav_url = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config()
                    .get::<String>(keys::WEBDAV_URL)
                    .unwrap_or_default(),
            )
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
            autostart: saladict_platform::autostart::is_enabled(),
            dark_mode: {
                let theme: String = config().get_or(keys::APP_THEME, "system".into());
                theme == "dark"
            },
            proxy_enable: config().get_or(keys::PROXY_ENABLE, false),
            proxy_host,
            proxy_port,
            saved_hint: false,
            webdav_url,
            webdav_username,
            webdav_password,
            backup_status: None,
        };
        this.refresh_add_state(window, cx);
        this
    }

    /// 为当前服务分类重建「添加服务」下拉。
    fn refresh_add_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 快捷键 / 通用页不对应服务分类。
        let Some(kind) = nav_kind(self.nav) else {
            self.add_state = None;
            return;
        };
        let enabled = enabled_prefixes(kind);
        let available: Vec<&'static str> = all_service_ids(kind)
            .into_iter()
            .filter(|id| !enabled.iter().any(|e| e == id))
            .collect();
        let state = cx.new(|cx| SelectState::new(available, Some(IndexPath::new(0)), window, cx));
        self.add_state = Some((kind, state));
        cx.notify();
    }

    fn open_form(&mut self, service_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = nav_kind(self.nav) else { return };
        let schema: Vec<ConfigField> = schema_of(kind, &service_id);
        if schema.is_empty() {
            // 免配置服务：直接加入列表。
            self.append_instance(kind, &service_id, service_id.clone());
            self.refresh_add_state(window, cx);
            return;
        }

        let instance_name =
            cx.new(|cx| InputState::new(window, cx).default_value(service_id.clone()));
        let mut text_fields = Vec::new();
        let mut select_fields = Vec::new();
        let mut switch_fields = Vec::new();
        let mut number_fields = Vec::new();

        for field in &schema {
            match field {
                ConfigField::Text { key, placeholder, secret, .. } => {
                    let state = cx.new(|cx| InputState::new(window, cx).placeholder(*placeholder));
                    text_fields.push((*key, state, *secret));
                }
                ConfigField::Select { key, options, default, .. } => {
                    let key = *key;
                    let labels: Vec<&'static str> = options.iter().map(|(_, l)| *l).collect();
                    let ix = options.iter().position(|(v, _)| v == default).unwrap_or(0);
                    let state =
                        cx.new(|cx| SelectState::new(labels, Some(IndexPath::new(ix)), window, cx));
                    let values: Vec<&'static str> = options.iter().map(|(v, _)| *v).collect();
                    select_fields.push((key, state, values));
                }
                ConfigField::Switch { key, default, .. } => {
                    switch_fields.push((*key, *default));
                }
                ConfigField::Number { key, default, .. } => {
                    let state =
                        cx.new(|cx| InputState::new(window, cx).default_value(default.to_string()));
                    number_fields.push((*key, state, *default));
                }
            }
        }

        self.form = Some(FormState {
            kind,
            service_id,
            schema,
            instance_name,
            text_fields,
            select_fields,
            switch_fields,
            number_fields,
            error: None,
        });
        cx.notify();
    }

    fn append_instance(&mut self, kind: ServiceKind, base_id: &str, display: String) {
        let store = config();
        let list_key = kind.list_key();
        let mut list = store.service_list(list_key);
        // 实例 key：首次用服务 id；重复添加则加随机后缀（与原 `id@随机` 约定一致）。
        let key = if list.iter().any(|k| parse_instance(k).0 == base_id) {
            let suffix: String = saladict_net::uuid_v4().chars().take(8).collect();
            format!("{}@{}", base_id, suffix)
        } else {
            base_id.to_string()
        };

        let mut cfg = saladict_core::ServiceConfig::new();
        cfg.insert(
            saladict_services::INSTANCE_NAME_KEY.to_string(),
            serde_json::json!(display),
        );
        let _ = store.set_instance_config(&key, cfg);
        list.push(key);
        let _ = store.set_service_list(list_key, &list);
    }

    fn remove_instance(&mut self, kind: ServiceKind, key: &str) {
        let store = config();
        let list_key = kind.list_key();
        let mut list = store.service_list(list_key);
        list.retain(|k| k != key);
        let _ = store.set_service_list(list_key, &list);
    }

    /// 上移 / 下移服务实例（`delta` 为 -1 或 +1）。
    fn move_instance(&mut self, kind: ServiceKind, index: usize, delta: i64) {
        let store = config();
        let list_key = kind.list_key();
        let mut list = store.service_list(list_key);
        let target = index as i64 + delta;
        if target < 0 || target >= list.len() as i64 {
            return;
        }
        list.swap(index, target as usize);
        let _ = store.set_service_list(list_key, &list);
    }

    fn save_form(&mut self, _: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut form) = self.form.take() else { return };

        // 校验 required 文本字段。
        for (key, ent, _) in &form.text_fields {
            let value = ent.read(cx).value().trim().to_string();
            let required = form
                .schema
                .iter()
                .any(|f| f.key() == *key && matches!(f, ConfigField::Text { required: true, .. }));
            if required && value.is_empty() {
                let label = field_label(&form.schema, key);
                form.error = Some(t_args("config-field-required", &[("field", label.as_str())]));
                self.form = Some(form);
                cx.notify();
                return;
            }
        }

        let display = form.instance_name.read(cx).value().trim().to_string();
        let display = if display.is_empty() { form.service_id.clone() } else { display };

        let store = config();
        let list_key = form.kind.list_key();
        let mut list = store.service_list(list_key);
        let key = if list.iter().any(|k| parse_instance(k).0 == form.service_id) {
            let suffix: String = saladict_net::uuid_v4().chars().take(8).collect();
            format!("{}@{}", form.service_id, suffix)
        } else {
            form.service_id.clone()
        };

        let mut cfg = saladict_core::ServiceConfig::new();
        cfg.insert(
            saladict_services::INSTANCE_NAME_KEY.to_string(),
            serde_json::json!(display),
        );
        for (key, ent, _) in &form.text_fields {
            cfg.insert(key.to_string(), serde_json::json!(ent.read(cx).value()));
        }
        for (key, state, values) in &form.select_fields {
            let selected = state
                .read(cx)
                .selected_value()
                .copied()
                .unwrap_or("")
                .to_string();
            // 展示名映射回值。
            let value = form
                .schema
                .iter()
                .find_map(|f| match f {
                    ConfigField::Select { key: k, options, .. } if k == key => options
                        .iter()
                        .find(|(_, label)| *label == selected)
                        .map(|(v, _)| v.to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| values.first().copied().unwrap_or_default().to_string());
            cfg.insert(key.to_string(), serde_json::json!(value));
        }
        for (key, value) in &form.switch_fields {
            cfg.insert(key.to_string(), serde_json::json!(value));
        }
        for (key, ent, default) in &form.number_fields {
            let n = ent.read(cx).value().trim().parse::<f64>().unwrap_or(*default);
            cfg.insert(key.to_string(), serde_json::json!(n));
        }
        let _ = store.set_instance_config(&key, cfg);
        list.push(key);
        let _ = store.set_service_list(list_key, &list);

        self.refresh_add_state(window, cx);
        cx.notify();
    }

    fn save_general(&mut self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let store = config();
        for (_key, ent, name) in &self.hotkey_inputs {
            let full = format!("{}{}", keys::HOTKEY_PREFIX, name);
            let _ = store.set(&full, &ent.read(cx).value().to_string());
        }
        let _ = store.set(keys::CLIPBOARD_MONITOR, &self.clipboard_monitor);
        let _ = store.set(keys::PROXY_ENABLE, &self.proxy_enable);
        let _ = store.set(keys::PROXY_HOST, &self.proxy_host.read(cx).value().to_string());
        let port = self
            .proxy_port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .unwrap_or(60606);
        let _ = store.set(keys::PROXY_PORT, &port);
        // 代理变化需要重建 HTTP 客户端。
        saladict_net::rebuild_client();
        self.saved_hint = true;
        cx.notify();
    }
}

use gpui_kit::AppContext as _;
use gpui_kit::prelude::FluentBuilder as _;

/// 某分类下已启用实例的服务名前缀集合。
fn enabled_prefixes(kind: ServiceKind) -> Vec<String> {
    config()
        .service_list(kind.list_key())
        .iter()
        .map(|k| parse_instance(k).0.to_string())
        .collect()
}

/// 注册表里该分类的全部服务 id。
///
/// 服务 id 是注册表里的静态字符串，直接泄漏成 'static（每次刷新最多几十个，
/// 量级有界），换取能直接喂给 `SelectState<Vec<&'static str>>`。
fn all_service_ids(kind: ServiceKind) -> Vec<&'static str> {
    let ids: Vec<String> = match kind {
        ServiceKind::Translate => services().translators().iter().map(|t| t.id().to_string()).collect(),
        ServiceKind::Recognize => services().recognizers().iter().map(|t| t.id().to_string()).collect(),
        ServiceKind::Tts => services().tts_list().iter().map(|t| t.id().to_string()).collect(),
        ServiceKind::Collection => services().collectors().iter().map(|t| t.id().to_string()).collect(),
    };
    ids.into_iter()
        .map(|id| Box::leak(id.into_boxed_str()) as &'static str)
        .collect()
}

fn schema_of(kind: ServiceKind, id: &str) -> Vec<ConfigField> {
    let s = services();
    match kind {
        ServiceKind::Translate => s.translator(id).map(|t| t.config_schema()),
        ServiceKind::Recognize => s.recognizer(id).map(|t| t.config_schema()),
        ServiceKind::Tts => s.tts(id).map(|t| t.config_schema()),
        ServiceKind::Collection => s.collector(id).map(|t| t.config_schema()),
    }
    .unwrap_or_default()
}

/// 从 schema 里取字段显示名。
fn field_label(schema: &[ConfigField], key: &str) -> String {
    schema
        .iter()
        .find(|f| f.key() == key)
        .map(|f| f.label().to_string())
        .unwrap_or_else(|| t("config-field-unknown"))
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------

fn placeholder_text(text: &str) -> gpui_kit::AnyElement {
    div()
        .text_size(px(12.))
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

/// 标签在上、输入在下的字段布局（用于配置表单）。
fn labeled_field(label: &str, state: &Entity<InputState>, secret: bool) -> gpui_kit::AnyElement {
    let mut input = Input::new(state);
    if secret {
        input = input.content_type(InputContentType::Password);
    }
    v_flex()
        .gap_1()
        .child(
            div()
                .text_size(px(12.))
                .child(SharedString::from(label.to_string())),
        )
        .child(input.w_full())
        .into_any_element()
}

fn field_label_div(label: &str, color: gpui_kit::Hsla) -> gpui_kit::AnyElement {
    div()
        .text_size(px(12.))
        .text_color(color)
        .child(SharedString::from(label.to_string()))
        .into_any_element()
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

        // ── 侧栏 ──
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
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(muted_fg)
                            .child(SharedString::from(format!("v{}", env!("CARGO_PKG_VERSION")))),
                    ),
            );

        let menu = v_flex().gap_1().children(NAV_ITEMS.iter().enumerate().map(
            |(ix, (icon, label))| {
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
            },
        ));

        let sidebar = v_flex()
            .w(px(190.))
            .h_full()
            .bg(sidebar_bg)
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
                    .bg(bg)
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

impl ConfigWindow {
    fn render_service_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (t.colors.foreground, t.colors.muted_foreground, t.colors.border)
        };
        let (kind, title) = KIND_TABS[self.nav];
        let store = config();
        let list = store.service_list(kind.list_key());

        let mut page = v_flex().px_4().py_3().gap_2();

        page = page.child(
            div()
                .text_size(px(12.))
                .text_color(muted_fg)
                .child(SharedString::from(t_args(
                    "config-instances-count",
                    &[("title", t(title).as_str()), ("count", &list.len().to_string())],
                ))),
        );

        if list.is_empty() {
            page = page.child(placeholder_text(t("config-no-instances").as_str()));
        }
        for (list_ix, key) in list.iter().enumerate() {
            let key = key.clone();
            let name = saladict_services::instance_display_name(&key, &store);
            let (sid, _) = parse_instance(&key);
            page = page.child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div().text_size(px(13.)).text_color(fg).child(SharedString::from(name)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted_fg)
                                    .child(SharedString::from(sid.to_string())),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(("up", list_ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ArrowUp)
                                    .tooltip(t("config-move-up").as_str())
                                    .disabled(list_ix == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_instance(kind, list_ix, -1);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(("down", list_ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ArrowDown)
                                    .tooltip(t("config-move-down").as_str())
                                    .disabled(list_ix == list.len() - 1)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_instance(kind, list_ix, 1);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(("del", list_ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Delete)
                                    .tooltip(t("config-remove").as_str())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_instance(kind, &key);
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }

        // 添加服务
        if let Some((_, state)) = &self.add_state {
            page = page.child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .child(placeholder_text(t("config-add-instance").as_str()))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Select::new(state).small().w(px(220.)))
                            .child(
                                Button::new("add")
                                    .primary()
                                    .small()
                                    .label(t("config-add").as_str())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let Some((_, state)) = &this.add_state else { return };
                                        let Some(id) =
                                            state.read(cx).selected_value().map(|v| v.to_string())
                                        else {
                                            return;
                                        };
                                        this.open_form(id, window, cx);
                                    })),
                            ),
                    ),
            );
        }

        // 配置表单
        if let Some(form) = &self.form {
            let mut form_card = v_flex()
                .w_full()
                .mt_2()
                .border_1()
                .border_color(border)
                .rounded_lg()
                .p_3()
                .gap_3()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(fg)
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .child(SharedString::from(t_args(
                        "config-configure",
                        &[("service", form.service_id.as_str())],
                    ))),
            );

            form_card = form_card.child(labeled_field(
                t("config-instance-name").as_str(),
                &form.instance_name,
                false,
            ));

            for (key, ent, secret) in &form.text_fields {
                let label = field_label(&form.schema, key);
                form_card = form_card.child(labeled_field(&label, ent, *secret));
            }
            for (key, state, _) in &form.select_fields {
                let label = field_label(&form.schema, key);
                form_card = form_card.child(
                    v_flex()
                        .gap_1()
                        .child(field_label_div(&label, muted_fg))
                        .child(Select::new(state).small()),
                );
            }
            for (sw_ix, (key, value)) in form.switch_fields.iter().enumerate() {
                let label = field_label(&form.schema, key);
                // Switch 是无状态组件：on_click 只给 (checked, window, cx)，
                // 回调里经 WeakEntity 回写自身状态。
                let weak = cx.entity().downgrade();
                let sw_key = *key;
                form_card = form_card.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Switch::new(("sw", sw_ix))
                                .checked(*value)
                                .on_click(move |checked: &bool, _, cx| {
                                    if let Some(this) = weak.upgrade() {
                                        this.update(cx, |this, cx| {
                                            if let Some(form) = &mut this.form {
                                                for (k, v) in form.switch_fields.iter_mut() {
                                                    if *k == sw_key {
                                                        *v = *checked;
                                                    }
                                                }
                                            }
                                            cx.notify();
                                        });
                                    }
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(fg)
                                .child(SharedString::from(label)),
                        ),
                );
            }
            for (key, ent, _) in &form.number_fields {
                let label = field_label(&form.schema, key);
                form_card = form_card.child(labeled_field(&label, ent, false));
            }

            if let Some(err) = &form.error {
                form_card = form_card.child(
                    div()
                        .text_size(px(12.))
                        .text_color(cx.theme().colors.danger)
                        .child(SharedString::from(err.clone())),
                );
            }

            form_card = form_card.child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-form")
                            .ghost()
                            .small()
                            .label(t("common-cancel").as_str())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.form = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("save-form")
                            .primary()
                            .small()
                            .label(t("common-save").as_str())
                            .on_click(cx.listener(Self::save_form)),
                    ),
            );

            page = page.child(form_card);
        }

        page.into_any_element()
    }

    fn render_hotkey_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (t.colors.foreground, t.colors.muted_foreground, t.colors.border)
        };
        let mut page = v_flex().px_4().py_3().gap_2();

        page = page.child(
            div()
                .text_size(px(12.))
                .text_color(muted_fg)
                .child(SharedString::from(t("config-hotkey-hint"))),
        );

        for (label, ent, _name) in &self.hotkey_inputs {
            page = page.child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(fg)
                            .child(SharedString::from(t(label))),
                    )
                    .child(Input::new(ent).w(px(200.)).small()),
            );
        }

        page.into_any_element()
    }

    fn render_general_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, border) = {
            let t = cx.theme();
            (t.colors.foreground, t.colors.border)
        };
        let mut page = v_flex().px_4().py_3().gap_2();

        page = page.child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .py_2()
                .border_b_1()
                .border_color(border)
                .child(div().text_size(px(13.)).text_color(fg).child(SharedString::from(t("config-clipboard-monitor"))))
                .child(
                    Switch::new("clipboard-monitor")
                        .checked(self.clipboard_monitor)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.clipboard_monitor = *checked;
                            cx.notify();
                        })),
                ),
        );

        // 深色模式
        {
            let weak = cx.entity().downgrade();
            page = page.child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(div().text_size(px(13.)).text_color(fg).child(SharedString::from(t("config-dark-mode"))))
                    .child(
                        Switch::new("dark-mode")
                            .checked(self.dark_mode)
                            .on_click(move |checked: &bool, window, cx| {
                                // 实时切换主题。
                                let mode = if *checked {
                                    gpui_kit::component::ThemeMode::Dark
                                } else {
                                    gpui_kit::component::ThemeMode::Light
                                };
                                gpui_kit::component::Theme::change(mode, Some(window), cx);
                                if let Some(this) = weak.upgrade() {
                                    this.update(cx, |this, cx| {
                                        this.dark_mode = *checked;
                                        let _ = config().set(
                                            keys::APP_THEME,
                                            &if *checked { "dark" } else { "light" },
                                        );
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
            );
        }

        page = page.child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .py_2()
                .border_b_1()
                .border_color(border)
                .child(div().text_size(px(13.)).text_color(fg).child(SharedString::from(t("config-proxy"))))
                .child(
                    Switch::new("proxy-enable")
                        .checked(self.proxy_enable)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.proxy_enable = *checked;
                            cx.notify();
                        })),
                ),
        );

        if self.proxy_enable {
            page = page
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .py_2()
                        .border_b_1()
                        .border_color(border)
                        .child(div().text_size(px(13.)).text_color(fg).child(SharedString::from(t("config-proxy-host"))))
                        .child(Input::new(&self.proxy_host).w(px(200.)).small()),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .py_2()
                        .border_b_1()
                        .border_color(border)
                        .child(div().text_size(px(13.)).text_color(fg).child(SharedString::from(t("config-proxy-port"))))
                        .child(Input::new(&self.proxy_port).w(px(200.)).small()),
                );
        }

        if self.saved_hint {
            page = page.child(
                div()
                    .text_size(px(12.))
                .text_color(cx.theme().colors.success)
                .child(SharedString::from(t("config-saved-hint"))),
            );
        }

        page = page.child(
            h_flex().justify_end().child(
                Button::new("save-general")
                    .primary()
                    .small()
                    .label(t("config-save-settings").as_str())
                    .on_click(cx.listener(Self::save_general)),
            ),
        );

        page.into_any_element()
    }

    fn render_backup_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (t.colors.foreground, t.colors.muted_foreground, t.colors.border)
        };

        // 先保存 WebDAV 凭据到 config。
        let url = self.webdav_url.read(cx).value().trim().to_string();
        let username = self.webdav_username.read(cx).value().trim().to_string();
        let password = self.webdav_password.read(cx).value().trim().to_string();

        let mut page = v_flex().px_4().py_3().gap_2();

        page = page.child(
            div()
                .text_size(px(12.))
                .text_color(muted_fg)
                .child("WebDAV 云端备份（填入服务器地址与凭据后可上传/恢复）"),
        );

        page = page.child(labeled_field("服务器地址", &self.webdav_url, false));
        page = page.child(labeled_field("用户名", &self.webdav_username, false));
        page = page.child(labeled_field("密码", &self.webdav_password, true));

        if let Some(status) = &self.backup_status {
            page = page.child(
                div()
                    .text_size(px(12.))
                    .text_color(fg)
                    .child(SharedString::from(status.clone())),
            );
        }

        page = page.child(
            h_flex()
                .gap_2()
                .child({
                    let url = url.clone();
                    let username = username.clone();
                    let password = password.clone();
                    Button::new("webdav-put")
                        .primary()
                        .small()
                        .label("备份到云端")
                        .disabled(url.is_empty() || username.is_empty())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.do_webdav_backup(&url, &username, &password, window, cx);
                        }))
                })
                .child({
                    let url = url.clone();
                    let username = username.clone();
                    let password = password.clone();
                    Button::new("webdav-get")
                        .secondary()
                        .small()
                        .label("从云端恢复")
                        .disabled(url.is_empty() || username.is_empty())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.do_webdav_restore(&url, &username, &password, window, cx);
                        }))
                }),
        );

        // ── 本地导出/导入 ──
        page = page.child(
            div()
                .text_size(px(12.))
                .text_color(muted_fg)
                .child("本地备份（保存为 zip 文件或从备份文件恢复）"),
        );

        page = page.child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("local-export")
                        .secondary()
                        .small()
                        .label("导出到文件")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.do_local_export(window, cx);
                        })),
                )
                .child(
                    Button::new("local-import")
                        .secondary()
                        .small()
                        .label("从文件导入")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.do_local_import(window, cx);
                        })),
                ),
        );

        page.into_any_element()
    }

    /// 保存 WebDAV 凭据到 config 并执行云端备份。
    fn do_webdav_backup(
        &mut self,
        url: &str,
        username: &str,
        password: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let store = config();
        let _ = store.set(keys::WEBDAV_URL, &url);
        let _ = store.set(keys::WEBDAV_USERNAME, &username);
        let _ = store.set(keys::WEBDAV_PASSWORD, &password);

        let cfg = saladict_net::webdav::WebDavConfig {
            url: url.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        };
        let name = format!("saladict-backup-{}.zip", saladict_net::iso_utc_now().replace(':', ""));
        let handle = saladict_core::runtime::spawn(async move {
            saladict_net::webdav::put(&cfg, &name, saladict_core::backup::pack()?).await?;
            Ok::<_, Error>(format!("已上传到云端: {name}"))
        });
        cx.spawn(async move |this, cx| {
            let status = match handle.await {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => format!("备份失败: {e}"),
                Err(e) => format!("任务取消: {e}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.backup_status = Some(status);
                cx.notify();
            });
        })
        .detach();
    }

    /// 从 WebDAV 下载并恢复备份。
    fn do_webdav_restore(
        &mut self,
        url: &str,
        username: &str,
        password: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cfg = saladict_net::webdav::WebDavConfig {
            url: url.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        };
        // 用当前配置里最近一次备份名（简化：取列表最后一个）。
        let handle = saladict_core::runtime::spawn(async move {
            let files = saladict_net::webdav::list(&cfg).await?;
            let name = files.last().ok_or_else(|| {
                Error::Service("云端没有备份文件".into())
            })?.clone();
            let data = saladict_net::webdav::get(&cfg, &name).await?;
            saladict_core::backup::unpack(&data)?;
            Ok::<_, Error>(format!("已从云端恢复: {name}"))
        });
        cx.spawn(async move |this, cx| {
            let status = match handle.await {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => format!("恢复失败: {e}"),
                Err(e) => format!("任务取消: {e}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.backup_status = Some(status);
                cx.notify();
            });
        })
        .detach();
    }

    /// 导出到本地 zip 文件。
    fn do_local_export(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match saladict_core::backup::export_to_file(
            config().app_dir().join("saladict-backup.zip").to_str().unwrap_or("/tmp/saladict-backup.zip"),
        ) {
            Ok(()) => self.backup_status = Some("已导出到配置目录 saladict-backup.zip".into()),
            Err(e) => self.backup_status = Some(format!("导出失败: {e}")),
        }
        cx.notify();
    }

    /// 从本地 zip 文件导入。
    fn do_local_import(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match saladict_core::backup::import_from_file(
            config().app_dir().join("saladict-backup.zip").to_str().unwrap_or("/tmp/saladict-backup.zip"),
        ) {
            Ok(()) => self.backup_status = Some("已从配置目录 saladict-backup.zip 恢复".into()),
            Err(e) => self.backup_status = Some(format!("导入失败: {e}")),
        }
        cx.notify();
    }
}

/// 打开设置窗口（820x560 居中）。
pub fn open_config(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point { x: px(310.0), y: px(140.0) },
            size: Size { width: px(820.0), height: px(560.0) },
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(t("config-title").into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: false,
        show: true,
        kind: WindowKind::Normal,
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| ConfigWindow::new(window, cx));
        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
    })
    .expect("打开设置窗口失败");
}
