//! 翻译窗口。
//!
//! 对应原 `src/window/Translate`：源文本区、语言选择、按启用实例纵向排列的
//! 结果卡片（加载/结果/错误三态），以及复制、交换语言等操作。
//! 布局与原版一致：标题栏 -> 语言行 -> 源文本 -> 操作行 -> 结果卡片列表。

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Disableable, IconName, Sizable};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::base::IndexPath;
use gpui_kit::{
    div, px, relative, AnyElement, ClickEvent, ClipboardItem, Context, Entity, FontWeight,
    IntoElement, InteractiveElement, StatefulInteractiveElement, AppContext, ParentElement,
    Render, ScrollHandle,
    SharedString, Styled, Window,
};
use std::sync::Arc;
use saladict_core::config::config;
use saladict_core::i18n::{t, t_args};
use saladict_core::{Language, TranslateRequest, TranslateResult};
use saladict_services::{instance_display_name, spawn_translate};

/// 一个结果卡片的渲染状态。
#[derive(Clone)]
enum CardState {
    Idle,
    Loading,
    Done(TranslateResult),
    Failed(String),
}

#[derive(Clone)]
struct Card {
    instance: String,
    state: CardState,
}

pub struct TranslateWindow {
    source: Entity<TextareaState>,
    source_lang: Entity<SelectState<Vec<&'static str>>>,
    target_lang: Entity<SelectState<Vec<&'static str>>>,
    instances: Vec<String>,
    cards: Vec<Card>,
    scroll: ScrollHandle,
    /// dynamic_translate 的防抖代次，每次文本变化 +1，延迟后比较代次决定是否翻译。
    dynamic_generation: std::cell::Cell<u64>,
}

impl TranslateWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let languages: Vec<&'static str> = Language::ALL.iter().map(|l| l.display_name()).collect();
        let store = config();

        // 恢复上次选择的语言。
        let saved_source = store.source_language();
        let saved_target = store.target_language();
        let source_ix = Language::ALL
            .iter()
            .position(|l| *l == saved_source)
            .unwrap_or(0);
        let target_ix = Language::ALL
            .iter()
            .position(|l| *l == saved_target)
            .unwrap_or(2);

        let source_lang = cx.new(|cx| {
            SelectState::new(languages.clone(), Some(IndexPath::new(source_ix)), window, cx)
        });
        let target_lang = cx.new(|cx| {
            SelectState::new(languages, Some(IndexPath::new(target_ix)), window, cx)
        });
        let source = cx.new(|cx| TextareaState::new(window, cx));

        let instances = store.service_list(saladict_core::config::keys::TRANSLATE_SERVICE_LIST);
        let cards = instances
            .iter()
            .map(|instance| Card {
                instance: instance.clone(),
                state: CardState::Idle,
            })
            .collect();

        let mut this = Self {
            source,
            source_lang,
            target_lang,
            instances,
            cards,
            scroll: ScrollHandle::new(),
            dynamic_generation: std::cell::Cell::new(0),
        };

        // dynamic_translate：文本变化后 500ms 防抖自动翻译。
        if store.get_or(saladict_core::config::keys::DYNAMIC_TRANSLATE, false) {
            cx.subscribe_in(&this.source, window, Self::on_source_changed).detach();
        }

        this
    }

    /// dynamic_translate 的输入变化处理：递增代次 → 延迟 500ms → 代次未变则翻译。
    fn on_source_changed(
        &mut self,
        _: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let gen = self.dynamic_generation.get() + 1;
        self.dynamic_generation.set(gen);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            // 代次不匹配说明用户还在输入，跳过。
            let _ = this.update(cx, |this, cx| {
                if this.dynamic_generation.get() != gen {
                    return;
                }
                let text = this.source.read(cx).value().trim().to_string();
                if !text.is_empty() {
                    this.run_translate(text, cx);
                }
            });
        })
        .detach();
    }

    fn selected_language(
        &self,
        select: &Entity<SelectState<Vec<&'static str>>>,
        cx: &Context<Self>,
        fallback: Language,
    ) -> Language {
        let value = select.read(cx).selected_value().map(|v| v.to_string());
        match value {
            Some(label) => Language::ALL
                .iter()
                .find(|l| l.display_name() == label)
                .copied()
                .unwrap_or(fallback),
            None => fallback,
        }
    }

    fn start_translate(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.source.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.run_translate(text, cx);
    }

    /// 用已写入输入框的文本触发翻译（点「翻译」按钮的等价逻辑）。
    fn run_translate(&mut self, text: String, cx: &mut Context<Self>) {
        let store = config();
        let from = self.selected_language(&self.source_lang, cx, Language::Auto);
        let to = self.selected_language(&self.target_lang, cx, Language::ZhCn);
        // 记住语言选择，与原 translate_remember_language 行为一致。
        let _ = store.set(
            saladict_core::config::keys::TRANSLATE_SOURCE_LANGUAGE,
            &from.code().to_string(),
        );
        let _ = store.set(
            saladict_core::config::keys::TRANSLATE_TARGET_LANGUAGE,
            &to.code().to_string(),
        );

        // translate_delete_newline：非 0 时去除换行符。
        let text = if store.get_or(saladict_core::config::keys::TRANSLATE_DELETE_NEWLINE, 0) > 0 {
            text.replace('\n', " ")
        } else {
            text
        };

        for card in self.cards.iter_mut() {
            card.state = CardState::Loading;
        }
        cx.notify();

        let instances = self.instances.clone();
        for (idx, instance) in instances.into_iter().enumerate() {
            // 流式增量：LLM 类服务经 on_stream 回调推送累计文本，
            // 回调在 tokio 线程上执行，用 channel 转回 GPUI 更新卡片。
            let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let req =
                TranslateRequest::new(text.clone(), from, to).with_stream(Arc::new(move |delta| {
                    let _ = stream_tx.send(delta);
                }));
            let task = spawn_translate(&instance, req);

            // 增量上屏：服务发的累计文本带 "_" 光标尾缀（沿用原版习惯），展示时去掉。
            cx.spawn(async move |this, cx| {
                while let Some(delta) = stream_rx.recv().await {
                    let text = delta.trim_end_matches('_').to_string();
                    let _ = this.update(cx, |this, cx| {
                        if let Some(card) = this.cards.get_mut(idx) {
                            card.state = CardState::Done(TranslateResult::Plain(text));
                        }
                        cx.notify();
                    });
                }
            })
            .detach();

            cx.spawn(async move |this, cx| {
                let state = match task.await {
                    Ok(Ok(result)) => CardState::Done(result),
                    Ok(Err(e)) => CardState::Failed(e.to_string()),
                    Err(e) => {
                        let msg = e.to_string();
                        CardState::Failed(t_args("task-cancelled", &[("error", &msg)]))
                    }
                };
                let _ = this.update(cx, |this, cx| {
                    if let Some(card) = this.cards.get_mut(idx) {
                        card.state = state;
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// 供托盘/快捷键命令调用：把 `text` 写入源输入框并触发翻译。
    ///
    /// 复用时直接操作已存在的窗口实体（见 lib.rs 中的 `TranslateWindowHandle`
    /// 全局弱引用），不会重复开窗口。
    pub fn translate_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        // 用 replace_all 而非 set_value：保留撤销栈，且与用户编辑行为一致。
        self.source
            .update(cx, |source, cx| source.replace_all(text.clone(), window, cx));
        self.run_translate(text, cx);
    }

    fn swap_languages(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let from = self.selected_language(&self.source_lang, cx, Language::Auto);
        let to = self.selected_language(&self.target_lang, cx, Language::ZhCn);
        if from == Language::Auto {
            return;
        }
        let source_ix = Language::ALL.iter().position(|l| *l == to).unwrap_or(2);
        let target_ix = Language::ALL.iter().position(|l| *l == from).unwrap_or(0);
        self.source_lang.update(cx, |s, cx| {
            s.set_selected_index(Some(IndexPath::new(source_ix)), window, cx)
        });
        self.target_lang.update(cx, |s, cx| {
            s.set_selected_index(Some(IndexPath::new(target_ix)), window, cx)
        });
        cx.notify();
    }

    fn copy_result(&mut self, idx: usize, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(card) = self.cards.get(idx) {
            if let CardState::Done(result) = &card.state {
                cx.write_to_clipboard(ClipboardItem::new_string(result.as_text()));
            }
        }
    }

    fn render_card(&self, card: &Card, idx: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = instance_display_name(&card.instance, &config());

        let body: AnyElement = match &card.state {
            CardState::Idle => div()
                .text_color(theme.colors.muted_foreground)
                .child(SharedString::from(t("translate-idle-hint")))
                .into_any_element(),
            CardState::Loading => h_flex()
                .gap_2()
                .child(Spinner::new().small())
                .child(div().text_color(theme.colors.muted_foreground).child(SharedString::from(t("app-translating"))))
                .into_any_element(),
            CardState::Done(result) => div()
                .text_color(theme.colors.foreground)
                .child(SharedString::from(result.as_text()))
                .into_any_element(),
            CardState::Failed(message) => div()
                .text_color(theme.colors.danger)
                .child(SharedString::from(message.clone()))
                .into_any_element(),
        };

        v_flex()
            .id(idx)
            .w_full()
            .border_1()
            .border_color(theme.colors.border)
            .rounded_md()
            .p_2()
            .gap_1()
            .bg(theme.colors.background)
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.colors.muted_foreground)
                            .child(name),
                    )
                    .child(
                        Button::new(("copy", idx))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .disabled(!matches!(card.state, CardState::Done(_)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.copy_result(idx, window, cx)
                            })),
                    ),
            )
            .child(body)
            .into_any_element()
    }
}

impl Render for TranslateWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .size_full()
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            // 标题栏：透明样式下兼任拖拽区
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
                            .child(SharedString::from(t("translate-title"))),
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
            // 语言行
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .items_center()
                    .child(Select::new(&self.source_lang).xsmall().w(relative(0.35)))
                    .child(
                        Button::new("swap")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Replace)
                            .on_click(cx.listener(Self::swap_languages)),
                    )
                    .child(Select::new(&self.target_lang).xsmall().w(relative(0.35))),
            )
            // 源文本
            .child(
                v_flex().px_2().pb_1().child(Textarea::new(&self.source).h(px(96.))),
            )
            // 操作行
            .child(
                h_flex()
                    .px_2()
                    .pb_1()
                    .justify_between()
                    .child(div())
                    .child(
                        Button::new("translate")
                            .primary()
                            .small()
                            .label(SharedString::from(t("translate-action")))
                            .on_click(cx.listener(Self::start_translate)),
                    ),
            )
            // 结果卡片列表
            .child(
                div()
                    .id("results")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll().track_scroll(&self.scroll)
                    .child(
                        v_flex().px_2().pb_2().gap_2().children(
                            self.cards
                                .iter()
                                .enumerate()
                                .map(|(idx, card)| self.render_card(card, idx, cx)),
                        ),
                    ),
            )
    }
}
