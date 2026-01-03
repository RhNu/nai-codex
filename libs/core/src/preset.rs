//! 预设模块 - 包含角色预设和主预设的定义与处理逻辑
//!
//! 预设应用规则:
//! 1. 空白字符的条目会被跳过
//! 2. 如果设置了 replace，则 before 和 after 失效
//! 3. before/after 会在原提示词前后添加内容

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 判断字符串是否为空或仅包含空白字符
fn is_blank(s: &Option<String>) -> bool {
    match s {
        None => true,
        Some(s) => s.trim().is_empty(),
    }
}

/// 角色预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPreset {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// 预览图路径
    #[serde(default)]
    pub preview_path: Option<String>,
    /// 正向提示词：添加到原提示词之前
    pub before: Option<String>,
    /// 正向提示词：添加到原提示词之后
    pub after: Option<String>,
    /// 正向提示词：完全替换原提示词
    pub replace: Option<String>,
    /// 负面提示词：添加到原UC之前
    #[serde(default)]
    pub uc_before: Option<String>,
    /// 负面提示词：添加到原UC之后
    #[serde(default)]
    pub uc_after: Option<String>,
    /// 负面提示词：完全替换原UC
    #[serde(default)]
    pub uc_replace: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

impl CharacterPreset {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            preview_path: None,
            before: None,
            after: None,
            replace: None,
            uc_before: None,
            uc_after: None,
            uc_replace: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Apply preset to negative prompt (UC).
    /// 规则: replace 非空白则直接替换；否则应用 before/after（非空白时）
    pub fn apply_uc(&self, raw_uc: &str) -> String {
        // replace 优先级最高，且非空白时生效
        if !is_blank(&self.uc_replace) {
            return self.uc_replace.as_ref().unwrap().clone();
        }

        let mut result = String::new();
        // before 非空白时添加
        if !is_blank(&self.uc_before) {
            result.push_str(self.uc_before.as_ref().unwrap().trim());
            if !result.is_empty() && !result.ends_with(' ') && !result.ends_with(',') {
                result.push_str(", ");
            }
        }
        result.push_str(raw_uc);
        // after 非空白时添加
        if !is_blank(&self.uc_after) {
            if !result.is_empty() && !result.ends_with(' ') && !result.ends_with(',') {
                result.push_str(", ");
            }
            result.push_str(self.uc_after.as_ref().unwrap().trim());
        }
        result
    }

    /// Apply preset to raw prompt before snippet expansion.
    /// 规则: replace 非空白则直接替换；否则应用 before/after（非空白时）
    pub fn apply(&self, raw_prompt: &str) -> String {
        // replace 优先级最高，且非空白时生效
        if !is_blank(&self.replace) {
            return self.replace.as_ref().unwrap().clone();
        }

        let mut result = String::new();
        // before 非空白时添加
        if !is_blank(&self.before) {
            result.push_str(self.before.as_ref().unwrap().trim());
            if !result.is_empty() && !result.ends_with(' ') {
                result.push(' ');
            }
        }
        result.push_str(raw_prompt);
        // after 非空白时添加
        if !is_blank(&self.after) {
            if !result.is_empty() && !result.ends_with(' ') {
                result.push(' ');
            }
            result.push_str(self.after.as_ref().unwrap().trim());
        }
        result
    }
}

/// 主提示词预设实体，用于持久化存储
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainPreset {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// 正面提示词：添加到原提示词之前
    #[serde(default)]
    pub before: Option<String>,
    /// 正面提示词：添加到原提示词之后
    #[serde(default)]
    pub after: Option<String>,
    /// 正面提示词：完全替换原提示词
    #[serde(default)]
    pub replace: Option<String>,
    /// 负面提示词：添加到原UC之前
    #[serde(default)]
    pub uc_before: Option<String>,
    /// 负面提示词：添加到原UC之后
    #[serde(default)]
    pub uc_after: Option<String>,
    /// 负面提示词：完全替换原UC
    #[serde(default)]
    pub uc_replace: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

impl MainPreset {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            before: None,
            after: None,
            replace: None,
            uc_before: None,
            uc_after: None,
            uc_replace: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 转换为设置对象
    pub fn to_settings(&self) -> MainPresetSettings {
        MainPresetSettings {
            before: self.before.clone(),
            after: self.after.clone(),
            replace: self.replace.clone(),
            uc_before: self.uc_before.clone(),
            uc_after: self.uc_after.clone(),
            uc_replace: self.uc_replace.clone(),
        }
    }
}

/// 主提示词预设设置，用于注入到主正/负面提示词（非持久化版本，用于任务提交）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MainPresetSettings {
    /// 正面提示词：添加到原提示词之前
    #[serde(default)]
    pub before: Option<String>,
    /// 正面提示词：添加到原提示词之后
    #[serde(default)]
    pub after: Option<String>,
    /// 正面提示词：完全替换原提示词
    #[serde(default)]
    pub replace: Option<String>,
    /// 负面提示词：添加到原UC之前
    #[serde(default)]
    pub uc_before: Option<String>,
    /// 负面提示词：添加到原UC之后
    #[serde(default)]
    pub uc_after: Option<String>,
    /// 负面提示词：完全替换原UC
    #[serde(default)]
    pub uc_replace: Option<String>,
}

impl MainPresetSettings {
    pub fn is_empty(&self) -> bool {
        is_blank(&self.before)
            && is_blank(&self.after)
            && is_blank(&self.replace)
            && is_blank(&self.uc_before)
            && is_blank(&self.uc_after)
            && is_blank(&self.uc_replace)
    }

    /// 应用预设到正面提示词
    /// 规则: replace 非空白则直接替换；否则应用 before/after（非空白时）
    pub fn apply_positive(&self, raw_prompt: &str) -> String {
        // replace 优先级最高，且非空白时生效
        if !is_blank(&self.replace) {
            return self.replace.as_ref().unwrap().clone();
        }

        let mut result = String::new();
        // before 非空白时添加
        if !is_blank(&self.before) {
            result.push_str(self.before.as_ref().unwrap().trim());
            if !result.is_empty() && !result.trim().ends_with(',') {
                result.push_str(", ");
            }
        }
        result.push_str(raw_prompt);
        // after 非空白时添加
        if !is_blank(&self.after) {
            if !result.is_empty() && !result.trim().ends_with(',') {
                result.push_str(", ");
            }
            result.push_str(self.after.as_ref().unwrap().trim());
        }
        result
    }

    /// 应用预设到负面提示词
    /// 规则: replace 非空白则直接替换；否则应用 before/after（非空白时）
    pub fn apply_negative(&self, raw_uc: &str) -> String {
        // replace 优先级最高，且非空白时生效
        if !is_blank(&self.uc_replace) {
            return self.uc_replace.as_ref().unwrap().clone();
        }

        let mut result = String::new();
        // before 非空白时添加
        if !is_blank(&self.uc_before) {
            result.push_str(self.uc_before.as_ref().unwrap().trim());
            if !result.is_empty() && !result.trim().ends_with(',') {
                result.push_str(", ");
            }
        }
        result.push_str(raw_uc);
        // after 非空白时添加
        if !is_blank(&self.uc_after) {
            if !result.is_empty() && !result.trim().ends_with(',') {
                result.push_str(", ");
            }
            result.push_str(self.uc_after.as_ref().unwrap().trim());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_preset_settings_blank_skip() {
        let settings = MainPresetSettings {
            before: Some("   ".to_string()), // 空白，应跳过
            after: Some("quality".to_string()),
            replace: None,
            uc_before: None,
            uc_after: None,
            uc_replace: None,
        };

        let result = settings.apply_positive("test prompt");
        assert_eq!(result, "test prompt, quality");
    }

    #[test]
    fn test_main_preset_settings_replace_priority() {
        let settings = MainPresetSettings {
            before: Some("ignored".to_string()),
            after: Some("also ignored".to_string()),
            replace: Some("replacement".to_string()),
            uc_before: None,
            uc_after: None,
            uc_replace: None,
        };

        let result = settings.apply_positive("test prompt");
        assert_eq!(result, "replacement");
    }

    #[test]
    fn test_main_preset_settings_blank_replace_uses_before_after() {
        let settings = MainPresetSettings {
            before: Some("start".to_string()),
            after: Some("end".to_string()),
            replace: Some("  ".to_string()), // 空白，应跳过 replace
            uc_before: None,
            uc_after: None,
            uc_replace: None,
        };

        let result = settings.apply_positive("middle");
        assert_eq!(result, "start, middle, end");
    }

    #[test]
    fn test_character_preset_apply() {
        let mut preset = CharacterPreset::new("test".to_string());
        preset.before = Some("1girl".to_string());
        preset.after = Some("solo".to_string());

        let result = preset.apply("blue hair");
        assert_eq!(result, "1girl blue hair solo");
    }

    #[test]
    fn test_character_preset_replace_priority() {
        let mut preset = CharacterPreset::new("test".to_string());
        preset.before = Some("ignored".to_string());
        preset.after = Some("also ignored".to_string());
        preset.replace = Some("complete replacement".to_string());

        let result = preset.apply("original");
        assert_eq!(result, "complete replacement");
    }
}
