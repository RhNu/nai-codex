use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::{self, Engine, prelude::BASE64_STANDARD};
use codex_core::Snippet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, RenamePayload};

#[derive(Debug, Deserialize)]
pub struct SnippetQuery {
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

pub async fn list_snippets(
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
pub struct CreateSnippetPayload {
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
pub struct SnippetResponse {
    id: String,
    name: String,
    category: String,
}

pub async fn create_snippet(
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
pub struct UpdateSnippetPayload {
    name: Option<String>,
    category: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    description: Option<String>,
    preview_base64: Option<String>,
}

pub async fn update_snippet(
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

pub async fn get_snippet(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.get_snippet(id)).await {
        Ok(Ok(Some(snippet))) => Json(snippet).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "snippet not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn delete_snippet(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_snippet(id)).await {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "snippet not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdatePreviewPayload {
    preview_base64: String,
}

pub async fn update_snippet_preview(
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

pub async fn delete_snippet_preview(
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

pub async fn rename_snippet(
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
