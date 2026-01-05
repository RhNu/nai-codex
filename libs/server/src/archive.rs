use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use codex_core::ArchiveManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::AppState;

/// 归档任务状态
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ArchiveTaskStatus {
    /// 空闲状态
    Idle,
    /// 正在归档
    Running { message: String },
    /// 归档完成
    Completed {
        archives: Vec<codex_core::ArchiveInfo>,
        deleted_records: usize,
    },
    /// 归档失败
    Failed { error: String },
}

/// 归档任务状态管理器
#[derive(Clone)]
pub struct ArchiveState {
    status: Arc<Mutex<ArchiveTaskStatus>>,
}

impl ArchiveState {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(ArchiveTaskStatus::Idle)),
        }
    }

    pub async fn get_status(&self) -> ArchiveTaskStatus {
        self.status.lock().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        matches!(*self.status.lock().await, ArchiveTaskStatus::Running { .. })
    }

    pub async fn set_running(&self, message: String) {
        *self.status.lock().await = ArchiveTaskStatus::Running { message };
    }

    pub async fn set_completed(
        &self,
        archives: Vec<codex_core::ArchiveInfo>,
        deleted_records: usize,
    ) {
        *self.status.lock().await = ArchiveTaskStatus::Completed {
            archives,
            deleted_records,
        };
    }

    pub async fn set_failed(&self, error: String) {
        *self.status.lock().await = ArchiveTaskStatus::Failed { error };
    }

    pub async fn reset(&self) {
        *self.status.lock().await = ArchiveTaskStatus::Idle;
    }
}

/// 归档任务启动响应
#[derive(Debug, Serialize)]
struct ArchiveStartedResponse {
    message: String,
}

/// 列出所有归档文件
pub async fn list_archives(State(state): State<AppState>) -> impl IntoResponse {
    let manager = ArchiveManager::new(&state.gallery_dir, &state.storage);
    match manager.list_archives().await {
        Ok(archives) => Json(archives).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 列出所有可归档的日期
pub async fn list_archivable_dates(State(state): State<AppState>) -> impl IntoResponse {
    let manager = ArchiveManager::new(&state.gallery_dir, &state.storage);
    match manager.list_archivable_dates().await {
        Ok(dates) => Json(dates).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 获取当前归档任务状态
pub async fn get_archive_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.archive_state.get_status().await;
    Json(status)
}

/// 创建归档：归档所有今天之前的日期（异步执行）
pub async fn create_archive(State(state): State<AppState>) -> impl IntoResponse {
    // 检查是否有生成任务正在运行
    if state.queue.has_active_tasks().await {
        return (
            StatusCode::CONFLICT,
            "cannot create archive while generation tasks are running",
        )
            .into_response();
    }

    // 检查是否已有归档任务在运行
    if state.archive_state.is_running().await {
        return (StatusCode::CONFLICT, "archive task is already running").into_response();
    }

    // 设置为运行中状态
    state
        .archive_state
        .set_running("正在归档所有日期...".to_string())
        .await;

    // 启动异步归档任务
    let gallery_dir = state.gallery_dir.clone();
    let storage = Arc::clone(&state.storage);
    let archive_state = state.archive_state.clone();

    tokio::spawn(async move {
        let manager = ArchiveManager::new(&gallery_dir, &storage);
        let result = manager.create_archives().await;

        match result {
            Ok(res) => {
                tracing::info!(
                    archives = res.archives.len(),
                    deleted = res.deleted_records,
                    "archive task completed"
                );
                archive_state
                    .set_completed(res.archives, res.deleted_records)
                    .await;
            }
            Err(err) => {
                tracing::error!(error = %err, "archive task failed");
                archive_state.set_failed(err.to_string()).await;
            }
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(ArchiveStartedResponse {
            message: "archive task started".to_string(),
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateArchiveSelectedRequest {
    dates: Vec<String>,
}

/// 创建归档：仅归档选定的日期（异步执行）
pub async fn create_archive_selected(
    State(state): State<AppState>,
    Json(req): Json<CreateArchiveSelectedRequest>,
) -> impl IntoResponse {
    // 检查是否有生成任务正在运行
    if state.queue.has_active_tasks().await {
        return (
            StatusCode::CONFLICT,
            "cannot create archive while generation tasks are running",
        )
            .into_response();
    }

    // 检查是否已有归档任务在运行
    if state.archive_state.is_running().await {
        return (StatusCode::CONFLICT, "archive task is already running").into_response();
    }

    let dates = req.dates;
    if dates.is_empty() {
        return (StatusCode::BAD_REQUEST, "no dates specified").into_response();
    }

    // 设置为运行中状态
    state
        .archive_state
        .set_running(format!("正在归档 {} 个日期...", dates.len()))
        .await;

    // 启动异步归档任务
    let gallery_dir = state.gallery_dir.clone();
    let storage = Arc::clone(&state.storage);
    let archive_state = state.archive_state.clone();

    tokio::spawn(async move {
        let manager = ArchiveManager::new(&gallery_dir, &storage);
        let result = manager.create_archives_for_dates(&dates).await;

        match result {
            Ok(res) => {
                tracing::info!(
                    archives = res.archives.len(),
                    deleted = res.deleted_records,
                    "archive task completed"
                );
                archive_state
                    .set_completed(res.archives, res.deleted_records)
                    .await;
            }
            Err(err) => {
                tracing::error!(error = %err, "archive task failed");
                archive_state.set_failed(err.to_string()).await;
            }
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(ArchiveStartedResponse {
            message: "archive task started".to_string(),
        }),
    )
        .into_response()
}

/// 下载归档文件
pub async fn download_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    use axum::body::Body;
    use axum::http::header;
    use tokio_util::io::ReaderStream;

    let gallery_dir = state.gallery_dir.clone();
    let storage = Arc::clone(&state.storage);
    let manager = ArchiveManager::new(&gallery_dir, &storage);

    let archive_path = match manager.get_archive_path(&name) {
        Ok(path) => path,
        Err(err) => {
            let status = if err.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            return (status, err.to_string()).into_response();
        }
    };

    match tokio::fs::File::open(&archive_path).await {
        Ok(file) => {
            let stream = ReaderStream::new(file);
            let body = Body::from_stream(stream);

            let headers = [
                (header::CONTENT_TYPE, "application/zip".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", name),
                ),
            ];

            (headers, body).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 删除归档文件
pub async fn delete_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let manager = ArchiveManager::new(&state.gallery_dir, &state.storage);

    match manager.delete_archive(&name).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            let status = if err.to_string().contains("invalid") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, err.to_string()).into_response()
        }
    }
}
