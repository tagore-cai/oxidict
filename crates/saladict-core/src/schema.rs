//! 服务配置的声明式描述。
//!
//! 原实现里每个服务都手写一份 `Config.jsx`，30 份表单结构上高度同构，
//! 改动一个字段要改 30 处。这里改为服务只声明字段，设置面板统一渲染。

/// 一个可渲染的配置字段。
#[derive(Debug, Clone)]
pub enum ConfigField {
    /// 单行文本。`secret` 为真时 UI 用密码框，并在历史/日志中脱敏。
    Text {
        key: &'static str,
        label: &'static str,
        placeholder: &'static str,
        secret: bool,
        required: bool,
    },
    /// 下拉选择。`options` 为 (值, 展示名)。
    Select {
        key: &'static str,
        label: &'static str,
        options: &'static [(&'static str, &'static str)],
        default: &'static str,
    },
    /// 开关。
    Switch {
        key: &'static str,
        label: &'static str,
        default: bool,
    },
    /// 数值。
    Number {
        key: &'static str,
        label: &'static str,
        default: f64,
        min: f64,
        max: f64,
    },
}

impl ConfigField {
    pub fn text(key: &'static str, label: &'static str) -> Self {
        ConfigField::Text {
            key,
            label,
            placeholder: "",
            secret: false,
            required: false,
        }
    }

    pub fn secret(key: &'static str, label: &'static str) -> Self {
        ConfigField::Text {
            key,
            label,
            placeholder: "",
            secret: true,
            required: true,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            ConfigField::Text { key, .. }
            | ConfigField::Select { key, .. }
            | ConfigField::Switch { key, .. }
            | ConfigField::Number { key, .. } => key,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ConfigField::Text { label, .. }
            | ConfigField::Select { label, .. }
            | ConfigField::Switch { label, .. }
            | ConfigField::Number { label, .. } => label,
        }
    }
}
