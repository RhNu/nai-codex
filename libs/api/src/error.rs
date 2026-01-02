use thiserror::Error;

#[derive(Debug, Error)]
pub enum NaiError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected response status {status}: {body}")]
    BadStatus { status: u16, body: String },
    #[error("missing zip entry: {file_name}")]
    BadResult { file_name: String },
    #[error("general error: {msg}")]
    General { msg: String },
}

pub type NaiResult<T> = Result<T, NaiError>;
