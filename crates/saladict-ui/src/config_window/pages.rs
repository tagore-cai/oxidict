//! 非服务类设置页：快捷键、通用（外观/代理/高级）、历史、备份。

use super::ConfigWindow;
use crate::hotkey::HotkeyRecorder;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputContentType, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, IconName, Sizable};
use gpui_kit::{
    Context, Entity, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window,
    div, px,
};
use saladict_core::Error;
use saladict_core::config::{config, keys};
use saladict_core::i18n::{t, t_args};

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
        .text_size(px(12.))
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
                .text_size(px(12.))
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
            .unwrap_or(60606);
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
        saladict_net::rebuild_client();
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
                        .text_size(px(13.))
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
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(fg)
                            .child(SharedString::from(t("config-dark-mode"))),
                    )
                    .child(Switch::new("dark-mode").checked(self.dark_mode).on_click(
                        move |checked: &bool, window, cx| {
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
                        },
                    )),
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
                .child(
                    div()
                        .text_size(px(13.))
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
                                .text_size(px(13.))
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
                                .text_size(px(13.))
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
                                .text_size(px(13.))
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
                                .text_size(px(13.))
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
                                .text_size(px(13.))
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
                            .text_size(px(13.))
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
                            .text_size(px(13.))
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
                                saladict_platform::dock::set_visible(!checked);
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
                            .text_size(px(13.))
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
                            .text_size(px(13.))
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
        let history = saladict_core::history::History::global();
        let entries = history.list(100, 0).unwrap_or_default();
        let count = history.count().unwrap_or(0);

        let mut page = v_flex().px_4().py_3().gap_2();

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
                        .text_size(px(12.))
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
                            let _ = saladict_core::history::History::global().clear();
                            this.saved_hint = false;
                            cx.notify();
                        })),
                ),
        );

        if entries.is_empty() {
            page = page.child(
                div()
                    .pt_8()
                    .text_size(px(13.))
                    .text_color(muted_fg)
                    .child(SharedString::from(t("config-history-empty"))),
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
                            .text_size(px(11.))
                            .text_color(muted_fg)
                            .child(SharedString::from(time)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(fg)
                            .truncate()
                            .child(SharedString::from(source)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(muted_fg)
                            .truncate()
                            .child(SharedString::from(result)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted_fg)
                            .child(SharedString::from(service)),
                    )
                    .child(
                        Button::new(("history-del", ix))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                let _ = saladict_core::history::History::global().delete(id);
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
        let (fg, muted_fg, _border) = {
            let t = cx.theme();
            (
                t.colors.foreground,
                t.colors.muted_foreground,
                t.colors.border,
            )
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
        let name = format!(
            "saladict-backup-{}.zip",
            saladict_net::iso_utc_now().replace(':', "")
        );
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
            let name = files
                .last()
                .ok_or_else(|| Error::Service("云端没有备份文件".into()))?
                .clone();
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
            config()
                .app_dir()
                .join("saladict-backup.zip")
                .to_str()
                .unwrap_or("/tmp/saladict-backup.zip"),
        ) {
            Ok(()) => self.backup_status = Some("已导出到配置目录 saladict-backup.zip".into()),
            Err(e) => self.backup_status = Some(format!("导出失败: {e}")),
        }
        cx.notify();
    }

    /// 从本地 zip 文件导入。
    fn do_local_import(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match saladict_core::backup::import_from_file(
            config()
                .app_dir()
                .join("saladict-backup.zip")
                .to_str()
                .unwrap_or("/tmp/saladict-backup.zip"),
        ) {
            Ok(()) => self.backup_status = Some("已从配置目录 saladict-backup.zip 恢复".into()),
            Err(e) => self.backup_status = Some(format!("导入失败: {e}")),
        }
        cx.notify();
    }
}
