use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::{self, Engine, prelude::BASE64_STANDARD};
use codex_core::{CharacterPreset, MainPreset};
use serde::Deserialize;
use uuid::Uuid;

use crate::{AppState, RenamePayload, UpdatePreviewPayload};

#[derive(Debug, Deserialize)]
pub struct PresetQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    20
}

pub async fn list_presets(
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
pub struct CreatePresetPayload {
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

pub async fn create_preset(
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

pub async fn get_preset(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.get_preset(id)).await {
        Ok(Ok(Some(preset))) => Json(preset).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdatePresetPayload {
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

pub async fn update_preset(
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

pub async fn update_preset_preview(
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

pub async fn delete_preset_preview(
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

pub async fn delete_preset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.delete_preset(id)).await {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn rename_preset(
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

pub async fn list_main_presets(
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
pub struct CreateMainPresetPayload {
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

pub async fn create_main_preset(
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

pub async fn get_main_preset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let storage = Arc::clone(&state.storage);
    match tokio::task::spawn_blocking(move || storage.get_main_preset(id)).await {
        Ok(Ok(Some(preset))) => Json(preset).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "main preset not found").into_response(),
        Ok(Err(err)) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateMainPresetPayload {
    name: Option<String>,
    description: Option<String>,
    before: Option<String>,
    after: Option<String>,
    replace: Option<String>,
    uc_before: Option<String>,
    uc_after: Option<String>,
    uc_replace: Option<String>,
}

pub async fn update_main_preset(
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

pub async fn delete_main_preset(
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
