//! NAI Prompt Parser - 解析 NovelAI 提示词语法
//!
//! NAI 支持的权重语法:
//! - `{tag}` - 增强 1.05 倍
//! - `{{tag}}` - 增强 1.05^2 倍，以此类推
//! - `[tag]` - 减弱，除以 1.05
//! - `[[tag]]` - 减弱，除以 1.05^2，以此类推
//! - `1.5::tag1, tag2 ::` - 冒号权重语法，乘以指定数值直到遇到 `::` 结束
//! - `//comment//` - 注释语法，双斜杠之间的内容被忽略
//! - 未闭合的 {} 或 [] 会影响后续所有提示词
//!
//! 提示词结构视为两层:
//! - 底层: 逗号分隔的提示词序列 (tags)
//! - 上层: 权重修饰层 (weight layer)

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 解析错误
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("未闭合的注释：在位置 {0} 处开始的注释没有结束符 '//'")]
    UnclosedComment(usize),
}

/// 权重倍数常量
const WEIGHT_MULTIPLIER: f64 = 1.05;

/// Token 类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Token {
    /// 普通文本 (tag)
    Text {
        value: String,
        /// 起始位置 (字节偏移)
        start: usize,
        /// 结束位置 (字节偏移)
        end: usize,
        /// 计算后的权重
        weight: f64,
    },
    /// 逗号分隔符
    Comma { start: usize, end: usize },
    /// 空白字符 (空格、换行等)
    Whitespace {
        value: String,
        start: usize,
        end: usize,
    },
    /// 增强标记 `{`
    BraceOpen {
        start: usize,
        end: usize,
        /// 当前深度 (开启后)
        depth: i32,
    },
    /// 增强结束 `}`
    BraceClose {
        start: usize,
        end: usize,
        /// 当前深度 (关闭后)
        depth: i32,
    },
    /// 减弱标记 `[`
    BracketOpen {
        start: usize,
        end: usize,
        /// 当前深度 (开启后)
        depth: i32,
    },
    /// 减弱结束 `]`
    BracketClose {
        start: usize,
        end: usize,
        /// 当前深度 (关闭后)
        depth: i32,
    },
    /// 冒号权重开始 `1.5::`
    WeightStart {
        value: f64,
        start: usize,
        end: usize,
    },
    /// 冒号权重结束 `::`
    WeightEnd { start: usize, end: usize },
    /// snippet 引用 `<snippet:name>`
    SnippetRef {
        name: String,
        start: usize,
        end: usize,
        weight: f64,
    },
    /// 换行符
    Newline { start: usize, end: usize },
    /// 注释 `//...//`
    Comment {
        value: String,
        start: usize,
        end: usize,
    },
}

impl Token {
    /// 获取 token 的起始位置
    pub fn start(&self) -> usize {
        match self {
            Token::Text { start, .. } => *start,
            Token::Comma { start, .. } => *start,
            Token::Whitespace { start, .. } => *start,
            Token::BraceOpen { start, .. } => *start,
            Token::BraceClose { start, .. } => *start,
            Token::BracketOpen { start, .. } => *start,
            Token::BracketClose { start, .. } => *start,
            Token::WeightStart { start, .. } => *start,
            Token::WeightEnd { start, .. } => *start,
            Token::SnippetRef { start, .. } => *start,
            Token::Newline { start, .. } => *start,
            Token::Comment { start, .. } => *start,
        }
    }

    /// 获取 token 的结束位置
    pub fn end(&self) -> usize {
        match self {
            Token::Text { end, .. } => *end,
            Token::Comma { end, .. } => *end,
            Token::Whitespace { end, .. } => *end,
            Token::BraceOpen { end, .. } => *end,
            Token::BraceClose { end, .. } => *end,
            Token::BracketOpen { end, .. } => *end,
            Token::BracketClose { end, .. } => *end,
            Token::WeightStart { end, .. } => *end,
            Token::WeightEnd { end, .. } => *end,
            Token::SnippetRef { end, .. } => *end,
            Token::Newline { end, .. } => *end,
            Token::Comment { end, .. } => *end,
        }
    }

    /// 获取当前 token 的有效权重 (仅对 Text 和 SnippetRef 有意义)
    pub fn weight(&self) -> Option<f64> {
        match self {
            Token::Text { weight, .. } => Some(*weight),
            Token::SnippetRef { weight, .. } => Some(*weight),
            _ => None,
        }
    }
}

/// 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    pub tokens: Vec<Token>,
    /// 是否有未闭合的括号
    pub unclosed_braces: i32,
    pub unclosed_brackets: i32,
    /// 是否有未结束的冒号权重
    pub unclosed_weight: bool,
}

/// 用于前端高亮的简化 span 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    /// 权重: 1.0 为正常, >1 为增强, <1 为减弱
    pub weight: f64,
    /// span 类型: "text", "brace", "bracket", "weight_num", "weight_end", "comma", "whitespace", "snippet", "newline", "comment"
    #[serde(rename = "type")]
    pub span_type: String,
}

/// 注释信息
#[derive(Debug, Clone)]
pub struct CommentSpan {
    pub start: usize,
    pub end: usize,
    pub content: String,
}

/// NAI 提示词解析器
pub struct PromptParser;

impl PromptParser {
    /// 剥离注释，返回处理后的字符串
    /// 如果有未闭合的注释，返回错误
    pub fn strip_comments(input: &str) -> Result<String, ParseError> {
        let mut result = String::with_capacity(input.len());
        let mut pos = 0;
        let bytes = input.as_bytes();
        let len = bytes.len();

        while pos < len {
            // 检查是否是注释开始 //
            if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
                let comment_start = pos;
                pos += 2; // 跳过开始的 //

                // 寻找结束的 //
                let mut found_end = false;
                while pos + 1 < len {
                    if bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
                        pos += 2; // 跳过结束的 //
                        found_end = true;
                        break;
                    }
                    pos += 1;
                }

                if !found_end {
                    return Err(ParseError::UnclosedComment(comment_start));
                }
            } else {
                // 普通字符
                result.push(input[pos..].chars().next().unwrap());
                pos += input[pos..].chars().next().unwrap().len_utf8();
            }
        }

        Ok(result)
    }

    /// 查找所有注释的位置（用于前端高亮）
    pub fn find_comments(input: &str) -> Vec<CommentSpan> {
        let mut comments = Vec::new();
        let mut pos = 0;
        let bytes = input.as_bytes();
        let len = bytes.len();

        while pos < len {
            // 检查是否是注释开始 //
            if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
                let comment_start = pos;
                pos += 2; // 跳过开始的 //
                let content_start = pos;

                // 寻找结束的 //
                while pos + 1 < len {
                    if bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
                        let content_end = pos;
                        pos += 2; // 跳过结束的 //
                        comments.push(CommentSpan {
                            start: comment_start,
                            end: pos,
                            content: input[content_start..content_end].to_string(),
                        });
                        break;
                    }
                    pos += 1;
                }
                // 如果没找到结束，pos 已经到末尾了，未闭合的注释不加入列表
            } else {
                pos += 1;
            }
        }

        comments
    }

    /// 解析提示词，返回 token 列表
    pub fn parse(input: &str) -> ParseResult {
        let mut tokens = Vec::new();
        let chars: Vec<(usize, char)> = input.char_indices().collect();
        let input_len = input.len();

        // 状态跟踪
        let mut brace_depth: i32 = 0; // {} 深度
        let mut bracket_depth: i32 = 0; // [] 深度
        let mut colon_weight: Option<f64> = None; // 当前冒号权重

        let mut pos = 0;

        while pos < chars.len() {
            let (byte_pos, ch) = chars[pos];

            // 检查注释 //...//
            if ch == '/' && pos + 1 < chars.len() && chars[pos + 1].1 == '/' {
                let comment_start = byte_pos;
                let mut comment_pos = pos + 2; // 跳过开始的 //
                let content_start = comment_pos;

                // 寻找结束的 //
                let mut found_end = false;
                while comment_pos + 1 < chars.len() {
                    if chars[comment_pos].1 == '/' && chars[comment_pos + 1].1 == '/' {
                        let content_end = comment_pos;
                        let comment_end = chars[comment_pos + 1].0 + 1; // 结束 // 的字节位置

                        // 提取注释内容
                        let content: String = chars[content_start..content_end]
                            .iter()
                            .map(|(_, c)| *c)
                            .collect();

                        tokens.push(Token::Comment {
                            value: content,
                            start: comment_start,
                            end: comment_end,
                        });

                        pos = comment_pos + 2;
                        found_end = true;
                        break;
                    }
                    comment_pos += 1;
                }

                // 如果找到了结束符，继续下一轮
                if found_end {
                    continue;
                }
                // 未闭合的注释，把 // 当作普通文本处理
                // 这里不报错，让 strip_comments 去处理错误
            }

            // 检查换行
            if ch == '\n' {
                tokens.push(Token::Newline {
                    start: byte_pos,
                    end: byte_pos + 1,
                });
                pos += 1;
                continue;
            }

            // 检查 \r\n
            if ch == '\r' {
                if pos + 1 < chars.len() && chars[pos + 1].1 == '\n' {
                    tokens.push(Token::Newline {
                        start: byte_pos,
                        end: chars[pos + 1].0 + 1,
                    });
                    pos += 2;
                } else {
                    tokens.push(Token::Newline {
                        start: byte_pos,
                        end: byte_pos + 1,
                    });
                    pos += 1;
                }
                continue;
            }

            // 检查冒号权重语法: `number::`
            if ch.is_ascii_digit() || ch == '-' || ch == '.' {
                if let Some((weight_val, consumed, end_byte)) =
                    Self::try_parse_weight_start(&chars, pos, input)
                {
                    tokens.push(Token::WeightStart {
                        value: weight_val,
                        start: byte_pos,
                        end: end_byte,
                    });
                    colon_weight = Some(weight_val);
                    pos += consumed;
                    continue;
                }
            }

            // 检查权重结束 `::`
            if ch == ':' && pos + 1 < chars.len() && chars[pos + 1].1 == ':' {
                // 检查这不是权重开始 (前面没有数字)
                let is_weight_end = colon_weight.is_some();
                if is_weight_end {
                    let end_byte = if pos + 1 < chars.len() {
                        chars[pos + 1].0 + chars[pos + 1].1.len_utf8()
                    } else {
                        input_len
                    };
                    tokens.push(Token::WeightEnd {
                        start: byte_pos,
                        end: end_byte,
                    });
                    colon_weight = None;
                    pos += 2;
                    continue;
                }
            }

            // 检查 `{`
            if ch == '{' {
                brace_depth += 1;
                tokens.push(Token::BraceOpen {
                    start: byte_pos,
                    end: byte_pos + 1,
                    depth: brace_depth,
                });
                pos += 1;
                continue;
            }

            // 检查 `}`
            if ch == '}' {
                brace_depth = (brace_depth - 1).max(0);
                tokens.push(Token::BraceClose {
                    start: byte_pos,
                    end: byte_pos + 1,
                    depth: brace_depth,
                });
                pos += 1;
                continue;
            }

            // 检查 `[`
            if ch == '[' {
                bracket_depth += 1;
                tokens.push(Token::BracketOpen {
                    start: byte_pos,
                    end: byte_pos + 1,
                    depth: bracket_depth,
                });
                pos += 1;
                continue;
            }

            // 检查 `]`
            if ch == ']' {
                bracket_depth = (bracket_depth - 1).max(0);
                tokens.push(Token::BracketClose {
                    start: byte_pos,
                    end: byte_pos + 1,
                    depth: bracket_depth,
                });
                pos += 1;
                continue;
            }

            // 检查逗号
            if ch == ',' {
                tokens.push(Token::Comma {
                    start: byte_pos,
                    end: byte_pos + 1,
                });
                pos += 1;
                continue;
            }

            // 检查空白
            if ch.is_whitespace() {
                let start = byte_pos;
                let mut end = byte_pos + ch.len_utf8();
                let mut ws = String::new();
                ws.push(ch);
                pos += 1;

                while pos < chars.len() {
                    let (next_byte, next_ch) = chars[pos];
                    if next_ch.is_whitespace() && next_ch != '\n' && next_ch != '\r' {
                        ws.push(next_ch);
                        end = next_byte + next_ch.len_utf8();
                        pos += 1;
                    } else {
                        break;
                    }
                }

                tokens.push(Token::Whitespace {
                    value: ws,
                    start,
                    end,
                });
                continue;
            }

            // 检查 snippet 引用: `<snippet:name>`
            if ch == '<' {
                if let Some((name, consumed, end_byte)) =
                    Self::try_parse_snippet_ref(&chars, pos, input)
                {
                    let weight = Self::calculate_weight(brace_depth, bracket_depth, colon_weight);
                    tokens.push(Token::SnippetRef {
                        name,
                        start: byte_pos,
                        end: end_byte,
                        weight,
                    });
                    pos += consumed;
                    continue;
                }
            }

            // 普通文本 - 收集直到遇到特殊字符
            let text_start = byte_pos;
            let mut text = String::new();
            let mut text_end = byte_pos;

            while pos < chars.len() {
                let (b, c) = chars[pos];
                if c == '{'
                    || c == '}'
                    || c == '['
                    || c == ']'
                    || c == ','
                    || c == '\n'
                    || c == '\r'
                    || c == '<'
                    || (c == ':' && pos + 1 < chars.len() && chars[pos + 1].1 == ':')
                    || (c == '/' && pos + 1 < chars.len() && chars[pos + 1].1 == '/')
                {
                    break;
                }
                // 检查是否是权重开始
                if c.is_ascii_digit() || c == '-' || c == '.' {
                    if Self::try_parse_weight_start(&chars, pos, input).is_some() {
                        break;
                    }
                }
                text.push(c);
                text_end = b + c.len_utf8();
                pos += 1;
            }

            if !text.is_empty() {
                let weight = Self::calculate_weight(brace_depth, bracket_depth, colon_weight);
                tokens.push(Token::Text {
                    value: text,
                    start: text_start,
                    end: text_end,
                    weight,
                });
            }
        }

        ParseResult {
            tokens,
            unclosed_braces: brace_depth,
            unclosed_brackets: bracket_depth,
            unclosed_weight: colon_weight.is_some(),
        }
    }

    /// 计算当前权重
    fn calculate_weight(brace_depth: i32, bracket_depth: i32, colon_weight: Option<f64>) -> f64 {
        let mut weight = 1.0;

        // 应用 {} 增强
        if brace_depth > 0 {
            weight *= WEIGHT_MULTIPLIER.powi(brace_depth);
        }

        // 应用 [] 减弱
        if bracket_depth > 0 {
            weight /= WEIGHT_MULTIPLIER.powi(bracket_depth);
        }

        // 应用冒号权重
        if let Some(w) = colon_weight {
            weight *= w;
        }

        weight
    }

    /// 尝试解析冒号权重开始语法 `number::`
    /// 返回 (权重值, 消耗的字符数, 结束字节位置)
    fn try_parse_weight_start(
        chars: &[(usize, char)],
        start: usize,
        _input: &str,
    ) -> Option<(f64, usize, usize)> {
        let mut pos = start;
        let mut num_str = String::new();

        // 可选负号
        if pos < chars.len() && chars[pos].1 == '-' {
            num_str.push('-');
            pos += 1;
        }

        // 收集数字部分
        let mut has_digit = false;
        let mut has_dot = false;

        while pos < chars.len() {
            let ch = chars[pos].1;
            if ch.is_ascii_digit() {
                num_str.push(ch);
                has_digit = true;
                pos += 1;
            } else if ch == '.' && !has_dot {
                num_str.push(ch);
                has_dot = true;
                pos += 1;
            } else {
                break;
            }
        }

        if !has_digit {
            return None;
        }

        // 检查是否有 `::`
        if pos + 1 < chars.len() && chars[pos].1 == ':' && chars[pos + 1].1 == ':' {
            let weight: f64 = num_str.parse().ok()?;
            let end_byte = chars[pos + 1].0 + 1; // `::` 的结束位置
            Some((weight, pos - start + 2, end_byte))
        } else {
            None
        }
    }

    /// 尝试解析 snippet 引用 `<snippet:name>`
    fn try_parse_snippet_ref(
        chars: &[(usize, char)],
        start: usize,
        _input: &str,
    ) -> Option<(String, usize, usize)> {
        // 检查 `<snippet:`
        let prefix = "<snippet:";
        let mut pos = start;

        for expected in prefix.chars() {
            if pos >= chars.len() || chars[pos].1 != expected {
                return None;
            }
            pos += 1;
        }

        // 收集名称直到 `>`
        let mut name = String::new();
        while pos < chars.len() {
            let (byte_pos, ch) = chars[pos];
            if ch == '>' {
                let end_byte = byte_pos + 1;
                return Some((name, pos - start + 1, end_byte));
            }
            if ch == '<' || ch == '\n' {
                // 无效的 snippet 引用
                return None;
            }
            name.push(ch);
            pos += 1;
        }

        None
    }

    /// 将 tokens 转换为前端高亮所需的 spans
    pub fn to_highlight_spans(result: &ParseResult) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();

        for token in &result.tokens {
            match token {
                Token::Text {
                    start, end, weight, ..
                } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: *weight,
                        span_type: "text".to_string(),
                    });
                }
                Token::Comma { start, end } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: 1.0,
                        span_type: "comma".to_string(),
                    });
                }
                Token::Whitespace { start, end, .. } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: 1.0,
                        span_type: "whitespace".to_string(),
                    });
                }
                Token::BraceOpen { start, end, depth } => {
                    let weight = WEIGHT_MULTIPLIER.powi(*depth);
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight,
                        span_type: "brace".to_string(),
                    });
                }
                Token::BraceClose { start, end, depth } => {
                    // 关闭后的深度，所以显示关闭前的权重
                    let weight = WEIGHT_MULTIPLIER.powi(*depth + 1);
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight,
                        span_type: "brace".to_string(),
                    });
                }
                Token::BracketOpen { start, end, depth } => {
                    let weight = 1.0 / WEIGHT_MULTIPLIER.powi(*depth);
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight,
                        span_type: "bracket".to_string(),
                    });
                }
                Token::BracketClose { start, end, depth } => {
                    let weight = 1.0 / WEIGHT_MULTIPLIER.powi(*depth + 1);
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight,
                        span_type: "bracket".to_string(),
                    });
                }
                Token::WeightStart { value, start, end } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: *value,
                        span_type: "weight_num".to_string(),
                    });
                }
                Token::WeightEnd { start, end } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: 1.0,
                        span_type: "weight_end".to_string(),
                    });
                }
                Token::SnippetRef {
                    start, end, weight, ..
                } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: *weight,
                        span_type: "snippet".to_string(),
                    });
                }
                Token::Newline { start, end } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: 1.0,
                        span_type: "newline".to_string(),
                    });
                }
                Token::Comment { start, end, .. } => {
                    spans.push(HighlightSpan {
                        start: *start,
                        end: *end,
                        weight: 1.0,
                        span_type: "comment".to_string(),
                    });
                }
            }
        }

        spans
    }

    /// 格式化提示词
    /// - 逗号后添加空格
    /// - 权重结束 `::` 前添加空格
    /// - 限制连续空行最多 2 行
    pub fn format(input: &str) -> String {
        let result = Self::parse(input);
        let mut output = String::with_capacity(input.len());
        let mut consecutive_newlines = 0;
        let mut prev_token: Option<&Token> = None;

        for token in &result.tokens {
            match token {
                Token::Newline { .. } => {
                    consecutive_newlines += 1;
                    if consecutive_newlines <= 2 {
                        output.push('\n');
                    }
                }
                Token::Comma { .. } => {
                    consecutive_newlines = 0;
                    output.push(',');
                    // 逗号后添加空格 (如果下一个不是空白或换行)
                }
                Token::Whitespace { value, .. } => {
                    // 如果前一个是逗号，确保有空格
                    if let Some(Token::Comma { .. }) = prev_token {
                        if !value.starts_with(' ') {
                            output.push(' ');
                        }
                    }
                    // 只保留单个空格，除非是换行后的缩进
                    if consecutive_newlines > 0 {
                        output.push_str(value);
                    } else {
                        output.push(' ');
                    }
                    consecutive_newlines = 0;
                }
                Token::WeightEnd { .. } => {
                    consecutive_newlines = 0;
                    // 权重结束前添加空格
                    if !output.ends_with(' ') && !output.ends_with('\n') {
                        output.push(' ');
                    }
                    output.push_str("::");
                }
                Token::Text { value, .. } => {
                    consecutive_newlines = 0;
                    // 如果前一个是逗号且没有空格，添加空格
                    if let Some(Token::Comma { .. }) = prev_token {
                        if !output.ends_with(' ') {
                            output.push(' ');
                        }
                    }
                    output.push_str(value);
                }
                Token::BraceOpen { .. } => {
                    consecutive_newlines = 0;
                    output.push('{');
                }
                Token::BraceClose { .. } => {
                    consecutive_newlines = 0;
                    output.push('}');
                }
                Token::BracketOpen { .. } => {
                    consecutive_newlines = 0;
                    output.push('[');
                }
                Token::BracketClose { .. } => {
                    consecutive_newlines = 0;
                    output.push(']');
                }
                Token::WeightStart { value, .. } => {
                    consecutive_newlines = 0;
                    // 格式化数字
                    if *value == value.floor() {
                        output.push_str(&format!("{}::", *value as i64));
                    } else {
                        output.push_str(&format!("{}::", value));
                    }
                }
                Token::SnippetRef { name, .. } => {
                    consecutive_newlines = 0;
                    if let Some(Token::Comma { .. }) = prev_token {
                        if !output.ends_with(' ') {
                            output.push(' ');
                        }
                    }
                    output.push_str(&format!("<snippet:{}>", name));
                }
                Token::Comment { value, .. } => {
                    // 保留注释原样
                    consecutive_newlines = 0;
                    output.push_str(&format!("//{}//", value));
                }
            }
            prev_token = Some(token);
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parse() {
        let input = "1girl, blue hair";
        let result = PromptParser::parse(input);
        assert!(!result.tokens.is_empty());
    }

    #[test]
    fn test_brace_weight() {
        let input = "{strong}";
        let result = PromptParser::parse(input);

        // 查找 text token
        let text_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Text { .. }));
        assert!(text_token.is_some());

        if let Some(Token::Text { weight, .. }) = text_token {
            assert!((*weight - 1.05).abs() < 0.001);
        }
    }

    #[test]
    fn test_bracket_weight() {
        let input = "[weak]";
        let result = PromptParser::parse(input);

        let text_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Text { .. }));
        assert!(text_token.is_some());

        if let Some(Token::Text { weight, .. }) = text_token {
            assert!((*weight - 1.0 / 1.05).abs() < 0.001);
        }
    }

    #[test]
    fn test_colon_weight() {
        let input = "1.5::strong tag ::";
        let result = PromptParser::parse(input);

        let text_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Text { .. }));
        assert!(text_token.is_some());

        if let Some(Token::Text { weight, .. }) = text_token {
            assert!((*weight - 1.5).abs() < 0.001);
        }
    }

    #[test]
    fn test_nested_braces() {
        let input = "{{very strong}}";
        let result = PromptParser::parse(input);

        let text_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Text { .. }));
        assert!(text_token.is_some());

        if let Some(Token::Text { weight, .. }) = text_token {
            let expected = 1.05_f64.powi(2);
            assert!((*weight - expected).abs() < 0.001);
        }
    }

    #[test]
    fn test_format() {
        let input = "1girl,blue hair,  {strong}";
        let formatted = PromptParser::format(input);
        assert!(formatted.contains(", "));
    }

    #[test]
    fn test_snippet_ref() {
        let input = "1girl, <snippet:my_style>";
        let result = PromptParser::parse(input);

        let snippet_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::SnippetRef { .. }));
        assert!(snippet_token.is_some());

        if let Some(Token::SnippetRef { name, .. }) = snippet_token {
            assert_eq!(name, "my_style");
        }
    }

    #[test]
    fn test_snippet_with_chinese() {
        // 测试包含中文的 snippet 名称
        let input = "<snippet:画风/粗糙线条>, at night, <snippet:粗糙/saaa>";
        let result = PromptParser::parse(input);
        let spans = PromptParser::to_highlight_spans(&result);

        // 验证所有 snippet 都被正确解析
        let snippet_spans: Vec<_> = spans.iter().filter(|s| s.span_type == "snippet").collect();
        assert_eq!(snippet_spans.len(), 2);

        // 验证位置正确
        assert_eq!(
            &input[snippet_spans[0].start..snippet_spans[0].end],
            "<snippet:画风/粗糙线条>"
        );
        assert_eq!(
            &input[snippet_spans[1].start..snippet_spans[1].end],
            "<snippet:粗糙/saaa>"
        );
    }

    #[test]
    fn test_comment_basic() {
        // 测试基本注释
        let input = "1girl, //this is a comment//, blue hair";
        let result = PromptParser::parse(input);

        let comment_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Comment { .. }));
        assert!(comment_token.is_some());

        if let Some(Token::Comment { value, .. }) = comment_token {
            assert_eq!(value, "this is a comment");
        }
    }

    #[test]
    fn test_comment_multiline() {
        // 测试多行注释
        let input = "1girl, //line1\nline2//, blue hair";
        let result = PromptParser::parse(input);

        let comment_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Comment { .. }));
        assert!(comment_token.is_some());

        if let Some(Token::Comment { value, .. }) = comment_token {
            assert_eq!(value, "line1\nline2");
        }
    }

    #[test]
    fn test_strip_comments() {
        // 测试剥离注释
        let input = "1girl, //comment//, blue hair";
        let result = PromptParser::strip_comments(input).unwrap();
        assert_eq!(result, "1girl, , blue hair");
    }

    #[test]
    fn test_strip_comments_multiple() {
        // 测试剥离多个注释
        let input = "//c1// hello //c2// world";
        let result = PromptParser::strip_comments(input).unwrap();
        assert_eq!(result, " hello  world");
    }

    #[test]
    fn test_strip_comments_unclosed() {
        // 测试未闭合注释
        let input = "1girl, //unclosed comment";
        let result = PromptParser::strip_comments(input);
        assert!(result.is_err());

        if let Err(ParseError::UnclosedComment(pos)) = result {
            assert_eq!(pos, 7); // 注释开始位置
        }
    }

    #[test]
    fn test_comment_with_special_chars() {
        // 测试注释内包含特殊字符
        let input = "1girl, //{special} [chars] 1.5:://, blue hair";
        let result = PromptParser::strip_comments(input).unwrap();
        assert_eq!(result, "1girl, , blue hair");

        // 解析应该忽略注释内的语法
        let parse_result = PromptParser::parse(input);
        let comment_spans: Vec<_> = parse_result
            .tokens
            .iter()
            .filter(|t| matches!(t, Token::Comment { .. }))
            .collect();
        assert_eq!(comment_spans.len(), 1);
    }

    #[test]
    fn test_find_comments() {
        let input = "hello //comment1// world //comment2//";
        let comments = PromptParser::find_comments(input);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].content, "comment1");
        assert_eq!(comments[1].content, "comment2");
    }

    #[test]
    fn test_single_slash_not_comment() {
        // 测试单斜杠不会被识别为注释
        let input = "1girl, a/b, c / d, path/to/file";
        let result = PromptParser::parse(input);

        // 不应该有任何注释 token
        let comment_tokens: Vec<_> = result
            .tokens
            .iter()
            .filter(|t| matches!(t, Token::Comment { .. }))
            .collect();
        assert_eq!(comment_tokens.len(), 0);

        // strip_comments 应该返回原始字符串（因为没有注释）
        let stripped = PromptParser::strip_comments(input).unwrap();
        assert_eq!(stripped, input);
    }

    #[test]
    fn test_triple_slash() {
        // 测试三个斜杠的情况：应该被识别为注释开始+一个斜杠内容
        let input = "hello ///content// world";
        let result = PromptParser::parse(input);

        let comment_token = result
            .tokens
            .iter()
            .find(|t| matches!(t, Token::Comment { .. }));
        assert!(comment_token.is_some(), "Should find a comment token");

        if let Some(Token::Comment { value, .. }) = comment_token {
            // /// 开始，// 结束，内容是 /content
            assert_eq!(value, "/content");
        }
    }
}
