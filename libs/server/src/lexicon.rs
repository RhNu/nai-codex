use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::AppState;

pub async fn get_lexicon_index(State(state): State<AppState>) -> impl IntoResponse {
    match &state.lexicon {
        Some(lex) => Json(lex.get_index().clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "lexicon not loaded").into_response(),
    }
}

pub async fn get_lexicon_category(
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
pub struct LexiconSearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_search_limit() -> usize {
    50
}

pub async fn search_lexicon(
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
