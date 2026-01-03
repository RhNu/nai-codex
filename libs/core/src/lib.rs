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

pub mod preset;
pub use preset::{CharacterPreset, MainPreset, MainPresetSettings};

const TABLE_SNIPPETS: TableDefinition<Uuid, String> = TableDefinition::new("snippets");
const TABLE_SNIPPET_NAME_INDEX: TableDefinition<String, Uuid> =
    TableDefinition::new("snippets_by_name");
const TABLE_PRESETS: TableDefinition<Uuid, String> = TableDefinition::new("character_presets");
const TABLE_MAIN_PRESETS: TableDefinition<Uuid, String> = TableDefinition::new("main_presets");
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

/// Snippet 重命名结果，包含更新统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameSnippetResult {
    pub snippet: Snippet,
    pub updated_presets: usize,
    pub updated_settings: bool,
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
    /// Variety+ mode for dynamic variation
    pub variety_plus: bool,
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
            variety_plus: false,
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
    /// 主提示词预设ID（替代之前的内联设置）
    #[serde(default)]
    pub main_preset_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateTaskRequest {
    pub id: Uuid,
    pub raw_prompt: String,
    pub negative_prompt: String,
    /// How many images to generate sequentially.
    pub count: u32,
    pub params: GenerationParams,
    /// 角色预设（应用于角色槽）
    pub preset: Option<CharacterPreset>,
    /// 主提示词预设设置
    #[serde(default)]
    pub main_preset: MainPresetSettings,
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
            main_preset: MainPresetSettings::default(),
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
                write_txn.open_table(TABLE_MAIN_PRESETS)?;
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

    /// 重命名 snippet，并更新所有引用该 snippet 的 preset 和 LastGenerationSettings
    pub fn rename_snippet(&self, id: Uuid, new_name: String) -> CoreResult<RenameSnippetResult> {
        validate_snippet_name(&new_name)?;

        let mut snippet = self
            .get_snippet(id)?
            .ok_or_else(|| anyhow!("snippet not found"))?;

        let old_name = snippet.name.clone();

        // 如果名称没变，直接返回
        if old_name == new_name {
            return Ok(RenameSnippetResult {
                snippet,
                updated_presets: 0,
                updated_settings: false,
            });
        }

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

        // 更新所有引用该 snippet 的 preset 和 settings
        let (updated_presets, updated_settings) =
            self.update_snippet_references(&old_name, &new_name)?;

        info!(
            old_name=%old_name,
            new_name=%new_name,
            updated_presets=%updated_presets,
            updated_settings=%updated_settings,
            "snippet references updated"
        );

        Ok(RenameSnippetResult {
            snippet,
            updated_presets,
            updated_settings,
        })
    }

    /// 更新所有引用旧 snippet 名称的地方
    fn update_snippet_references(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> CoreResult<(usize, bool)> {
        let old_tag = format!("<snippet:{}>", old_name);
        let new_tag = format!("<snippet:{}>", new_name);

        // 更新所有 presets
        let mut updated_presets = 0;
        let presets = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(TABLE_PRESETS)?;
            let mut list = Vec::new();
            for entry in table.iter()? {
                let (_, value) = entry?;
                let preset: CharacterPreset = serde_json::from_str(&value.value())?;
                list.push(preset);
            }
            list
        };

        for mut preset in presets {
            let mut changed = false;

            if let Some(ref mut before) = preset.before {
                if before.contains(&old_tag) {
                    *before = before.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }
            if let Some(ref mut after) = preset.after {
                if after.contains(&old_tag) {
                    *after = after.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }
            if let Some(ref mut replace) = preset.replace {
                if replace.contains(&old_tag) {
                    *replace = replace.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }
            if let Some(ref mut uc_before) = preset.uc_before {
                if uc_before.contains(&old_tag) {
                    *uc_before = uc_before.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }
            if let Some(ref mut uc_after) = preset.uc_after {
                if uc_after.contains(&old_tag) {
                    *uc_after = uc_after.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }
            if let Some(ref mut uc_replace) = preset.uc_replace {
                if uc_replace.contains(&old_tag) {
                    *uc_replace = uc_replace.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }

            if changed {
                preset.updated_at = Utc::now();
                self.upsert_preset(preset)?;
                updated_presets += 1;
            }
        }

        // 更新 LastGenerationSettings
        let mut updated_settings = false;
        if let Some(mut settings) = self.load_last_generation_settings()? {
            let mut changed = false;

            if settings.prompt.contains(&old_tag) {
                settings.prompt = settings.prompt.replace(&old_tag, &new_tag);
                changed = true;
            }
            if settings.negative_prompt.contains(&old_tag) {
                settings.negative_prompt = settings.negative_prompt.replace(&old_tag, &new_tag);
                changed = true;
            }

            for slot in &mut settings.character_slots {
                if slot.prompt.contains(&old_tag) {
                    slot.prompt = slot.prompt.replace(&old_tag, &new_tag);
                    changed = true;
                }
                if slot.uc.contains(&old_tag) {
                    slot.uc = slot.uc.replace(&old_tag, &new_tag);
                    changed = true;
                }
            }

            if changed {
                self.save_last_generation_settings(&settings)?;
                updated_settings = true;
            }
        }

        Ok((updated_presets, updated_settings))
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

    /// 获取单条记录
    pub fn get_record(&self, id: Uuid) -> CoreResult<Option<GenerationRecord>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_RECORDS)?;
        if let Some(value) = table.get(id)? {
            let record: GenerationRecord = serde_json::from_str(&value.value())?;
            return Ok(Some(record));
        }
        Ok(None)
    }

    /// 删除记录（同时删除关联的图片文件）
    pub fn delete_record(&self, id: Uuid) -> CoreResult<Option<GenerationRecord>> {
        // 先获取记录以便后续删除文件
        let record = self.get_record(id)?;
        if record.is_none() {
            return Ok(None);
        }
        let record = record.unwrap();

        // 删除关联的图片文件
        for img in &record.images {
            if img.path.exists() {
                if let Err(e) = fs::remove_file(&img.path) {
                    info!(path=?img.path, error=%e, "failed to delete gallery image file");
                } else {
                    info!(path=?img.path, "deleted gallery image file");
                }
            }
        }

        // 从数据库删除记录
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_RECORDS)?;
            table.remove(id)?;
        }
        write_txn.commit()?;
        info!(id=%id, images=%record.images.len(), "record deleted");
        Ok(Some(record))
    }

    /// 批量删除记录
    pub fn delete_records(&self, ids: &[Uuid]) -> CoreResult<usize> {
        let mut deleted = 0;
        for id in ids {
            if self.delete_record(*id)?.is_some() {
                deleted += 1;
            }
        }
        Ok(deleted)
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

    // ==================== 主预设 CRUD ====================

    /// 创建或更新主预设
    pub fn upsert_main_preset(&self, preset: MainPreset) -> CoreResult<MainPreset> {
        let serialized = serde_json::to_string(&preset)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_MAIN_PRESETS)?;
            table.insert(preset.id, serialized)?;
        }
        write_txn.commit()?;
        info!(id=%preset.id, name=%preset.name, "main preset upserted");
        Ok(preset)
    }

    /// 获取主预设
    pub fn get_main_preset(&self, id: Uuid) -> CoreResult<Option<MainPreset>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_MAIN_PRESETS)?;
        if let Some(value) = table.get(id)? {
            let preset: MainPreset = serde_json::from_str(&value.value())?;
            return Ok(Some(preset));
        }
        Ok(None)
    }

    /// 删除主预设
    pub fn delete_main_preset(&self, id: Uuid) -> CoreResult<bool> {
        let write_txn = self.db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(TABLE_MAIN_PRESETS)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        if removed {
            info!(id=%id, "main preset deleted");
        }
        Ok(removed)
    }

    /// 列出所有主预设
    pub fn list_main_presets(&self, offset: usize, limit: usize) -> CoreResult<Page<MainPreset>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_MAIN_PRESETS)?;
        let mut presets = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let preset: MainPreset = serde_json::from_str(&value.value())?;
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

/// 角色提示词处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedCharacterPrompt {
    /// 预设应用后的正面提示词
    pub after_preset: String,
    /// snippet 展开后的最终正面提示词
    pub final_prompt: String,
    /// 预设应用后的负面提示词
    pub uc_after_preset: String,
    /// snippet 展开后的最终负面提示词
    pub final_uc: String,
    pub enabled: bool,
}

/// Dry-run 结果，展示提示词处理链的各个阶段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunResult {
    /// 原始正面提示词
    pub raw_positive: String,
    /// 主预设应用后的正面提示词
    pub positive_after_preset: String,
    /// snippet 展开后的最终正面提示词
    pub final_positive: String,
    /// 原始负面提示词
    pub raw_negative: String,
    /// 主预设应用后的负面提示词
    pub negative_after_preset: String,
    /// snippet 展开后的最终负面提示词
    pub final_negative: String,
    /// 角色提示词处理结果
    pub character_prompts: Vec<ProcessedCharacterPrompt>,
}

/// 提示词处理器 - 统一处理提示词预设注入和 snippet 展开
///
/// 处理链：
/// 1. 应用主预设（before/after/replace）到主提示词
/// 2. 应用角色预设到角色提示词
/// 3. 展开所有 snippet 引用
#[derive(Debug, Clone)]
pub struct PromptProcessor {
    storage: Arc<CoreStorage>,
}

impl PromptProcessor {
    pub fn new(storage: Arc<CoreStorage>) -> Self {
        Self { storage }
    }

    /// 执行 dry-run，返回处理链各阶段的结果
    pub fn dry_run(
        &self,
        raw_positive: &str,
        raw_negative: &str,
        main_preset: &MainPresetSettings,
        character_slots: &[CharacterSlotSettings],
    ) -> CoreResult<DryRunResult> {
        let resolver = SnippetResolver::new(Arc::clone(&self.storage));

        // 步骤 1: 应用主预设
        let positive_after_preset = main_preset.apply_positive(raw_positive);
        let negative_after_preset = main_preset.apply_negative(raw_negative);

        // 步骤 2: 展开 snippet
        let final_positive = resolver.expand(&positive_after_preset)?;
        let final_negative = resolver.expand(&negative_after_preset)?;

        // 步骤 3: 处理角色提示词
        let mut processed_chars = Vec::new();
        for slot in character_slots {
            if !slot.enabled {
                continue;
            }
            if slot.prompt.trim().is_empty() && slot.preset_id.is_none() {
                continue;
            }

            let mut char_positive = slot.prompt.clone();
            let mut char_negative = slot.uc.clone();

            // 应用角色预设
            if let Some(preset_id) = slot.preset_id {
                if let Some(preset) = self.storage.get_preset(preset_id)? {
                    char_positive = preset.apply(&char_positive);
                    char_negative = preset.apply_uc(&char_negative);
                }
            }

            let after_preset = char_positive.clone();
            let uc_after_preset = char_negative.clone();

            // 展开 snippet
            let final_char_prompt = resolver.expand(&char_positive)?;
            let final_char_uc = resolver.expand(&char_negative)?;

            processed_chars.push(ProcessedCharacterPrompt {
                after_preset,
                final_prompt: final_char_prompt,
                uc_after_preset,
                final_uc: final_char_uc,
                enabled: true,
            });
        }

        Ok(DryRunResult {
            raw_positive: raw_positive.to_string(),
            positive_after_preset,
            final_positive,
            raw_negative: raw_negative.to_string(),
            negative_after_preset,
            final_negative,
            character_prompts: processed_chars,
        })
    }

    /// 处理任务请求中的提示词，返回处理后的结果
    pub fn process_task(&self, task: &mut GenerateTaskRequest) -> CoreResult<(String, String)> {
        let resolver = SnippetResolver::new(Arc::clone(&self.storage));

        // 步骤 1: 应用主预设
        let positive_after_preset = task.main_preset.apply_positive(&task.raw_prompt);
        let negative_after_preset = task.main_preset.apply_negative(&task.negative_prompt);

        // 步骤 2: 展开主提示词中的 snippet
        let final_positive = resolver.expand(&positive_after_preset)?;
        let final_negative = resolver.expand(&negative_after_preset)?;

        // 步骤 3: 处理角色提示词
        if let Some(ref mut chars) = task.params.character_prompts {
            for char_prompt in chars.iter_mut() {
                char_prompt.prompt = resolver.expand(&char_prompt.prompt)?;
                char_prompt.uc = resolver.expand(&char_prompt.uc)?;
            }
        }

        Ok((final_positive, final_negative))
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

    pub async fn execute(&self, mut task: GenerateTaskRequest) -> CoreResult<GenerationRecord> {
        info!(task_id=%task.id, count=task.count, "task started");

        let storage_for_process = Arc::clone(&self.storage);
        let main_preset = task.main_preset.clone();
        let raw_prompt = task.raw_prompt.clone();
        let raw_negative = task.negative_prompt.clone();
        let character_prompts = task.params.character_prompts.clone();

        // 使用 PromptProcessor 处理提示词
        // 处理链：注入主预设 -> 展开 snippet
        let (expanded_prompt, expanded_negative, expanded_character_prompts) =
            tokio::task::spawn_blocking(move || {
                let processor = PromptProcessor::new(storage_for_process);
                let resolver = SnippetResolver::new(Arc::clone(&processor.storage));

                // 步骤 1: 应用主预设
                let positive_after_preset = main_preset.apply_positive(&raw_prompt);
                let negative_after_preset = main_preset.apply_negative(&raw_negative);

                // 步骤 2: 展开 snippet
                let final_positive = resolver.expand(&positive_after_preset)?;
                let final_negative = resolver.expand(&negative_after_preset)?;

                // 步骤 3: 处理角色提示词
                let expanded_chars = if let Some(chars) = character_prompts {
                    let mut result = Vec::with_capacity(chars.len());
                    for mut char_prompt in chars {
                        char_prompt.prompt = resolver.expand(&char_prompt.prompt)?;
                        char_prompt.uc = resolver.expand(&char_prompt.uc)?;
                        result.push(char_prompt);
                    }
                    Some(result)
                } else {
                    None
                };

                Ok::<_, anyhow::Error>((final_positive, final_negative, expanded_chars))
            })
            .await
            .map_err(|e| anyhow!("join error: {e}"))??;

        // 更新 task 中的 character_prompts 为展开后的版本
        task.params.character_prompts = expanded_character_prompts;

        let mut images = Vec::with_capacity(task.count as usize);

        // Use fixed seed if provided, otherwise random
        let base_seed = task.params.seed.filter(|&s| s > 0).map(|s| s as u64);

        for idx in 0..task.count {
            let seed = base_seed.unwrap_or_else(random_seed);
            info!(task_id=%task.id, idx, seed, "generating image");
            let req = to_nai_request(&task, &expanded_prompt, &expanded_negative, seed);
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
            negative_prompt: expanded_negative,
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

fn to_nai_request(
    task: &GenerateTaskRequest,
    prompt: &str,
    negative: &str,
    seed: u64,
) -> ImageGenerationRequest {
    ImageGenerationRequest {
        model: task.params.model,
        prompt_positive: prompt.to_string(),
        prompt_negative: negative.to_string(),
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
        variety_plus: task.params.variety_plus,
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
