//! 非服务类设置页：快捷键、通用（外观/代理/高级）、历史、备份。

use super::{ConfigWindow, HISTORY_PAGE, PendingRestore};
use crate::hotkey::HotkeyRecorder;
use crate::tokens::Text;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputContentType, InputState};
use gpui_kit::component::select::Select;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, IconName, Sizable};
use gpui_kit::{
    ClipboardItem, Context, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, Window, div, px,
};
use oxidict_core::Error;
use oxidict_core::config::{config, keys};
use oxidict_core::i18n::{t, t_args};
use oxidict_net::backup_backend::{BackupBackend, KIND_LOCAL_FOLDER, KIND_WEBDAV};
use oxidict_net::webdav::WebDavConfig;
use oxidict_services::translate::custom::{self as custom_svc, FormValues};

/// 备份后端下拉的顺序：0=WebDAV，1=本地目录。
///
/// 下标与 `BACKUP_TYPE` 取值的映射只此一处；未知下标回退 WebDAV（默认值）。
fn kind_at(ix: Option<usize>) -> &'static str {
    match ix {
        Some(1) => KIND_LOCAL_FOLDER,
        _ => KIND_WEBDAV,
    }
}

/// 多行文本单行化并截断（历史列表预览用）。
fn one_line(text: &str, max_chars: usize) -> String {
    let flat = text.replace(['\n', '\r'], " ").trim().to_string();
    if flat.chars().count() > max_chars {
        let mut s: String = flat.chars().take(max_chars).collect();
        s.push('…');
        s
    } else {
        flat
    }
}

pub(super) fn placeholder_text(text: &str) -> gpui_kit::AnyElement {
    div()
        .text_size(Text::Body.size())
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

/// 标签在上、输入在下的字段布局（用于配置表单）。
pub(super) fn labeled_field(
    label: &str,
    state: &Entity<InputState>,
    secret: bool,
) -> gpui_kit::AnyElement {
    let mut input = Input::new(state);
    if secret {
        input = input.content_type(InputContentType::Password);
    }
    v_flex()
        .gap_1()
        .child(
            div()
                .text_size(Text::Body.size())
                .child(SharedString::from(label.to_string())),
        )
        .child(input.w_full())
        .into_any_element()
}

impl ConfigWindow {
    fn save_general(&mut self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let store = config();
        for (_key, ent, name) in &self.hotkey_inputs {
            let full = format!("{}{}", keys::HOTKEY_PREFIX, name);
            let _ = store.set(&full, &ent.read(cx).current_value().to_string());
        }
        let _ = store.set(keys::CLIPBOARD_MONITOR, &self.clipboard_monitor);
        let _ = store.set(keys::PROXY_ENABLE, &self.proxy_enable);
        let _ = store.set(
            keys::PROXY_HOST,
            &self.proxy_host.read(cx).value().to_string(),
        );
        let port = self
            .proxy_port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            // 回退值必须与 `PROXY_PORT` 的默认值一致（1087）。早期这里写成
            // 60606（那是服务端口），非法输入会把代理端口静默改成无关值。
            .unwrap_or_else(|_| config().get_or(keys::PROXY_PORT, 1087));
        let _ = store.set(keys::PROXY_PORT, &port);
        // 代理认证与排除列表。
        let _ = store.set(
            keys::PROXY_USERNAME,
            &self.proxy_username.read(cx).value().trim().to_string(),
        );
        let _ = store.set(
            keys::PROXY_PASSWORD,
            &self.proxy_password.read(cx).value().to_string(),
        );
        let _ = store.set(
            keys::NO_PROXY,
            &self.no_proxy.read(cx).value().trim().to_string(),
        );
        // 高级：服务端口（非法输入回退默认值）与开发者模式。
        let server_port = self
            .server_port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .unwrap_or(60606);
        let _ = store.set(keys::SERVER_PORT, &server_port);
        let _ = store.set(keys::DEV_MODE, &self.dev_mode);
        // 代理变化需要重建 HTTP 客户端。
        oxidict_net::rebuild_client();
        self.saved_hint = true;
        cx.notify();
    }

    pub(super) fn render_hotkey_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (
                t.colors.foreground,
                t.colors.muted_foreground,
                t.colors.border,
            )
        };
        let mut page = v_flex().px_4().py_3().gap_2();

        page = page.child(
            div()
                .text_size(Text::Body.size())
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
                            .text_size(Text::Label.size())
                            .text_color(fg)
                            .child(SharedString::from(t(label))),
                    )
                    .child(HotkeyRecorder::new(ent).w(px(200.)).small()),
            );
        }

        page.into_any_element()
    }

    pub(super) fn render_general_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
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
                .child(
                    div()
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-clipboard-monitor"))),
                )
                .child(
                    Switch::new("clipboard-monitor")
                        .checked(self.clipboard_monitor)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.clipboard_monitor = *checked;
                            cx.notify();
                        })),
                ),
        );

        // 主题：跟随系统 / 浅色 / 深色。
        //
        // 早期是二态开关，默认值 "system"（跟随系统）一旦被用户碰过就永远选不回来；
        // 现在直接把三个选项都摆出来。变更即生效：写回配置后走
        // `apply_saved_theme`（"system" 按当前窗口外观推导）。
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
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-theme"))),
                )
                .child(Select::new(&self.theme_select).small()),
        );

        // 界面语言：切换即时生效（Fluent 运行时切 locale，界面重绘即翻译）。
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
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-language"))),
                )
                .child(Select::new(&self.language_select).small()),
        );

        // 界面字体族：留空即系统字体；回车 / 失焦时生效（见 ConfigWindow::new 的订阅）。
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
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-font"))),
                )
                .child(
                    div()
                        .w(px(180.))
                        .child(Input::new(&self.font_family).small()),
                ),
        );

        // 界面字号：对应此前完全不生效的 app_font_size。
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
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-font-size"))),
                )
                .child(Select::new(&self.font_size).small()),
        );

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
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-proxy"))),
                )
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
                        .child(
                            div()
                                .text_size(Text::Label.size())
                                .text_color(fg)
                                .child(SharedString::from(t("config-proxy-host"))),
                        )
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
                        .child(
                            div()
                                .text_size(Text::Label.size())
                                .text_color(fg)
                                .child(SharedString::from(t("config-proxy-port"))),
                        )
                        .child(Input::new(&self.proxy_port).w(px(200.)).small()),
                );
            // 代理认证（可选）：留空 = 匿名代理。
            page = page
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .py_2()
                        .child(
                            div()
                                .text_size(Text::Label.size())
                                .text_color(fg)
                                .child(SharedString::from(t("config-proxy-username"))),
                        )
                        .child(Input::new(&self.proxy_username).w(px(200.)).small()),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .py_2()
                        .child(
                            div()
                                .text_size(Text::Label.size())
                                .text_color(fg)
                                .child(SharedString::from(t("config-proxy-password"))),
                        )
                        .child(Input::new(&self.proxy_password).w(px(200.)).small()),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .py_2()
                        .child(
                            div()
                                .text_size(Text::Label.size())
                                .text_color(fg)
                                .child(SharedString::from(t("config-no-proxy"))),
                        )
                        .child(Input::new(&self.no_proxy).w(px(280.)).small()),
                );
        }

        // 外观与系统集成：毛玻璃（新开窗口生效）+ 隐藏 Dock 图标（仅 macOS）。
        page = page
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(Text::Label.size())
                            .text_color(fg)
                            .child(SharedString::from(t("config-transparent"))),
                    )
                    .child(
                        Switch::new("transparent")
                            .checked(self.transparent)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.transparent = *checked;
                                let _ = config().set(keys::TRANSPARENT, checked);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(Text::Label.size())
                            .text_color(fg)
                            .child(SharedString::from(t("config-dock-icon"))),
                    )
                    .child(
                        Switch::new("hide-dock-icon")
                            .checked(self.hide_dock)
                            // Dock 是 macOS 概念，其它平台开关置灰。
                            .disabled(!cfg!(target_os = "macos"))
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.hide_dock = *checked;
                                let _ = config().set(keys::HIDE_DOCK_ICON, checked);
                                // 立即生效：Accessory/Regular 策略切换。
                                oxidict_platform::dock::set_visible(!checked);
                                cx.notify();
                            })),
                    ),
            );

        // 高级：HTTP 服务端口 / 开发者模式（对齐原版 Advance 页；端口改动
        // 重启生效——tiny_http 端口只在启动时读取，与原版一致）。
        page = page
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(Text::Label.size())
                            .text_color(fg)
                            .child(SharedString::from(t("config-server-port"))),
                    )
                    .child(Input::new(&self.server_port).w(px(120.)).small()),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(Text::Label.size())
                            .text_color(fg)
                            .child(SharedString::from(t("config-dev-mode"))),
                    )
                    .child(
                        Switch::new("dev-mode")
                            .checked(self.dev_mode)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.dev_mode = *checked;
                                cx.notify();
                            })),
                    ),
            );

        if self.saved_hint {
            page = page.child(
                div()
                    .text_size(Text::Body.size())
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

    /// 历史管理页（对齐原版 History 页）：时间倒序列表 + 单条删除 + 清空。
    /// 数据在渲染时直读 SQLite（百行量级，免维护镜像状态）。
    pub(super) fn render_history_page(&mut self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (
                t.colors.foreground,
                t.colors.muted_foreground,
                t.colors.border,
            )
        };
        let history = oxidict_core::history::History::global();
        // 有查询词时走 core 的 LIKE 检索（匹配原文或译文），否则列出最近 100 条。
        // 检索能力 core 一直有，只是历史页从未接上。
        let query = self.history_query.read(cx).value().trim().to_string();
        let limit = self.history_limit;
        let entries = if query.is_empty() {
            history.list(limit, 0)
        } else {
            history.search(&query, limit)
        }
        .unwrap_or_default();
        let count = history.count().unwrap_or(0);

        let mut page = v_flex().px_4().py_3().gap_2();
        page = page.child(Input::new(&self.history_query).small());

        // 头部：计数 + 清空。
        page = page.child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .pb_2()
                .border_b_1()
                .border_color(border)
                .child(
                    div()
                        .text_size(Text::Body.size())
                        .text_color(muted_fg)
                        .child(SharedString::from(t_args(
                            "config-history-count",
                            &[("count", &count.to_string())],
                        ))),
                )
                .child(
                    Button::new("history-clear")
                        .danger()
                        .xsmall()
                        .label(t("config-history-clear").as_str())
                        .disabled(entries.is_empty())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = oxidict_core::history::History::global().clear();
                            this.saved_hint = false;
                            cx.notify();
                        })),
                ),
        );

        if entries.is_empty() {
            // 有查询词时说明是「没匹配到」，避免被误读成历史被清空。
            let msg = if query.is_empty() {
                SharedString::from(t("config-history-empty"))
            } else {
                SharedString::from(t("config-history-no-match"))
            };
            page = page.child(
                div()
                    .pt_8()
                    .text_size(Text::Label.size())
                    .text_color(muted_fg)
                    .child(msg),
            );
            return page.into_any_element();
        }

        for (ix, entry) in entries.iter().enumerate() {
            let id = entry.id;
            let time = chrono::DateTime::from_timestamp_millis(entry.timestamp)
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            let source = one_line(&entry.text, 40);
            let result = one_line(&entry.result, 60);
            // 复制用的必须是完整译文，不是列表里截断的预览。
            let full_result = entry.result.clone();
            let service = entry.service.clone();
            page = page.child(
                h_flex()
                    .id(("history-row", ix))
                    .w_full()
                    .gap_3()
                    .items_center()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .min_w(px(108.))
                            .text_size(Text::Sm.size())
                            .text_color(muted_fg)
                            .child(SharedString::from(time)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(Text::Body.size())
                            .text_color(fg)
                            .truncate()
                            .child(SharedString::from(source)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(Text::Body.size())
                            .text_color(muted_fg)
                            .truncate()
                            .child(SharedString::from(result)),
                    )
                    .child(
                        div()
                            .text_size(Text::Sm.size())
                            .text_color(muted_fg)
                            .child(SharedString::from(service)),
                    )
                    .child(
                        Button::new(("history-copy", ix))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .tooltip(t("config-history-copy").as_str())
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    full_result.clone(),
                                ));
                            })),
                    )
                    .child(
                        Button::new(("history-del", ix))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                let _ = oxidict_core::history::History::global().delete(id);
                                cx.notify();
                            })),
                    ),
            );
        }

        // 拿满一页时才可能出现更多；检索结果通常少于一页，此时按钮不出现。
        // limit 是 u32（History 的形参类型），与 usize 比较走 try_from，
        // 避免引入 `as` 转换。
        let limit_usize = usize::try_from(limit).unwrap_or(usize::MAX);
        if entries.len() >= limit_usize {
            page = page.child(
                h_flex().w_full().justify_center().py_2().child(
                    Button::new("history-more")
                        .ghost()
                        .small()
                        .label(t("config-history-more").as_str())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.history_limit += HISTORY_PAGE;
                            cx.notify();
                        })),
                ),
            );
        }

        page.into_any_element()
    }

    pub(super) fn render_backup_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let (fg, muted_fg, border) = {
            let t = cx.theme();
            (
                t.colors.foreground,
                t.colors.muted_foreground,
                t.colors.border,
            )
        };

        let mut page = v_flex().px_4().py_3().gap_2();

        // 备份位置：WebDAV 服务器 或 本机目录（可指向云盘客户端的同步目录）。
        // 两种后端共用同一组 list/put/get 操作，只是落点不同。
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
                        .text_size(Text::Label.size())
                        .text_color(fg)
                        .child(SharedString::from(t("config-backup-type"))),
                )
                .child(Select::new(&self.backup_kind).small()),
        );

        page = page.child(
            div()
                .text_size(Text::Body.size())
                .text_color(muted_fg)
                .child(SharedString::from(t("config-backup-webdav-hint"))),
        );

        if self.backup_kind_is_local(cx) {
            page = page.child(labeled_field(
                &t("config-backup-folder"),
                &self.backup_folder,
                false,
            ));
        } else {
            page = page.child(labeled_field(
                &t("config-backup-server"),
                &self.webdav_url,
                false,
            ));
            page = page.child(labeled_field(
                &t("config-backup-username"),
                &self.webdav_username,
                false,
            ));
            page = page.child(labeled_field(
                &t("config-backup-password"),
                &self.webdav_password,
                true,
            ));
        }

        if let Some(status) = &self.backup_status {
            page = page.child(
                div()
                    .text_size(Text::Body.size())
                    .text_color(fg)
                    .child(SharedString::from(status.clone())),
            );
        }

        // 按钮不持有凭据快照：点击时按**当前界面状态**构造后端，
        // 因此切后端类型、改路径会立刻反映到下一次操作。
        let ready = self.backend_ready(cx);
        page = page.child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("backup-put")
                        .primary()
                        .small()
                        .label(t("config-backup-to-cloud"))
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.do_backup(window, cx);
                        })),
                )
                .child(
                    // 只拉列表；恢复哪一份由用户在下方选择并确认。
                    Button::new("backup-list")
                        .secondary()
                        .small()
                        .label(t("config-backup-from-cloud"))
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.do_fetch_backups(window, cx);
                        })),
                ),
        );

        // ── 本地导出/导入 ──
        page = page.child(
            div()
                .text_size(Text::Body.size())
                .text_color(muted_fg)
                .child(SharedString::from(t("config-backup-local-hint"))),
        );

        page = page.child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("local-export")
                        .secondary()
                        .small()
                        .label(t("config-backup-export"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.do_local_export(window, cx);
                        })),
                )
                .child(
                    Button::new("local-import")
                        .secondary()
                        .small()
                        .label(t("config-backup-import"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.do_local_import(window, cx);
                        })),
                ),
        );

        // ── 待确认的恢复 ──
        // 恢复会整份替换 config.json 与 history.db，必须先让用户看清来源再确认。
        if let Some(pending) = &self.pending_restore {
            let desc = match pending {
                PendingRestore::Remote(name) => {
                    SharedString::from(t_args("config-restore-confirm-remote", &[("name", name)]))
                }
                PendingRestore::Local => SharedString::from(t("config-restore-confirm-local")),
            };
            page = page.child(
                v_flex()
                    .gap_2()
                    .p_2()
                    .rounded_sm()
                    .border_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(Text::Body.size())
                            .text_color(fg)
                            .child(desc),
                    )
                    .child(
                        div()
                            .text_size(Text::Sm.size())
                            .text_color(muted_fg)
                            .child(SharedString::from(t("config-restore-snapshot-note"))),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("restore-cancel")
                                    .ghost()
                                    .xsmall()
                                    .label(t("common-cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.do_cancel_restore(cx);
                                    })),
                            )
                            .child(
                                Button::new("restore-confirm")
                                    .danger()
                                    .xsmall()
                                    .label(t("config-restore-confirm"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.do_confirm_restore(window, cx);
                                    })),
                            ),
                    ),
            );
        }

        // ── 云端备份列表 ──
        if let Some(list) = &self.remote_backups {
            if list.is_empty() {
                page = page.child(
                    div()
                        .text_size(Text::Body.size())
                        .text_color(muted_fg)
                        .child(SharedString::from(t("config-backup-empty"))),
                );
            } else {
                for (ix, name) in list.iter().enumerate() {
                    let name = name.clone();
                    page = page.child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .py_1()
                            .border_b_1()
                            .border_color(border)
                            .child(
                                div()
                                    .text_size(Text::Body.size())
                                    .text_color(fg)
                                    .child(SharedString::from(name.clone())),
                            )
                            .child(
                                Button::new(("backup-pick", ix))
                                    .secondary()
                                    .xsmall()
                                    .label(t("config-backup-pick"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.do_select_restore(name.clone(), cx);
                                    })),
                            ),
                    );
                }
            }
        }

        page.into_any_element()
    }

    /// 当前选中的后端类型（即 `BACKUP_TYPE` 的取值）。
    fn backup_kind_value(&self, cx: &Context<Self>) -> &'static str {
        kind_at(self.backup_kind.read(cx).selected_index(cx).map(|p| p.row))
    }

    /// 是否选中本地目录后端（决定备份页展示哪一组字段）。
    fn backup_kind_is_local(&self, cx: &Context<Self>) -> bool {
        self.backup_kind_value(cx) == KIND_LOCAL_FOLDER
    }

    /// 当前输入是否足以执行「备份 / 拉取列表」（按钮禁用态用）。
    fn backend_ready(&self, cx: &Context<Self>) -> bool {
        if self.backup_kind_is_local(cx) {
            !self.backup_folder.read(cx).value().trim().is_empty()
        } else {
            !self.webdav_url.read(cx).value().trim().is_empty()
                && !self.webdav_username.read(cx).value().trim().is_empty()
        }
    }

    /// 按**当前界面状态**构造后端，并把对应配置写回 config。
    ///
    /// 写回这一步是必要的：确认恢复时（`do_confirm_restore`）要按「已保存的值」
    /// 重建后端，那时手上没有输入框的即时值。
    fn build_backend(&self, cx: &Context<Self>) -> Result<BackupBackend, Error> {
        let kind = self.backup_kind_value(cx);
        let folder = self.backup_folder.read(cx).value().trim().to_string();
        let cfg = WebDavConfig {
            url: self.webdav_url.read(cx).value().trim().to_string(),
            username: self.webdav_username.read(cx).value().trim().to_string(),
            password: self.webdav_password.read(cx).value().to_string(),
        };
        let store = config();
        let _ = store.set(keys::BACKUP_TYPE, &kind);
        if kind == KIND_LOCAL_FOLDER {
            let _ = store.set(keys::BACKUP_FOLDER, &folder);
        } else {
            let _ = store.set(keys::WEBDAV_URL, &cfg.url);
            let _ = store.set(keys::WEBDAV_USERNAME, &cfg.username);
            let _ = store.set(keys::WEBDAV_PASSWORD, &cfg.password);
        }
        BackupBackend::select(kind, cfg, &folder)
    }

    /// 按**已保存**的配置重建后端（确认恢复时用）。
    fn saved_backend(&self) -> Result<BackupBackend, Error> {
        let store = config();
        let kind = store.get_or(keys::BACKUP_TYPE, String::from(KIND_WEBDAV));
        let cfg = WebDavConfig {
            url: store.get_or(keys::WEBDAV_URL, String::new()),
            username: store.get_or(keys::WEBDAV_USERNAME, String::new()),
            password: store.get_or(keys::WEBDAV_PASSWORD, String::new()),
        };
        let folder = store.get_or(keys::BACKUP_FOLDER, String::new());
        BackupBackend::select(&kind, cfg, &folder)
    }

    /// 打包当前配置与历史，写入所选后端。
    fn do_backup(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let backend = match self.build_backend(cx) {
            Ok(backend) => backend,
            Err(e) => {
                self.backup_status = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        let name = format!(
            "oxidict-backup-{}.zip",
            oxidict_net::iso_utc_now().replace(':', "")
        );
        self.pending_restore = None;
        self.remote_backups = None;
        self.backup_status = Some(t_args("config-backup-uploading", &[("name", &name)]));
        let handle = oxidict_core::runtime::spawn(async move {
            let data = oxidict_core::backup::pack()?;
            backend.put(&name, data).await?;
            Ok::<_, Error>(name)
        });
        cx.spawn(async move |this, cx| {
            let status = match handle.await {
                Ok(Ok(name)) => t_args("config-backup-uploaded", &[("name", &name)]),
                Ok(Err(e)) => t_args("config-backup-upload-failed", &[("error", &e.to_string())]),
                Err(e) => t_args("task-cancelled", &[("error", &e.to_string())]),
            };
            let _ = this.update(cx, |this, cx| {
                this.backup_status = Some(status);
                cx.notify();
            });
        })
        .detach();
    }

    /// 拉取备份列表（只拉列表，不恢复）。
    ///
    /// 恢复哪一份必须由用户显式选择并确认：早期实现静默取「列表最后一个」，
    /// 用户既看不到选中了哪一份，也没有任何确认步骤，选错即整份覆盖本地数据。
    fn do_fetch_backups(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let backend = match self.build_backend(cx) {
            Ok(backend) => backend,
            Err(e) => {
                self.backup_status = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        self.pending_restore = None;
        self.remote_backups = None;
        self.backup_status = Some(t("config-backup-list-fetching"));
        let handle = oxidict_core::runtime::spawn(async move { backend.list().await });
        cx.spawn(async move |this, cx| {
            let (status, list) = match handle.await {
                Ok(Ok(files)) if files.is_empty() => (t("config-backup-empty"), Some(files)),
                Ok(Ok(files)) => (
                    t_args(
                        "config-backup-list-count",
                        &[("count", &files.len().to_string())],
                    ),
                    Some(files),
                ),
                Ok(Err(e)) => (
                    t_args("config-backup-list-failed", &[("error", &e.to_string())]),
                    None,
                ),
                Err(e) => (t_args("task-cancelled", &[("error", &e.to_string())]), None),
            };
            let _ = this.update(cx, |this, cx| {
                this.backup_status = Some(status);
                if list.is_some() {
                    this.remote_backups = list;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 选中某份云端备份，进入确认态（尚未下载、尚未写入本地）。
    fn do_select_restore(&mut self, name: String, cx: &mut Context<Self>) {
        self.pending_restore = Some(PendingRestore::Remote(name));
        cx.notify();
    }

    /// 请求从本地 zip 恢复，进入确认态。
    fn do_request_local_import(&mut self, cx: &mut Context<Self>) {
        self.pending_restore = Some(PendingRestore::Local);
        cx.notify();
    }

    /// 取消待确认的恢复。
    fn do_cancel_restore(&mut self, cx: &mut Context<Self>) {
        self.pending_restore = None;
        cx.notify();
    }

    /// 用户确认后执行恢复。
    ///
    /// 覆盖前 `oxidict_core::backup::unpack` 会把现有 `config.json` / `history.db`
    /// 另存为 `*.pre-restore.bak`，作为选错备份时的回退路径。
    fn do_confirm_restore(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_restore.take() else {
            return;
        };
        self.remote_backups = None;

        match pending {
            PendingRestore::Local => {
                let path = config()
                    .app_dir()
                    .join("oxidict-backup.zip")
                    .to_str()
                    .unwrap_or("/tmp/oxidict-backup.zip")
                    .to_string();
                match oxidict_core::backup::import_from_file(&path) {
                    Ok(()) => self.backup_status = Some(t("config-restore-done-local")),
                    Err(e) => {
                        self.backup_status = Some(t_args(
                            "config-restore-failed",
                            &[("error", &e.to_string())],
                        ))
                    }
                }
                cx.notify();
            }
            PendingRestore::Remote(name) => {
                // 按已保存的配置重建后端：拉列表时已把当前输入写回 config，
                // 因此这里与用户当时看到的后端一致。
                let backend = match self.saved_backend() {
                    Ok(backend) => backend,
                    Err(e) => {
                        self.backup_status = Some(e.to_string());
                        cx.notify();
                        return;
                    }
                };
                self.backup_status = Some(t_args("config-restore-restoring", &[("name", &name)]));
                let handle = oxidict_core::runtime::spawn(async move {
                    let data = backend.get(&name).await?;
                    oxidict_core::backup::unpack(&data)?;
                    Ok::<_, Error>(name)
                });
                cx.spawn(async move |this, cx| {
                    let status = match handle.await {
                        Ok(Ok(name)) => t_args("config-restore-done", &[("name", &name)]),
                        Ok(Err(e)) => t_args("config-restore-failed", &[("error", &e.to_string())]),
                        Err(e) => t_args("task-cancelled", &[("error", &e.to_string())]),
                    };
                    let _ = this.update(cx, |this, cx| {
                        this.backup_status = Some(status);
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }

    /// 导出到本地 zip 文件。
    fn do_local_export(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match oxidict_core::backup::export_to_file(
            config()
                .app_dir()
                .join("oxidict-backup.zip")
                .to_str()
                .unwrap_or("/tmp/oxidict-backup.zip"),
        ) {
            Ok(()) => self.backup_status = Some(t("config-backup-export-done")),
            Err(e) => {
                self.backup_status = Some(t_args(
                    "config-backup-export-failed",
                    &[("error", &e.to_string())],
                ))
            }
        }
        cx.notify();
    }

    /// 请求从本地 zip 恢复：先进入确认态，不直接覆盖。
    fn do_local_import(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.do_request_local_import(cx);
    }

    // ── 自定义服务（配置驱动）─────────────────────────────────────────────

    /// 鉴权方式下拉的当前下标。
    fn custom_auth_ix(&self, cx: &Context<Self>) -> usize {
        self.custom_auth
            .read(cx)
            .selected_index(cx)
            .map(|p| p.row)
            .unwrap_or(0)
    }

    fn custom_method_value(&self, cx: &Context<Self>) -> String {
        match self
            .custom_method
            .read(cx)
            .selected_index(cx)
            .map(|p| p.row)
        {
            Some(0) => "GET".to_string(),
            _ => "POST".to_string(),
        }
    }

    /// 指纹下拉的取值。选项与 `TLS_OPTIONS` 同源，下标越界时回退 `native`。
    fn custom_tls_value(&self, cx: &Context<Self>) -> String {
        let ix = self
            .custom_tls
            .read(cx)
            .selected_index(cx)
            .map(|p| p.row)
            .unwrap_or(0);
        oxidict_services::TLS_OPTIONS
            .get(ix)
            .map(|(value, _)| (*value).to_string())
            .unwrap_or_else(|| "native".to_string())
    }

    /// 一行「标签 + 右侧小号控件」。
    fn custom_row(
        label: String,
        control: gpui_kit::AnyElement,
        cx: &Context<Self>,
    ) -> gpui_kit::AnyElement {
        let (fg, border) = {
            let theme = cx.theme();
            (theme.colors.foreground, theme.colors.border)
        };
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .py_2()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .text_size(Text::Label.size())
                    .text_color(fg)
                    .child(SharedString::from(label)),
            )
            .child(control)
            .into_any_element()
    }

    /// 自定义服务页：表单 + 已有条目列表。
    ///
    /// 表单只覆盖「纯映射服务」的典型形态（地址 + 鉴权 + 取字段路径）；进阶用法
    /// （headers / fields / 流式）仍在 config.json 里手写——把那些塞进表单会让
    /// 界面复杂度暴涨，而收益只落在少数条目上。
    pub(super) fn render_custom_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let muted_fg = cx.theme().colors.muted_foreground;
        let auth_ix = self.custom_auth_ix(cx);

        let mut page = v_flex().px_4().py_3().gap_2();

        page = page.child(
            div()
                .text_size(Text::Body.size())
                .text_color(muted_fg)
                .child(SharedString::from(t("config-custom-intro"))),
        );

        page = page.child(labeled_field(
            &t("config-custom-id"),
            &self.custom_id,
            false,
        ));
        page = page.child(labeled_field(
            &t("config-custom-endpoint"),
            &self.custom_endpoint,
            false,
        ));
        page = page.child(Self::custom_row(
            t("config-custom-method"),
            Select::new(&self.custom_method).small().into_any_element(),
            cx,
        ));
        page = page.child(Self::custom_row(
            t("config-custom-auth"),
            Select::new(&self.custom_auth).small().into_any_element(),
            cx,
        ));
        // 只有"自定义头/查询参数"才需要参数名，避免固定占用一行。
        if auth_ix >= 2 {
            page = page.child(labeled_field(
                &t("config-custom-auth-name"),
                &self.custom_auth_name,
                false,
            ));
        }
        page = page.child(labeled_field(
            &t("config-custom-body"),
            &self.custom_body,
            false,
        ));
        page = page.child(labeled_field(
            &t("config-custom-extract"),
            &self.custom_extract,
            false,
        ));
        page = page.child(Self::custom_row(
            t("config-custom-tls"),
            Select::new(&self.custom_tls).small().into_any_element(),
            cx,
        ));

        if let Some(status) = &self.custom_status {
            page = page.child(
                div()
                    .text_size(Text::Body.size())
                    .child(SharedString::from(status.clone())),
            );
        }

        page = page.child(
            h_flex().gap_2().child(
                Button::new("custom-save")
                    .primary()
                    .small()
                    .label(t("config-custom-save"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_save_custom(window, cx);
                    })),
            ),
        );

        // ── 已有条目 ──
        page = page.child(
            div()
                .text_size(Text::Label.size())
                .child(SharedString::from(t("config-custom-list"))),
        );
        let specs = custom_svc::specs(&config());
        if specs.is_empty() {
            page = page.child(
                div()
                    .text_size(Text::Body.size())
                    .text_color(muted_fg)
                    .child(SharedString::from(t("config-custom-empty"))),
            );
        } else {
            for spec in specs {
                // 编辑与删除是两个独立闭包，String 不可 Copy，各自持有一份。
                let edit_id = spec.id.clone();
                let del_id = spec.id.clone();
                page = page.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .py_1()
                        .child(div().text_size(Text::Body.size()).child(SharedString::from(
                            format!("{} · {}", spec.id, spec.endpoint),
                        )))
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Button::new(("custom-edit", spec.id.len()))
                                        .ghost()
                                        .xsmall()
                                        .label(t("config-custom-edit"))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.do_edit_custom(edit_id.clone(), window, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("custom-del", spec.id.len()))
                                        .ghost()
                                        .xsmall()
                                        .label(t("config-custom-delete"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.do_delete_custom(del_id.clone(), cx);
                                        })),
                                ),
                        ),
                );
            }
        }

        page.into_any_element()
    }

    /// 保存表单：同 id 覆盖、否则追加，写回配置并**立即生效**。
    fn do_save_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = FormValues {
            id: self.custom_id.read(cx).value().trim().to_string(),
            endpoint: self.custom_endpoint.read(cx).value().trim().to_string(),
            method: self.custom_method_value(cx),
            auth_kind: custom_svc::auth_kind_at(self.custom_auth_ix(cx)).to_string(),
            auth_name: self.custom_auth_name.read(cx).value().trim().to_string(),
            body: self.custom_body.read(cx).value().trim().to_string(),
            extract_text: self.custom_extract.read(cx).value().trim().to_string(),
            tls: self.custom_tls_value(cx),
        };

        let spec = match form.to_spec() {
            Ok(spec) => spec,
            // 校验错误直接展示给用户——这里是他唯一能得到反馈的地方。
            Err(e) => {
                self.custom_status = Some(e);
                cx.notify();
                return;
            }
        };

        let store = config();
        let mut specs = custom_svc::specs(&store);
        match specs.iter().position(|s| s.id == spec.id) {
            Some(ix) => specs[ix] = spec,
            None => specs.push(spec),
        }
        match custom_svc::apply(&store, &specs) {
            Ok(()) => {
                self.custom_status = Some(t("config-custom-saved"));
                // 清空"新增"表单，避免反复提交同一条；用户想编辑再重新填。
                self.custom_id
                    .update(cx, |s, cx| s.set_value("", window, cx));
                self.custom_endpoint
                    .update(cx, |s, cx| s.set_value("", window, cx));
            }
            Err(e) => self.custom_status = Some(e.to_string()),
        }
        cx.notify();
    }

    /// 把既有条目回填到表单。
    ///
    /// **下拉也必须回填**：只填文本框的话，保存会把鉴权/指纹静默改回默认值——
    /// 这类"编辑器吃配置"的 bug 比不能编辑更糟。
    fn do_edit_custom(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let store = config();
        let Some(spec) = custom_svc::specs(&store).into_iter().find(|s| s.id == id) else {
            self.custom_status = Some(format!("找不到自定义服务 {id}"));
            cx.notify();
            return;
        };
        // 签名协议在表单的下拉里没有对应项，回填会显示成"无鉴权"，
        // 用户一保存就把签名配置改没了——明确拒绝并指路。
        // 表单表达不了的配置（签名协议 / 多步会话）不能进编辑：
        // 回填会把它们显示成默认值，用户一保存就丢——那比不能编辑更糟。
        if matches!(spec.auth, custom_svc::AuthSpec::HashQuery { .. }) || spec.session.is_some() {
            self.custom_status = Some(t("config-custom-edit-unsupported"));
            cx.notify();
            return;
        }
        let form = FormValues::from_spec(&spec);
        // 闭包里可变借用 `window`，因此本身也要 mut。
        let mut set_text = |state: &Entity<InputState>, value: &str, cx: &mut Context<Self>| {
            state.update(cx, |s, cx| s.set_value(value, window, cx));
        };
        set_text(&self.custom_id, &form.id, cx);
        set_text(&self.custom_endpoint, &form.endpoint, cx);
        set_text(&self.custom_auth_name, &form.auth_name, cx);
        set_text(&self.custom_body, &form.body, cx);
        set_text(&self.custom_extract, &form.extract_text, cx);

        let method_ix = if form.method.eq_ignore_ascii_case("GET") {
            0
        } else {
            1
        };
        self.custom_method.update(cx, |s, cx| {
            s.set_selected_index(Some(gpui_kit::base::IndexPath::new(method_ix)), window, cx)
        });
        let auth_ix = custom_svc::AUTH_KINDS
            .iter()
            .position(|k| *k == form.auth_kind)
            .unwrap_or(0);
        self.custom_auth.update(cx, |s, cx| {
            s.set_selected_index(Some(gpui_kit::base::IndexPath::new(auth_ix)), window, cx)
        });
        let tls_ix = oxidict_services::TLS_OPTIONS
            .iter()
            .position(|(v, _)| *v == form.tls)
            .unwrap_or(0);
        self.custom_tls.update(cx, |s, cx| {
            s.set_selected_index(Some(gpui_kit::base::IndexPath::new(tls_ix)), window, cx)
        });

        self.custom_status = Some(t("config-custom-editing"));
        cx.notify();
    }

    /// 删除一条自定义服务并立即生效（`custom::apply` 内部会重载注册表）。
    fn do_delete_custom(&mut self, id: String, cx: &mut Context<Self>) {
        let store = config();
        let mut specs = custom_svc::specs(&store);
        specs.retain(|s| s.id != id);
        match custom_svc::apply(&store, &specs) {
            Ok(()) => self.custom_status = Some(t("config-custom-saved")),
            Err(e) => self.custom_status = Some(e.to_string()),
        }
        cx.notify();
    }
}
