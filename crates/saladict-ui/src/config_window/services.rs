//! 服务实例管理：实例列表（增/删/移）、声明式配置表单与服务分类页渲染。
//!
//! 「实例列表 + 添加下拉 + schema 驱动的表单」是四个服务分类页共用的
//! 唯一实现；表单结构来自服务的 `config_schema()`，不再逐服务手写。

use super::pages::{labeled_field, placeholder_text};
use super::{ConfigWindow, KIND_TABS, nav_kind};
use gpui_kit::AppContext as _;
use gpui_kit::base::{IndexPath, h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, IconName, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div, img, px,
};
use saladict_core::ServiceKind;
use saladict_core::config::{config, parse_instance};
use saladict_core::i18n::{t, t_args};
use saladict_core::schema::ConfigField;
use saladict_services::services;

/// 下拉字段：`(配置键, 选择状态实体, 可选项的值列表)`。
type SelectField = (
    &'static str,
    Entity<SelectState<Vec<&'static str>>>,
    Vec<&'static str>,
);

/// 添加/编辑服务的表单状态。字段实体在表单打开时创建。
pub(super) struct FormState {
    pub(super) kind: ServiceKind,
    pub(super) service_id: String,
    pub(super) schema: Vec<ConfigField>,
    pub(super) instance_name: Entity<InputState>,
    pub(super) text_fields: Vec<(&'static str, Entity<InputState>, bool)>,
    pub(super) select_fields: Vec<SelectField>,
    pub(super) switch_fields: Vec<(&'static str, bool)>,
    pub(super) number_fields: Vec<(&'static str, Entity<InputState>, f64)>,
    pub(super) error: Option<String>,
}

impl ConfigWindow {
    /// 为当前服务分类重建「添加服务」下拉。
    pub(super) fn refresh_add_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let Some(kind) = nav_kind(self.nav) else {
            return;
        };
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
                ConfigField::Text {
                    key,
                    placeholder,
                    secret,
                    ..
                } => {
                    let state = cx.new(|cx| InputState::new(window, cx).placeholder(*placeholder));
                    text_fields.push((*key, state, *secret));
                }
                ConfigField::Select {
                    key,
                    options,
                    default,
                    ..
                } => {
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
        let Some(mut form) = self.form.take() else {
            return;
        };

        // 校验 required 文本字段。
        for (key, ent, _) in &form.text_fields {
            let value = ent.read(cx).value().trim().to_string();
            let required = form
                .schema
                .iter()
                .any(|f| f.key() == *key && matches!(f, ConfigField::Text { required: true, .. }));
            if required && value.is_empty() {
                let label = field_label(&form.schema, key);
                form.error = Some(t_args(
                    "config-field-required",
                    &[("field", label.as_str())],
                ));
                self.form = Some(form);
                cx.notify();
                return;
            }
        }

        let display = form.instance_name.read(cx).value().trim().to_string();
        let display = if display.is_empty() {
            form.service_id.clone()
        } else {
            display
        };

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
                    ConfigField::Select {
                        key: k, options, ..
                    } if k == key => options
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
            let n = ent
                .read(cx)
                .value()
                .trim()
                .parse::<f64>()
                .unwrap_or(*default);
            cfg.insert(key.to_string(), serde_json::json!(n));
        }
        let _ = store.set_instance_config(&key, cfg);
        list.push(key);
        let _ = store.set_service_list(list_key, &list);

        self.refresh_add_state(window, cx);
        cx.notify();
    }
}

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
        ServiceKind::Translate => services()
            .translators()
            .iter()
            .map(|t| t.id().to_string())
            .collect(),
        ServiceKind::Recognize => services()
            .recognizers()
            .iter()
            .map(|t| t.id().to_string())
            .collect(),
        ServiceKind::Tts => services()
            .tts_list()
            .iter()
            .map(|t| t.id().to_string())
            .collect(),
        ServiceKind::Collection => services()
            .collectors()
            .iter()
            .map(|t| t.id().to_string())
            .collect(),
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

fn field_label_div(label: &str, color: gpui_kit::Hsla) -> gpui_kit::AnyElement {
    div()
        .text_size(px(12.))
        .text_color(color)
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

impl ConfigWindow {
    pub(super) fn render_service_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (
                t.colors.foreground,
                t.colors.muted_foreground,
                t.colors.border,
            )
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
                    &[
                        ("title", t(title).as_str()),
                        ("count", &list.len().to_string()),
                    ],
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
                            .when_some(crate::logos::logo_uri(sid), |this, uri| {
                                this.child(
                                    img(uri).size(px(16.)).rounded_sm().flex_shrink_0(), // 默认 ObjectFit::Contain：等比缩放不裁切。
                                )
                            })
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(fg)
                                    .child(SharedString::from(name)),
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
                                        let Some((_, state)) = &this.add_state else {
                                            return;
                                        };
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
                        .child(Switch::new(("sw", sw_ix)).checked(*value).on_click(
                            move |checked: &bool, _, cx| {
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
                            },
                        ))
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
}
