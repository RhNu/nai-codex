use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// 编译时嵌入单个词库文件
const EMBEDDED_LEXICON: &str = include_str!("../../../assets/lexicon.json");

/// 单个标签条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LexiconEntry {
    pub tag: String,
    pub zh: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u64>,
    pub category: String,
    pub subcategory: String,
}

/// 词库索引信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LexiconIndex {
    pub categories: Vec<CategoryInfo>,
    pub stats: LexiconStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    pub name: String,
    pub subcategories: Vec<String>,
    pub tag_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LexiconStats {
    pub total_tags: usize,
    pub categorized_tags: usize,
    pub uncategorized_tags: usize,
    pub matched_weights: usize,
}

/// 词库分类数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryData {
    pub name: String,
    pub subcategories: HashMap<String, Vec<LexiconEntry>>,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub entries: Vec<LexiconEntry>,
    pub total: usize,
}

/// 嵌入的 JSON 结构
#[derive(Debug, Deserialize)]
struct EmbeddedLexicon {
    categories: Vec<EmbeddedCategory>,
    stats: LexiconStats,
}

#[derive(Debug, Deserialize)]
struct EmbeddedCategory {
    name: String,
    subcategories: Vec<EmbeddedSubcategory>,
}

#[derive(Debug, Deserialize)]
struct EmbeddedSubcategory {
    name: String,
    tags: Vec<EmbeddedTag>,
}

#[derive(Debug, Deserialize)]
struct EmbeddedTag {
    tag: String,
    zh: String,
    #[serde(default)]
    weight: Option<u64>,
}

/// 词库管理器
pub struct Lexicon {
    /// 分类名 -> CategoryData
    categories: HashMap<String, CategoryData>,
    /// 所有条目的平面列表，用于搜索
    all_entries: Vec<LexiconEntry>,
    /// 索引信息
    index: LexiconIndex,
}

impl Lexicon {
    /// 从嵌入的数据加载词库（编译时嵌入）
    pub fn load_embedded() -> Result<Self> {
        let embedded: EmbeddedLexicon = serde_json::from_str(EMBEDDED_LEXICON)?;

        let mut categories = HashMap::new();
        let mut all_entries = Vec::new();
        let mut index_categories = Vec::new();

        for cat in embedded.categories {
            let mut subcategories: HashMap<String, Vec<LexiconEntry>> = HashMap::new();
            let mut subcat_names = Vec::new();
            let mut tag_count = 0;

            for subcat in cat.subcategories {
                subcat_names.push(subcat.name.clone());
                let entries: Vec<LexiconEntry> = subcat
                    .tags
                    .into_iter()
                    .map(|t| {
                        tag_count += 1;
                        LexiconEntry {
                            tag: t.tag,
                            zh: t.zh,
                            weight: t.weight,
                            category: cat.name.clone(),
                            subcategory: subcat.name.clone(),
                        }
                    })
                    .collect();

                all_entries.extend(entries.iter().cloned());
                subcategories.insert(subcat.name, entries);
            }

            index_categories.push(CategoryInfo {
                name: cat.name.clone(),
                subcategories: subcat_names,
                tag_count,
            });

            categories.insert(
                cat.name.clone(),
                CategoryData {
                    name: cat.name,
                    subcategories,
                },
            );
        }

        // 预排序所有条目（按权重高到低）
        all_entries.sort_by(|a, b| b.weight.unwrap_or(0).cmp(&a.weight.unwrap_or(0)));

        let index = LexiconIndex {
            categories: index_categories,
            stats: embedded.stats,
        };

        Ok(Self {
            categories,
            all_entries,
            index,
        })
    }

    /// 获取索引信息
    pub fn get_index(&self) -> &LexiconIndex {
        &self.index
    }

    /// 获取分类列表
    pub fn list_categories(&self) -> Vec<&CategoryInfo> {
        self.index.categories.iter().collect()
    }

    /// 获取某个分类的数据
    pub fn get_category(&self, name: &str) -> Option<&CategoryData> {
        self.categories.get(name)
    }

    /// 搜索标签
    /// 支持中英文搜索，返回匹配结果（按权重排序）
    pub fn search(&self, query: &str, limit: usize, offset: usize) -> SearchResult {
        let query_lower = query.to_lowercase();
        let query_normalized = query_lower.replace('_', " ");

        let mut matches: Vec<&LexiconEntry> = self
            .all_entries
            .iter()
            .filter(|entry| {
                let tag_normalized = entry.tag.to_lowercase().replace('_', " ");
                tag_normalized.contains(&query_normalized) || entry.zh.contains(&query_lower)
            })
            .collect();

        // 已按权重预排序，但精确匹配应优先
        matches.sort_by(|a, b| {
            let a_tag = a.tag.to_lowercase().replace('_', " ");
            let b_tag = b.tag.to_lowercase().replace('_', " ");

            // 精确匹配优先
            let a_exact = a_tag == query_normalized || a.zh == query_lower;
            let b_exact = b_tag == query_normalized || b.zh == query_lower;

            if a_exact != b_exact {
                return b_exact.cmp(&a_exact);
            }

            // 前缀匹配次之
            let a_prefix = a_tag.starts_with(&query_normalized) || a.zh.starts_with(&query_lower);
            let b_prefix = b_tag.starts_with(&query_normalized) || b.zh.starts_with(&query_lower);

            if a_prefix != b_prefix {
                return b_prefix.cmp(&a_prefix);
            }

            // 最后按权重
            b.weight.unwrap_or(0).cmp(&a.weight.unwrap_or(0))
        });

        let total = matches.len();
        let entries: Vec<LexiconEntry> = matches
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();

        SearchResult { entries, total }
    }
}
