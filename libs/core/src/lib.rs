use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Utc};
use codex_api::{CharacterPrompt, ImageGenerationRequest, Model, NaiClient, Noise, Sampler};
use rand::{Rng, rng};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

pub mod prompt_parser;
pub use prompt_parser::{HighlightSpan, ParseResult, PromptParser, Token};

pub mod lexicon;
pub use lexicon::{
    CategoryData, CategoryInfo, Lexicon, LexiconEntry, LexiconIndex, LexiconStats,
    SearchResult as LexiconSearchResult,
};

const TABLE_SNIPPETS: TableDefinition<Uuid, String> = TableDefinition::new("snippets");
const TABLE_SNIPPET_NAME_INDEX: TableDefinition<String, Uuid> =
    TableDefinition::new("snippets_by_name");
const TABLE_PRESETS: TableDefinition<Uuid, String> = TableDefinition::new("character_presets");
const TABLE_RECORDS: TableDefinition<Uuid, String> = TableDefinition::new("generation_records");
const TABLE_SETTINGS: TableDefinition<&str, String> = TableDefinition::new("settings");
const SETTINGS_KEY_LAST_GENERATION: &str = "last_generation";

pub type CoreResult<T> = Result<T>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub tags: Vec<String>,
    pub description: Option<String>,
    /// 预览图文件名（存储在 preview_dir 中）
    pub preview_path: Option<String>,
    pub content: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

impl Snippet {
    pub fn new(name: String, category: String, content: String) -> CoreResult<Self> {
        validate_snippet_name(&name)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            category,
            tags: Vec::new(),
            description: None,
            preview_path: None,
            content,
            created_at: now,
            updated_at: now,
        })
    }
}

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
    pub fn apply_uc(&self, raw_uc: &str) -> String {
        if let Some(replace) = &self.uc_replace {
            return replace.clone();
        }

        let mut result = String::new();
        if let Some(before) = &self.uc_before {
            result.push_str(before);
            if !result.ends_with(' ') && !result.ends_with(',') {
                result.push_str(", ");
            }
        }
        result.push_str(raw_uc);
        if let Some(after) = &self.uc_after {
            if !result.ends_with(' ') && !result.ends_with(',') {
                result.push_str(", ");
            }
            result.push_str(after);
        }
        result
    }

    /// Apply preset to raw prompt before snippet expansion.
    pub fn apply(&self, raw_prompt: &str) -> String {
        if let Some(replace) = &self.replace {
            return replace.clone();
        }

        let mut result = String::new();
        if let Some(before) = &self.before {
            result.push_str(before);
            if !result.ends_with(' ') {
                result.push(' ');
            }
        }
        result.push_str(raw_prompt);
        if let Some(after) = &self.after {
            if !result.ends_with(' ') {
                result.push(' ');
            }
            result.push_str(after);
        }
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryImage {
    pub path: PathBuf,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRecord {
    pub id: Uuid,
    pub task_id: Uuid,
    pub created_at: chrono::DateTime<Utc>,
    /// Raw prompt before snippet injection.
    pub raw_prompt: String,
    /// Prompt after preset + snippet expansion (for debug only).
    pub expanded_prompt: String,
    pub negative_prompt: String,
    pub images: Vec<GalleryImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GenerationParams {
    pub model: Model,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub scale: f32,
    pub sampler: Sampler,
    pub noise: Noise,
    pub cfg_rescale: f32,
    pub undesired_content_preset: Option<u8>,
    pub add_quality_tags: bool,
    pub character_prompts: Option<Vec<CharacterPrompt>>,
    /// Fixed seed for reproducibility. None or negative means random.
    pub seed: Option<i64>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            model: Model::default(),
            width: 1024,
            height: 1024,
            steps: 28,
            scale: 5.0,
            sampler: Sampler::default(),
            noise: Noise::default(),
            cfg_rescale: 0.0,
            undesired_content_preset: None,
            add_quality_tags: true,
            character_prompts: None,
            seed: None,
        }
    }
}

/// 角色槽设置，用于保存角色提示词
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterSlotSettings {
    pub prompt: String,
    pub uc: String,
    pub enabled: bool,
    pub preset_id: Option<Uuid>,
}

/// 保存上次生成页面的设置，用于下次打开时恢复
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LastGenerationSettings {
    pub prompt: String,
    pub negative_prompt: String,
    pub count: u32,
    pub params: GenerationParams,
    #[serde(default)]
    pub character_slots: Vec<CharacterSlotSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateTaskRequest {
    pub id: Uuid,
    pub raw_prompt: String,
    pub negative_prompt: String,
    /// How many images to generate sequentially.
    pub count: u32,
    pub params: GenerationParams,
    pub preset: Option<CharacterPreset>,
}

impl GenerateTaskRequest {
    pub fn new(raw_prompt: String, negative_prompt: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            raw_prompt,
            negative_prompt,
            count: 1,
            params: GenerationParams::default(),
            preset: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GalleryPaths {
    pub root: PathBuf,
}

impl GalleryPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Build path as YYYY-MM-DD/{number}_{seed}.png
    pub fn image_path(&self, index: u32, seed: u64) -> PathBuf {
        let today = Utc::now();
        let date_dir = format!(
            "{:04}-{:02}-{:02}",
            today.year(),
            today.month(),
            today.day()
        );
        self.root
            .join(date_dir)
            .join(format!("{}_{}.png", index, seed))
    }
}

#[derive(Debug, Clone)]
pub struct CoreStorage {
    db: Arc<Database>,
    preview_dir: PathBuf,
}

impl CoreStorage {
    pub fn open(db_path: impl AsRef<Path>, preview_dir: impl AsRef<Path>) -> CoreResult<Self> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).context("create db parent dir")?;
        }
        fs::create_dir_all(&preview_dir).context("create preview dir")?;
        let db = Database::create(db_path).context("open redb database")?;

        // Ensure all tables exist so read transactions never fail on first use
        {
            let write_txn = db.begin_write()?;
            {
                write_txn.open_table(TABLE_SNIPPETS)?;
                write_txn.open_table(TABLE_SNIPPET_NAME_INDEX)?;
                write_txn.open_table(TABLE_PRESETS)?;
                write_txn.open_table(TABLE_RECORDS)?;
                write_txn.open_table(TABLE_SETTINGS)?;
            }
            write_txn.commit()?;
        }

        let str_db_path = db_path.to_str().unwrap_or("unknown");
        let str_preview_dir = preview_dir.as_ref().to_str().unwrap_or("unknown");
        info!(?str_db_path, ?str_preview_dir, "core storage opened");
        Ok(Self {
            db: Arc::new(db),
            preview_dir: preview_dir.as_ref().to_path_buf(),
        })
    }

    pub fn upsert_snippet(
        &self,
        mut snippet: Snippet,
        preview_bytes: Option<&[u8]>,
    ) -> CoreResult<Snippet> {
        validate_snippet_name(&snippet.name)?;
        snippet.updated_at = Utc::now();

        if let Some(bytes) = preview_bytes {
            let preview_filename = format!("{}.png", snippet.id);
            let preview_path = self.preview_dir.join(&preview_filename);
            fs::write(&preview_path, bytes).context("write snippet preview")?;
            snippet.preview_path = Some(preview_filename);
        }

        // 获取旧的名称以便更新索引
        let old_name = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(TABLE_SNIPPETS)?;
            if let Some(value) = table.get(snippet.id)? {
                let old: Snippet = serde_json::from_str(&value.value())?;
                Some(old.name)
            } else {
                None
            }
        };

        let serialized = serde_json::to_string(&snippet)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SNIPPETS)?;
            table.insert(snippet.id, serialized)?;

            let mut index = write_txn.open_table(TABLE_SNIPPET_NAME_INDEX)?;

            // 检查新名称是否已被其他 snippet 使用
            if let Some(existing) = index.get(snippet.name.clone())? {
                let existing_id = existing.value();
                if existing_id != snippet.id {
                    return Err(anyhow!("snippet name already exists"));
                }
            }

            // 如果是重命名，删除旧的索引条目
            if let Some(old) = &old_name {
                if old != &snippet.name {
                    index.remove(old.clone())?;
                }
            }

            index.insert(snippet.name.clone(), snippet.id)?;
        }
        write_txn.commit()?;
        info!(id=%snippet.id, name=%snippet.name, "snippet upserted");
        Ok(snippet)
    }

    /// 重命名 snippet
    pub fn rename_snippet(&self, id: Uuid, new_name: String) -> CoreResult<Snippet> {
        validate_snippet_name(&new_name)?;

        let mut snippet = self
            .get_snippet(id)?
            .ok_or_else(|| anyhow!("snippet not found"))?;

        let old_name = snippet.name.clone();
        snippet.name = new_name.clone();
        snippet.updated_at = Utc::now();

        let serialized = serde_json::to_string(&snippet)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SNIPPETS)?;
            let mut index = write_txn.open_table(TABLE_SNIPPET_NAME_INDEX)?;

            // 检查新名称是否已被使用
            if let Some(existing) = index.get(new_name.clone())? {
                let existing_id = existing.value();
                if existing_id != snippet.id {
                    return Err(anyhow!("snippet name already exists"));
                }
            }

            // 更新数据和索引
            table.insert(snippet.id, serialized)?;
            index.remove(old_name.clone())?;
            index.insert(new_name.clone(), snippet.id)?;
        }
        write_txn.commit()?;
        info!(id=%snippet.id, old_name=%old_name, new_name=%new_name, "snippet renamed");
        Ok(snippet)
    }

    pub fn get_snippet_by_name(&self, name: &str) -> CoreResult<Option<Snippet>> {
        let read_txn = self.db.begin_read()?;
        let index = read_txn.open_table(TABLE_SNIPPET_NAME_INDEX)?;
        if let Some(id) = index.get(name.to_string())? {
            let id = id.value();
            let table = read_txn.open_table(TABLE_SNIPPETS)?;
            if let Some(value) = table.get(id)? {
                let snippet: Snippet = serde_json::from_str(&value.value())?;
                return Ok(Some(snippet));
            }
        }
        Ok(None)
    }

    pub fn upsert_preset(&self, preset: CharacterPreset) -> CoreResult<CharacterPreset> {
        let serialized = serde_json::to_string(&preset)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PRESETS)?;
            table.insert(preset.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%preset.id, name=%preset.name, "preset upserted");
        Ok(preset)
    }

    /// 创建或更新 preset 并可选保存预览图
    pub fn upsert_preset_with_preview(
        &self,
        mut preset: CharacterPreset,
        preview_bytes: Option<&[u8]>,
    ) -> CoreResult<CharacterPreset> {
        if let Some(bytes) = preview_bytes {
            let preview_filename = format!("preset_{}.png", preset.id);
            let preview_path = self.preview_dir.join(&preview_filename);
            fs::write(&preview_path, bytes).context("write preset preview")?;
            preset.preview_path = Some(preview_filename);
        }

        let serialized = serde_json::to_string(&preset)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PRESETS)?;
            table.insert(preset.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%preset.id, name=%preset.name, "preset upserted");
        Ok(preset)
    }

    /// 重命名 preset
    pub fn rename_preset(&self, id: Uuid, new_name: String) -> CoreResult<CharacterPreset> {
        let mut preset = self
            .get_preset(id)?
            .ok_or_else(|| anyhow!("preset not found"))?;

        let old_name = preset.name.clone();
        preset.name = new_name.clone();
        preset.updated_at = Utc::now();

        let serialized = serde_json::to_string(&preset)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PRESETS)?;
            table.insert(preset.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%preset.id, old_name=%old_name, new_name=%new_name, "preset renamed");
        Ok(preset)
    }

    pub fn get_preset(&self, id: Uuid) -> CoreResult<Option<CharacterPreset>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_PRESETS)?;
        if let Some(value) = table.get(id)? {
            let preset: CharacterPreset = serde_json::from_str(&value.value())?;
            return Ok(Some(preset));
        }
        Ok(None)
    }

    pub fn delete_preset(&self, id: Uuid) -> CoreResult<bool> {
        // First read the preset to get its preview path
        let preview_path = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(TABLE_PRESETS)?;
            if let Some(value) = table.get(id)? {
                let preset: CharacterPreset = serde_json::from_str(&value.value())?;
                preset.preview_path
            } else {
                return Ok(false);
            }
        };

        let write_txn = self.db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(TABLE_PRESETS)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;

        // Remove preview file if exists
        if let Some(path) = preview_path {
            let full_path = self.preview_dir.join(path);
            let _ = fs::remove_file(full_path);
        }

        if removed {
            info!(id=%id, "preset deleted");
        }
        Ok(removed)
    }

    /// 更新 preset 的预览图
    pub fn update_preset_preview(
        &self,
        id: Uuid,
        preview_bytes: &[u8],
    ) -> CoreResult<CharacterPreset> {
        let mut preset = self
            .get_preset(id)?
            .ok_or_else(|| anyhow!("preset not found"))?;

        let preview_filename = format!("preset_{}.png", preset.id);
        let preview_path = self.preview_dir.join(&preview_filename);
        fs::write(&preview_path, preview_bytes).context("write preset preview")?;
        preset.preview_path = Some(preview_filename);
        preset.updated_at = Utc::now();

        let serialized = serde_json::to_string(&preset)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PRESETS)?;
            table.insert(preset.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%preset.id, "preset preview updated");
        Ok(preset)
    }

    /// 删除 preset 的预览图
    pub fn delete_preset_preview(&self, id: Uuid) -> CoreResult<CharacterPreset> {
        let mut preset = self
            .get_preset(id)?
            .ok_or_else(|| anyhow!("preset not found"))?;

        if let Some(path) = &preset.preview_path {
            let full_path = self.preview_dir.join(path);
            let _ = fs::remove_file(full_path);
        }
        preset.preview_path = None;
        preset.updated_at = Utc::now();

        let serialized = serde_json::to_string(&preset)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PRESETS)?;
            table.insert(preset.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%preset.id, "preset preview deleted");
        Ok(preset)
    }

    pub fn get_snippet(&self, id: Uuid) -> CoreResult<Option<Snippet>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_SNIPPETS)?;
        if let Some(value) = table.get(id)? {
            let snippet: Snippet = serde_json::from_str(&value.value())?;
            return Ok(Some(snippet));
        }
        Ok(None)
    }

    pub fn delete_snippet(&self, id: Uuid) -> CoreResult<bool> {
        // First read the snippet to get its name and preview path
        let snippet_data = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(TABLE_SNIPPETS)?;
            if let Some(value) = table.get(id)? {
                let snippet: Snippet = serde_json::from_str(&value.value())?;
                Some((snippet.name, snippet.preview_path))
            } else {
                None
            }
        };

        let Some((name, preview_path)) = snippet_data else {
            return Ok(false);
        };

        // Now delete from tables
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SNIPPETS)?;
            table.remove(id)?;
            let mut index = write_txn.open_table(TABLE_SNIPPET_NAME_INDEX)?;
            index.remove(name)?;
        }
        write_txn.commit()?;

        // Remove preview file if exists
        if let Some(path) = preview_path {
            let full_path = self.preview_dir.join(path);
            let _ = fs::remove_file(full_path);
        }

        info!(id=%id, "snippet deleted");
        Ok(true)
    }

    /// 更新 snippet 的预览图
    pub fn update_snippet_preview(&self, id: Uuid, preview_bytes: &[u8]) -> CoreResult<Snippet> {
        let mut snippet = self
            .get_snippet(id)?
            .ok_or_else(|| anyhow!("snippet not found"))?;

        let preview_filename = format!("{}.png", snippet.id);
        let preview_path = self.preview_dir.join(&preview_filename);
        fs::write(&preview_path, preview_bytes).context("write snippet preview")?;
        snippet.preview_path = Some(preview_filename);
        snippet.updated_at = Utc::now();

        let serialized = serde_json::to_string(&snippet)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SNIPPETS)?;
            table.insert(snippet.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%snippet.id, "snippet preview updated");
        Ok(snippet)
    }

    /// 删除 snippet 的预览图
    pub fn delete_snippet_preview(&self, id: Uuid) -> CoreResult<Snippet> {
        let mut snippet = self
            .get_snippet(id)?
            .ok_or_else(|| anyhow!("snippet not found"))?;

        if let Some(path) = &snippet.preview_path {
            let full_path = self.preview_dir.join(path);
            let _ = fs::remove_file(full_path);
        }
        snippet.preview_path = None;
        snippet.updated_at = Utc::now();

        let serialized = serde_json::to_string(&snippet)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SNIPPETS)?;
            table.insert(snippet.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%snippet.id, "snippet preview deleted");
        Ok(snippet)
    }

    /// 获取 preview 目录路径
    pub fn preview_dir(&self) -> &PathBuf {
        &self.preview_dir
    }

    pub fn append_record(&self, record: &GenerationRecord) -> CoreResult<()> {
        let serialized = serde_json::to_string(record)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_RECORDS)?;
            table.insert(record.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%record.id, task_id=%record.task_id, images=%record.images.len(), "record appended");
        Ok(())
    }

    pub fn list_snippets(
        &self,
        query: Option<&str>,
        category: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> CoreResult<Page<Snippet>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_SNIPPETS)?;
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let snippet: Snippet = serde_json::from_str(&value.value())?;
            if let Some(cat) = category {
                if snippet.category != cat {
                    continue;
                }
            }
            if let Some(q) = query {
                let ql = q.to_lowercase();
                let hay = format!(
                    "{} {} {:?}",
                    snippet.name,
                    snippet.description.clone().unwrap_or_default(),
                    snippet.tags.join(" ")
                )
                .to_lowercase();
                if !hay.contains(&ql) {
                    continue;
                }
            }
            out.push(snippet);
        }
        let total = out.len();
        let items = out.into_iter().skip(offset).take(limit).collect();
        Ok(Page { items, total })
    }

    pub fn list_recent_records(&self, limit: usize) -> CoreResult<Vec<GenerationRecord>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_RECORDS)?;
        let mut records = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let rec: GenerationRecord = serde_json::from_str(&value.value())?;
            records.push(rec);
        }
        records.sort_by_key(|r| r.created_at);
        records.reverse();
        records.truncate(limit);
        Ok(records)
    }

    pub fn list_presets(&self, offset: usize, limit: usize) -> CoreResult<Page<CharacterPreset>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_PRESETS)?;
        let mut presets = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let preset: CharacterPreset = serde_json::from_str(&value.value())?;
            presets.push(preset);
        }
        presets.sort_by(|a, b| a.name.cmp(&b.name));
        let total = presets.len();
        let items = presets.into_iter().skip(offset).take(limit).collect();
        Ok(Page { items, total })
    }

    /// 保存上次生成设置
    pub fn save_last_generation_settings(
        &self,
        settings: &LastGenerationSettings,
    ) -> CoreResult<()> {
        let serialized = serde_json::to_string(settings)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SETTINGS)?;
            table.insert(SETTINGS_KEY_LAST_GENERATION, serialized)?;
        }
        write_txn.commit()?;
        info!("last generation settings saved");
        Ok(())
    }

    /// 加载上次生成设置
    pub fn load_last_generation_settings(&self) -> CoreResult<Option<LastGenerationSettings>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_SETTINGS)?;
        if let Some(value) = table.get(SETTINGS_KEY_LAST_GENERATION)? {
            let settings: LastGenerationSettings = serde_json::from_str(&value.value())?;
            return Ok(Some(settings));
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct SnippetResolver {
    storage: Arc<CoreStorage>,
}

impl SnippetResolver {
    pub fn new(storage: Arc<CoreStorage>) -> Self {
        Self { storage }
    }

    pub fn expand(&self, prompt: &str) -> CoreResult<String> {
        let mut result = String::with_capacity(prompt.len());
        let mut chars = prompt.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '<' {
                let mut token = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '>' {
                        break;
                    }
                    token.push(next);
                }
                if let Some(rest) = token.strip_prefix("snippet:") {
                    validate_snippet_name(rest)?;
                    let snippet = self
                        .storage
                        .get_snippet_by_name(rest)?
                        .ok_or_else(|| anyhow!("snippet not found: {rest}"))?;
                    result.push_str(&snippet.content);
                } else {
                    // Unknown token, keep literal
                    result.push('<');
                    result.push_str(&token);
                    result.push('>');
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct TaskExecutor {
    client: Arc<NaiClient>,
    storage: Arc<CoreStorage>,
    gallery: GalleryPaths,
}

impl TaskExecutor {
    pub fn new(client: Arc<NaiClient>, storage: Arc<CoreStorage>, gallery: GalleryPaths) -> Self {
        Self {
            client,
            storage,
            gallery,
        }
    }

    pub async fn execute(&self, task: GenerateTaskRequest) -> CoreResult<GenerationRecord> {
        info!(task_id=%task.id, count=task.count, "task started");

        let storage_for_expand = Arc::clone(&self.storage);
        let applied = if let Some(preset) = &task.preset {
            preset.apply(&task.raw_prompt)
        } else {
            task.raw_prompt.clone()
        };

        let expanded_prompt = tokio::task::spawn_blocking(move || {
            let resolver = SnippetResolver::new(storage_for_expand);
            resolver.expand(&applied)
        })
        .await
        .map_err(|e| anyhow!("join error: {e}"))??;

        let mut images = Vec::with_capacity(task.count as usize);

        // Use fixed seed if provided, otherwise random
        let base_seed = task.params.seed.filter(|&s| s > 0).map(|s| s as u64);

        for idx in 0..task.count {
            let seed = base_seed.unwrap_or_else(random_seed);
            info!(task_id=%task.id, idx, seed, "generating image");
            let req = to_nai_request(&task, &expanded_prompt, seed);
            let bytes = self.client.generate_image(&req).await?;
            let path = self.gallery.image_path(idx, seed);

            let path_clone = path.clone();
            tokio::task::spawn_blocking(move || -> CoreResult<()> {
                if let Some(parent) = path_clone.parent() {
                    fs::create_dir_all(parent).context("create gallery dir")?;
                }
                fs::write(&path_clone, &bytes).context("write generated image")?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("join error: {e}"))??;

            images.push(GalleryImage {
                path,
                seed,
                width: task.params.width,
                height: task.params.height,
            });
        }

        let storage_for_record = Arc::clone(&self.storage);
        let record_id = Uuid::new_v4();
        let record_len = images.len();
        let record = GenerationRecord {
            id: record_id.clone(),
            task_id: task.id,
            created_at: Utc::now(),
            raw_prompt: task.raw_prompt,
            expanded_prompt,
            negative_prompt: task.negative_prompt,
            images,
        };

        let append = record.clone();
        tokio::task::spawn_blocking(move || storage_for_record.append_record(&append))
            .await
            .map_err(|e| anyhow!("join error: {e}"))??;

        info!(task_id=%task.id, record_id=%record_id, images=%record_len, "task completed");
        Ok(record)
    }
}

fn to_nai_request(task: &GenerateTaskRequest, prompt: &str, seed: u64) -> ImageGenerationRequest {
    ImageGenerationRequest {
        model: task.params.model,
        prompt_positive: prompt.to_string(),
        prompt_negative: task.negative_prompt.clone(),
        quantity: None,
        width: task.params.width,
        height: task.params.height,
        steps: task.params.steps,
        scale: task.params.scale,
        sampler: task.params.sampler,
        noise: task.params.noise,
        cfg_rescale: task.params.cfg_rescale,
        seed: Some(seed as i64),
        character_prompts: task.params.character_prompts.clone(),
        add_quality_tags: task.params.add_quality_tags,
        undesired_content_preset: task.params.undesired_content_preset,
        legacy_uc: false,
        variety_plus: false,
    }
}

fn random_seed() -> u64 {
    let mut rng = rng();
    rng.random_range(1_000_000_000u64..=9_999_999_999u64)
}

fn validate_snippet_name(name: &str) -> CoreResult<()> {
    if name.contains(['<', '>', ',', ' ', '{', '}', '(', ')', '[', ']']) || name.is_empty() {
        return Err(anyhow!("invalid snippet name"));
    }
    Ok(())
}
