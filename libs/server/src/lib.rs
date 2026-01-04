use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use codex_api::NaiClient;
use codex_core::{
    CharacterSlotSettings, CoreStorage, GalleryPaths, GenerateTaskRequest, GenerationParams,
    GenerationRecord, HighlightSpan, LastGenerationSettings, Lexicon, MainPresetSettings,
    PromptParser, PromptProcessor, TaskExecutor,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use uuid::Uuid;

mod archive;
mod lexicon;
mod perset;
mod snippet;

use crate::archive::{
    ArchiveState, create_archive, create_archive_selected, delete_archive, download_archive,
    get_archive_status, list_archivable_dates, list_archives,
};
use crate::lexicon::{get_lexicon_category, get_lexicon_index, search_lexicon};
use crate::perset::{
    create_main_preset, create_preset, delete_main_preset, delete_preset, delete_preset_preview,
    get_main_preset, get_preset, list_main_presets, list_presets, rename_preset,
    update_main_preset, update_preset, update_preset_preview,
};
use crate::snippet::{
    create_snippet, delete_snippet, delete_snippet_preview, get_snippet, list_snippets,
    rename_snippet, update_snippet, update_snippet_preview,
};

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
    pub archive_state: ArchiveState,
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
        archive_state: ArchiveState::new(),
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
        // 归档 API
        .route("/archives", get(list_archives).post(create_archive))
        .route("/archives/dates", get(list_archivable_dates))
        .route("/archives/selected", post(create_archive_selected))
        .route("/archives/status", get(get_archive_status))
        .route(
            "/archives/{name}",
            get(download_archive).delete(delete_archive),
        )
        // 增加请求体大小限制（10MB，适应较大的图片上传）
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024));

    let mut router = Router::new()
        .nest("/api", api_router)
        .with_state(state.clone());

    if let Some(static_dir) = cfg.static_dir.clone() {
        router = router.fallback_service(
            ServiceBuilder::new()
                .layer(axum::middleware::from_fn(index_cache_control))
                .service(ServeDir::new(static_dir)),
        );
    }

    router = router.nest_service("/gallery", ServeDir::new(cfg.gallery_dir.clone()));
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

async fn index_cache_control(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut response = next.run(req).await;

    let cache_value = if path.contains("/assets/") {
        "public, max-age=31536000, immutable"
    } else if path.ends_with(".html") || path == "/" {
        "no-cache, no-store, must-revalidate"
    } else {
        "no-cache"
    };

    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_str(cache_value).unwrap());
    response
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

/// Snippet / Preset shared payloads

#[derive(Debug, Deserialize)]
struct UpdatePreviewPayload {
    preview_base64: String,
}

#[derive(Debug, Deserialize)]
struct RenamePayload {
    name: String,
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

    /// 检查是否有任务正在运行或待处理
    pub async fn has_active_tasks(&self) -> bool {
        let map = self.statuses.lock().await;
        map.values()
            .any(|s| matches!(s, TaskStatus::Pending | TaskStatus::Running))
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
