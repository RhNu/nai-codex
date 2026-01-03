use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use base64::{self, Engine, prelude::BASE64_STANDARD};
use codex_api::NaiClient;
use codex_core::{
    CharacterPreset, CharacterSlotSettings, CoreStorage, GalleryPaths, GenerateTaskRequest,
    GenerationParams, GenerationRecord, HighlightSpan, LastGenerationSettings, Lexicon, MainPreset,
    MainPresetSettings, PromptParser, PromptProcessor, Snippet, TaskExecutor,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use tower_http::services::ServeDir;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub addr: SocketAddr,
    pub db_path: PathBuf,
    pub preview_dir: PathBuf,
    pub gallery_dir: PathBuf,
    pub static_dir: Option<PathBuf>,
    pub nai_token: String,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<CoreStorage>,
    pub queue: TaskQueue,
    pub gallery_dir: PathBuf,
    pub lexicon: Option<Arc<Lexicon>>,
    pub nai_client: Arc<NaiClient>,
}

pub async fn serve(cfg: ServerConfig) -> Result<()> {
    let storage = Arc::new(CoreStorage::open(&cfg.db_path, &cfg.preview_dir)?);
    let gallery = GalleryPaths::new(&cfg.gallery_dir);
    let client = Arc::new(NaiClient::new(cfg.nai_token)?);
    let queue = TaskQueue::new(Arc::clone(&client), Arc::clone(&storage), gallery.clone());

    // 从嵌入数据加载词库
    let lexicon = match Lexicon::load_embedded() {
        Ok(lex) => {
            tracing::info!("lexicon loaded from embedded data");
            Some(Arc::new(lex))
        }
        Err(err) => {
            tracing::warn!("failed to load lexicon: {}", err);
            None
        }
    };

    let state = AppState {
        storage,
        queue,
        gallery_dir: cfg.gallery_dir.clone(),
        lexicon,
        nai_client: client,
    };

    // API 路由都放在 /api 前缀下
    let api_router = Router::new()
        .route("/health", get(health))
        .route("/quota", get(get_quota))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/records/recent", get(list_recent_records))
        .route("/records/{id}", axum::routing::delete(delete_record))
        .route("/records/batch", post(delete_records_batch))
        .route("/snippets", get(list_snippets).post(create_snippet))
        .route(
            "/snippets/{id}",
            get(get_snippet).put(update_snippet).delete(delete_snippet),
        )
        .route(
            "/snippets/{id}/preview",
            put(update_snippet_preview).delete(delete_snippet_preview),
        )
        .route("/snippets/{id}/rename", put(rename_snippet))
        .route("/presets", get(list_presets).post(create_preset))
        .route(
            "/presets/{id}",
            get(get_preset).put(update_preset).delete(delete_preset),
        )
        .route(
            "/presets/{id}/preview",
            put(update_preset_preview).delete(delete_preset_preview),
        )
        .route("/presets/{id}/rename", put(rename_preset))
        // 主预设 API
        .route(
            "/main-presets",
            get(list_main_presets).post(create_main_preset),
        )
        .route(
            "/main-presets/{id}",
            get(get_main_preset)
                .put(update_main_preset)
                .delete(delete_main_preset),
        )
        .route(
            "/settings/generation",
            get(get_generation_settings).put(save_generation_settings),
        )
        .route("/prompt/parse", post(parse_prompt))
        .route("/prompt/format", post(format_prompt))
        .route("/prompt/dry-run", post(dry_run_prompt))
        // 词库 API
        .route("/lexicon", get(get_lexicon_index))
        .route("/lexicon/categories/{name}", get(get_lexicon_category))
        .route("/lexicon/search", get(search_lexicon))
        // 增加请求体大小限制（10MB，适应较大的图片上传）
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024));

    let mut router = Router::new()
        .nest("/api", api_router)
        .with_state(state.clone());

    if let Some(static_dir) = cfg.static_dir.clone() {
        router = router.fallback_service(ServeDir::new(static_dir));
    }
    router = router.nest_service("/gallery", ServeDir::new(cfg.gallery_dir.clone()));
    // 添加预览图服务
    router = router.nest_service(
        "/previews",
        ServeDir::new(state.storage.preview_dir().clone()),
    );

    tracing::info!("server listening on {}", cfg.addr);
    axum::serve(
        tokio::net::TcpListener::bind(cfg.addr).await?,
        router.into_make_service(),
    )
    .await?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Debug, Serialize)]
struct QuotaResponse {
    anlas: u64,
}

async fn get_quota(State(state): State<AppState>) -> impl IntoResponse {
    match state.nai_client.inquire_quota().await {
        Ok(anlas) => (StatusCode::OK, Json(QuotaResponse { anlas })).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CreateTaskPayload {
    raw_prompt: String,
    negative_prompt: String,
    #[serde(default = "default_count")]
    count: u32,
    #[serde(default)]
    params: Option<GenerationParams>,
    /// 主提示词预设设置
    #[serde(default)]
    main_preset: MainPresetSettings,
}

#[derive(Debug, Serialize)]
pub struct GenerationRecordView {
    id: String,
    task_id: String,
    created_at: String,
    raw_prompt: String,
    expanded_prompt: String,
    negative_prompt: String,
    images: Vec<GalleryImageView>,
}

#[derive(Debug, Serialize)]
struct GalleryImageView {
    url: String,
    seed: u64,
    width: u32,
    height: u32,
}

fn default_count() -> u32 {
    1
}

#[derive(Debug, Serialize)]
struct TaskSubmittedResponse {
    id: Uuid,
}

async fn create_task(
    State(state): State<AppState>,
    Json(payload): Json<CreateTaskPayload>,
) -> impl IntoResponse {
    let mut task = GenerateTaskRequest::new(payload.raw_prompt, payload.negative_prompt);
    task.count = payload.count.max(1);
    task.main_preset = payload.main_preset;
    if let Some(params) = payload.params {
        task.params = params;
    }
    // preset_id 已废弃，保留兼容性但不再使用
    // 角色预设现在由前端构建 character_prompts 时直接处理

    let id = task.id;
    if let Err(err) = state.queue.submit(task).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }

    (StatusCode::ACCEPTED, Json(TaskSubmittedResponse { id })).into_response()
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskStatusView {
    Pending,
    Running,
    Completed { record: GenerationRecordView },
    Failed { error: String },
    Unknown,
}

async fn get_task(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let gallery = state.gallery_dir.clone();
    let status = state.queue.status(&id).await;
    let view = match status {
        Some(TaskStatus::Pending) => TaskStatusView::Pending,
        Some(TaskStatus::Running) => TaskStatusView::Running,
        Some(TaskStatus::Completed(rec)) => TaskStatusView::Completed {
            record: to_record_view(rec, &gallery),
        },
        Some(TaskStatus::Failed(err)) => TaskStatusView::Failed { error: err },
        None => TaskStatusView::Unknown,
    };
    Json(view)
}

async fn list_recent_records(State(state): State<AppState>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    let gallery = state.gallery_dir.clone();
    match tokio::task::spawn_blocking(move || storage.list_recent_records(50)).await {
        Ok(Ok(records)) => {
            let mapped: Vec<_> = records
                .into_iter()
                .map(|r| to_record_view(r, &gallery))
                .collect();
            Json(mapped).into_response()
        }
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 删除单条记录
async fn delete_record(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_record(id)).await {
        Ok(Ok(Some(_))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(None)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct DeleteRecordsBatchPayload {
    ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
struct DeleteRecordsBatchResponse {
    deleted: usize,
}

/// 批量删除记录
async fn delete_records_batch(
    State(state): State<AppState>,
    Json(payload): Json<DeleteRecordsBatchPayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_records(&payload.ids)).await {
        Ok(Ok(deleted)) => Json(DeleteRecordsBatchResponse { deleted }).into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct SnippetQuery {
    q: Option<String>,
    category: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    20
}

async fn list_snippets(
    State(state): State<AppState>,
    Query(q): Query<SnippetQuery>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || {
        storage.list_snippets(q.q.as_deref(), q.category.as_deref(), q.offset, q.limit)
    })
    .await
    {
        Ok(Ok(page)) => Json(page).into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CreateSnippetPayload {
    name: String,
    category: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    preview_base64: Option<String>,
}

#[derive(Debug, Serialize)]
struct SnippetResponse {
    id: String,
    name: String,
    category: String,
}

async fn create_snippet(
    State(state): State<AppState>,
    Json(payload): Json<CreateSnippetPayload>,
) -> impl IntoResponse {
    let mut snippet = match Snippet::new(payload.name, payload.category, payload.content) {
        Ok(s) => s,
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };
    snippet.tags = payload.tags;
    snippet.description = payload.description;

    let preview_bytes = match payload.preview_base64 {
        Some(b64) => match BASE64_STANDARD.decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        },
        None => None,
    };

    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || {
        storage.upsert_snippet(snippet, preview_bytes.as_deref())
    })
    .await
    {
        Ok(Ok(saved)) => {
            let body = Json(SnippetResponse {
                id: saved.id.to_string(),
                name: saved.name,
                category: saved.category,
            });
            (StatusCode::CREATED, body).into_response()
        }
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateSnippetPayload {
    name: Option<String>,
    category: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    description: Option<String>,
    preview_base64: Option<String>,
}

async fn update_snippet(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateSnippetPayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    let storage_for_get = Arc::clone(&storage);

    // First get the existing snippet
    let existing = match tokio::task::spawn_blocking(move || storage_for_get.get_snippet(id)).await
    {
        Ok(Ok(Some(snippet))) => snippet,
        Ok(Ok(None)) => return (StatusCode::NOT_FOUND, "snippet not found").into_response(),
        Ok(Err(err)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };

    // Update fields
    let mut snippet = existing;
    if let Some(name) = payload.name {
        snippet.name = name;
    }
    if let Some(category) = payload.category {
        snippet.category = category;
    }
    if let Some(content) = payload.content {
        snippet.content = content;
    }
    if let Some(tags) = payload.tags {
        snippet.tags = tags;
    }
    if payload.description.is_some() {
        snippet.description = payload.description;
    }

    let preview_bytes = match payload.preview_base64 {
        Some(b64) => match BASE64_STANDARD.decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        },
        None => None,
    };

    match tokio::task::spawn_blocking(move || {
        storage.upsert_snippet(snippet, preview_bytes.as_deref())
    })
    .await
    {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn get_snippet(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.get_snippet(id)).await {
        Ok(Ok(Some(snippet))) => Json(snippet).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "snippet not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_snippet(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_snippet(id)).await {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "snippet not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UpdatePreviewPayload {
    preview_base64: String,
}

async fn update_snippet_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdatePreviewPayload>,
) -> impl IntoResponse {
    let preview_bytes = match BASE64_STANDARD.decode(&payload.preview_base64) {
        Ok(bytes) => bytes,
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };

    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.update_snippet_preview(id, &preview_bytes))
        .await
    {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_snippet_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_snippet_preview(id)).await {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct RenamePayload {
    name: String,
}

async fn rename_snippet(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RenamePayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.rename_snippet(id, payload.name)).await {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct PresetQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

async fn list_presets(
    State(state): State<AppState>,
    Query(q): Query<PresetQuery>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.list_presets(q.offset, q.limit)).await {
        Ok(Ok(page)) => Json(page).into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CreatePresetPayload {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    uc_before: Option<String>,
    #[serde(default)]
    uc_after: Option<String>,
    #[serde(default)]
    uc_replace: Option<String>,
    #[serde(default)]
    preview_base64: Option<String>,
}

async fn create_preset(
    State(state): State<AppState>,
    Json(payload): Json<CreatePresetPayload>,
) -> impl IntoResponse {
    let mut preset = CharacterPreset::new(payload.name);
    preset.description = payload.description;
    preset.before = payload.before;
    preset.after = payload.after;
    preset.replace = payload.replace;
    preset.uc_before = payload.uc_before;
    preset.uc_after = payload.uc_after;
    preset.uc_replace = payload.uc_replace;

    let preview_bytes = match payload.preview_base64 {
        Some(b64) => match BASE64_STANDARD.decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        },
        None => None,
    };

    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || {
        storage.upsert_preset_with_preview(preset, preview_bytes.as_deref())
    })
    .await
    {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn get_preset(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.get_preset(id)).await {
        Ok(Ok(Some(preset))) => Json(preset).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UpdatePresetPayload {
    name: Option<String>,
    description: Option<String>,
    before: Option<String>,
    after: Option<String>,
    replace: Option<String>,
    uc_before: Option<String>,
    uc_after: Option<String>,
    uc_replace: Option<String>,
    preview_base64: Option<String>,
}

async fn update_preset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdatePresetPayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    let storage_for_get = Arc::clone(&storage);

    // First get the existing preset
    let existing = match tokio::task::spawn_blocking(move || storage_for_get.get_preset(id)).await {
        Ok(Ok(Some(preset))) => preset,
        Ok(Ok(None)) => return (StatusCode::NOT_FOUND, "preset not found").into_response(),
        Ok(Err(err)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };

    // Update fields
    let mut preset = existing;
    if let Some(name) = payload.name {
        preset.name = name;
    }
    if payload.description.is_some() {
        preset.description = payload.description;
    }
    if payload.before.is_some() {
        preset.before = payload.before;
    }
    if payload.after.is_some() {
        preset.after = payload.after;
    }
    if payload.replace.is_some() {
        preset.replace = payload.replace;
    }
    if payload.uc_before.is_some() {
        preset.uc_before = payload.uc_before;
    }
    if payload.uc_after.is_some() {
        preset.uc_after = payload.uc_after;
    }
    if payload.uc_replace.is_some() {
        preset.uc_replace = payload.uc_replace;
    }
    preset.updated_at = chrono::Utc::now();

    let preview_bytes = match payload.preview_base64 {
        Some(b64) => match BASE64_STANDARD.decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        },
        None => None,
    };

    match tokio::task::spawn_blocking(move || {
        storage.upsert_preset_with_preview(preset, preview_bytes.as_deref())
    })
    .await
    {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn update_preset_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdatePreviewPayload>,
) -> impl IntoResponse {
    let preview_bytes = match BASE64_STANDARD.decode(&payload.preview_base64) {
        Ok(bytes) => bytes,
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };

    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.update_preset_preview(id, &preview_bytes))
        .await
    {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_preset_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_preset_preview(id)).await {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_preset(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_preset(id)).await {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn rename_preset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RenamePayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.rename_preset(id, payload.name)).await {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

// ============== Main Presets ==============

async fn list_main_presets(
    State(state): State<AppState>,
    Query(q): Query<PresetQuery>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.list_main_presets(q.offset, q.limit)).await {
        Ok(Ok(page)) => Json(page).into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CreateMainPresetPayload {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    uc_before: Option<String>,
    #[serde(default)]
    uc_after: Option<String>,
    #[serde(default)]
    uc_replace: Option<String>,
}

async fn create_main_preset(
    State(state): State<AppState>,
    Json(payload): Json<CreateMainPresetPayload>,
) -> impl IntoResponse {
    let mut preset = MainPreset::new(payload.name);
    preset.description = payload.description;
    preset.before = payload.before;
    preset.after = payload.after;
    preset.replace = payload.replace;
    preset.uc_before = payload.uc_before;
    preset.uc_after = payload.uc_after;
    preset.uc_replace = payload.uc_replace;

    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.upsert_main_preset(preset)).await {
        Ok(Ok(saved)) => (StatusCode::CREATED, Json(saved)).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn get_main_preset(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.get_main_preset(id)).await {
        Ok(Ok(Some(preset))) => Json(preset).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "main preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateMainPresetPayload {
    name: Option<String>,
    description: Option<String>,
    before: Option<String>,
    after: Option<String>,
    replace: Option<String>,
    uc_before: Option<String>,
    uc_after: Option<String>,
    uc_replace: Option<String>,
}

async fn update_main_preset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateMainPresetPayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    let storage_for_get = Arc::clone(&storage);

    // First get the existing preset
    let existing = match tokio::task::spawn_blocking(move || storage_for_get.get_main_preset(id))
        .await
    {
        Ok(Ok(Some(preset))) => preset,
        Ok(Ok(None)) => return (StatusCode::NOT_FOUND, "main preset not found").into_response(),
        Ok(Err(err)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };

    // Update fields
    let mut preset = existing;
    if let Some(name) = payload.name {
        preset.name = name;
    }
    if payload.description.is_some() {
        preset.description = payload.description;
    }
    if payload.before.is_some() {
        preset.before = payload.before;
    }
    if payload.after.is_some() {
        preset.after = payload.after;
    }
    if payload.replace.is_some() {
        preset.replace = payload.replace;
    }
    if payload.uc_before.is_some() {
        preset.uc_before = payload.uc_before;
    }
    if payload.uc_after.is_some() {
        preset.uc_after = payload.uc_after;
    }
    if payload.uc_replace.is_some() {
        preset.uc_replace = payload.uc_replace;
    }
    preset.updated_at = chrono::Utc::now();

    match tokio::task::spawn_blocking(move || storage.upsert_main_preset(preset)).await {
        Ok(Ok(saved)) => Json(saved).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_main_preset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_main_preset(id)).await {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "main preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

// ============== Generation Settings ==============

async fn get_generation_settings(State(state): State<AppState>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.load_last_generation_settings()).await {
        Ok(Ok(Some(settings))) => Json(settings).into_response(),
        Ok(Ok(None)) => Json(LastGenerationSettings::default()).into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn save_generation_settings(
    State(state): State<AppState>,
    Json(settings): Json<LastGenerationSettings>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.save_last_generation_settings(&settings))
        .await
    {
        Ok(Ok(())) => StatusCode::OK.into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Clone)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed(GenerationRecord),
    Failed(String),
}

#[derive(Clone)]
pub struct TaskQueue {
    tx: mpsc::Sender<GenerateTaskRequest>,
    statuses: Arc<Mutex<HashMap<Uuid, TaskStatus>>>,
}

impl TaskQueue {
    pub fn new(client: Arc<NaiClient>, storage: Arc<CoreStorage>, gallery: GalleryPaths) -> Self {
        let (tx, mut rx) = mpsc::channel::<GenerateTaskRequest>(32);
        let statuses = Arc::new(Mutex::new(HashMap::new()));
        let status_clone = Arc::clone(&statuses);
        let client_clone = Arc::clone(&client);
        let storage_clone = Arc::clone(&storage);
        let gallery_clone = gallery.clone();
        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                {
                    let mut map = status_clone.lock().await;
                    map.insert(task.id, TaskStatus::Running);
                }

                let executor = TaskExecutor::new(
                    Arc::clone(&client_clone),
                    Arc::clone(&storage_clone),
                    gallery_clone.clone(),
                );
                let res = executor.execute(task.clone()).await;
                let mut map = status_clone.lock().await;
                match res {
                    Ok(record) => {
                        map.insert(record.task_id, TaskStatus::Completed(record));
                    }
                    Err(err) => {
                        map.insert(task.id, TaskStatus::Failed(err.to_string()));
                    }
                }
            }
        });

        Self { tx, statuses }
    }

    pub async fn submit(&self, task: GenerateTaskRequest) -> Result<()> {
        {
            let mut map = self.statuses.lock().await;
            map.insert(task.id, TaskStatus::Pending);
        }
        self.tx.send(task).await.map_err(|e| anyhow!(e))
    }

    pub async fn status(&self, id: &Uuid) -> Option<TaskStatus> {
        let map = self.statuses.lock().await;
        map.get(id).cloned()
    }
}

fn to_record_view(rec: GenerationRecord, gallery_root: &std::path::Path) -> GenerationRecordView {
    GenerationRecordView {
        id: rec.id.to_string(),
        task_id: rec.task_id.to_string(),
        created_at: rec.created_at.to_rfc3339(),
        raw_prompt: rec.raw_prompt,
        expanded_prompt: rec.expanded_prompt,
        negative_prompt: rec.negative_prompt,
        images: rec
            .images
            .into_iter()
            .map(|img| GalleryImageView {
                url: to_gallery_url(&img.path, gallery_root),
                seed: img.seed,
                width: img.width,
                height: img.height,
            })
            .collect(),
    }
}

fn to_gallery_url(path: &std::path::Path, gallery_root: &std::path::Path) -> String {
    if let Ok(rel) = path.strip_prefix(gallery_root) {
        let mut url = String::from("/gallery/");
        url.push_str(&rel.to_string_lossy().replace('\\', "/"));
        return url;
    }
    path.to_string_lossy().replace('\\', "/")
}

// ============== Prompt API ==============

#[derive(Debug, Deserialize)]
struct PromptPayload {
    prompt: String,
}

#[derive(Debug, Serialize)]
struct ParsePromptResponse {
    spans: Vec<HighlightSpan>,
    unclosed_braces: i32,
    unclosed_brackets: i32,
    unclosed_weight: bool,
}

async fn parse_prompt(Json(payload): Json<PromptPayload>) -> impl IntoResponse {
    let result = PromptParser::parse(&payload.prompt);
    let spans = PromptParser::to_highlight_spans(&result);

    Json(ParsePromptResponse {
        spans,
        unclosed_braces: result.unclosed_braces,
        unclosed_brackets: result.unclosed_brackets,
        unclosed_weight: result.unclosed_weight,
    })
}

#[derive(Debug, Serialize)]
struct FormatPromptResponse {
    formatted: String,
}

async fn format_prompt(Json(payload): Json<PromptPayload>) -> impl IntoResponse {
    let formatted = PromptParser::format(&payload.prompt);
    Json(FormatPromptResponse { formatted })
}

// Dry-run 请求负载
#[derive(Debug, Deserialize)]
struct DryRunPayload {
    raw_positive: String,
    raw_negative: String,
    #[serde(default)]
    main_preset: Option<MainPresetSettings>,
    #[serde(default)]
    character_slots: Vec<CharacterSlotSettings>,
}

/// 执行 dry-run，返回提示词处理链各阶段的结果
async fn dry_run_prompt(
    State(state): State<AppState>,
    Json(payload): Json<DryRunPayload>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || {
        let processor = PromptProcessor::new(storage);
        processor.dry_run(
            &payload.raw_positive,
            &payload.raw_negative,
            &payload.main_preset.unwrap_or_default(),
            &payload.character_slots,
        )
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

// ============== Lexicon API ==============

async fn get_lexicon_index(State(state): State<AppState>) -> impl IntoResponse {
    match &state.lexicon {
        Some(lex) => Json(lex.get_index().clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "lexicon not loaded").into_response(),
    }
}

async fn get_lexicon_category(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match &state.lexicon {
        Some(lex) => match lex.get_category(&name) {
            Some(cat) => Json(cat.clone()).into_response(),
            None => (StatusCode::NOT_FOUND, "category not found").into_response(),
        },
        None => (StatusCode::NOT_FOUND, "lexicon not loaded").into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct LexiconSearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_search_limit() -> usize {
    50
}

async fn search_lexicon(
    State(state): State<AppState>,
    Query(query): Query<LexiconSearchQuery>,
) -> impl IntoResponse {
    match &state.lexicon {
        Some(lex) => {
            let result = lex.search(&query.q, query.limit, query.offset);
            Json(result).into_response()
        }
        None => (StatusCode::NOT_FOUND, "lexicon not loaded").into_response(),
    }
}
