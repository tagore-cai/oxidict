//! 应用自有界面的排版标尺与字体配置注入。
//!
//! 改造前：`text_size(px(N))` 散落 100+ 处，正文层级靠手写的 12px / 13px 区分
//! （12 = 说明与数据，13 = 设置项标签——分工其实是自洽的，只是没有名字，也没有
//! 单一来源）。这里把既有档位收敛成**具名档位**，未缩放值与改造前逐一对应，
//! 因此 `app_font_size = 16`（默认）时**不产生任何视觉变化**。
//!
//! 同时让 `app_font` / `app_font_size` 两个配置真正生效——它们此前是死键：
//! 经由 `ThemeConfig.font_size` / `.font_family` 注入主题，组件库自带的控件
//! （按钮、输入框、下拉）也会一起跟随。

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Pixels, Window, px};
use oxidict_core::config::{config, keys};
use std::rc::Rc;

/// 缩放基准字号（`app_font_size` 的缺省值，也是比例分母）。
pub const BASE_PX: f32 = 16.0;
/// 缩放下限：再小会影响可读性。
const SCALE_MIN: f32 = 0.75;
/// 缩放上限：本工程大量使用固定像素内边距，放太大布局会挤爆，故收紧。
const SCALE_MAX: f32 = 1.6;

/// 具名文字档位。注释里的像素值是**未缩放**值。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Text {
    /// 10px：侧栏极小字
    Xs,
    /// 11px：次要小字
    Sm,
    /// 12px：说明 / 辅助文本 / 数据行
    Body,
    /// 13px：设置项标签
    Label,
    /// 14px：页标题
    Heading,
    /// 16px：侧栏 Logo 字符
    Logo,
}

impl Text {
    /// 未缩放像素值。
    const fn base(self) -> f32 {
        match self {
            Self::Xs => 10.0,
            Self::Sm => 11.0,
            Self::Body => 12.0,
            Self::Label => 13.0,
            Self::Heading => 14.0,
            Self::Logo => 16.0,
        }
    }

    /// 应用全局字号缩放后的尺寸。
    pub fn size(self) -> Pixels {
        px(self.base() * scale())
    }
}

/// 由配置值算缩放系数（纯函数，便于测试）。
pub fn scale_from(configured_px: f32) -> f32 {
    (configured_px / BASE_PX).clamp(SCALE_MIN, SCALE_MAX)
}

/// 当前配置下的全局缩放系数。
pub fn scale() -> f32 {
    scale_from(config().get_or(keys::APP_FONT_SIZE, BASE_PX))
}

/// 把 `app_font` / `app_font_size` 注入主题，并按 `mode` 应用。
///
/// 注入点必须是 `ThemeConfig.font_size` / `.font_family`：`Theme::change` 会用
/// light/dark 两份 `ThemeConfig` 重算 `Theme.font_family` / `font_size`
/// （见 gpui-component `theme/schema.rs::apply_config`），直接改 `Theme` 上那两个
/// 公开字段会被下一次 `Theme::change` 覆盖回去。
pub fn apply_font_config(mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    let family: String = config().get_or(keys::APP_FONT, String::new());
    let size: f32 = config().get_or(keys::APP_FONT_SIZE, BASE_PX);

    // 首次调用时全局 Theme 尚不存在，`Theme::change` 会从 registry 装入默认的
    // light/dark 配置；必须等它建立之后再改写，否则改动会被默认值覆盖。
    Theme::change(mode, None, cx);

    {
        let theme = cx.global_mut::<Theme>();
        for slot in [&mut theme.light_theme, &mut theme.dark_theme] {
            let cfg = Rc::make_mut(slot);
            cfg.font_size = Some(size);
            // 空串与 "default" 都表示「用系统字体」：保持 None，让库按平台挑。
            cfg.font_family = if family.is_empty() || family == "default" {
                None
            } else {
                Some(family.clone().into())
            };
        }
    }

    // 重新应用一次，这次带着上面注入的字体设置。
    Theme::change(mode, window, cx);
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// 档位必须单调递增，且默认基准（16）下尺寸与改造前完全一致——
    /// 这是「不顺手改观感」的硬约束。
    #[test]
    fn steps_are_ordered_and_match_legacy_pixels() {
        let pairs = [
            (Text::Xs, 10.0),
            (Text::Sm, 11.0),
            (Text::Body, 12.0),
            (Text::Label, 13.0),
            (Text::Heading, 14.0),
            (Text::Logo, 16.0),
        ];
        let mut prev = 0.0;
        for (step, expected) in pairs {
            assert_eq!(step.base(), expected, "{step:?} 基线值被改动");
            assert!(expected > prev, "{step:?} 破坏单调性");
            prev = expected;
        }
    }

    /// 默认基准下缩放系数恰好为 1（保证不顺手改观感），两端被夹紧。
    #[test]
    fn scale_is_unity_at_default_base_and_clamped() {
        assert_eq!(scale_from(BASE_PX), 1.0, "默认基准必须是 1.0");
        // 界面可选的 14/16/18/20 都应落在区间内、原样生效。
        for (px_value, expected) in [(14.0, 0.875), (18.0, 1.125), (20.0, 1.25)] {
            assert_eq!(scale_from(px_value), expected, "{px_value}px 不该被夹");
        }
        // 手改配置到极端值时才夹紧。
        assert_eq!(scale_from(4.0), SCALE_MIN, "过小应夹到下限");
        assert_eq!(scale_from(64.0), SCALE_MAX, "过大应夹到上限");
    }
}
