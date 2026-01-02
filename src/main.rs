use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::Result;
use codex_server::{ServerConfig, serve};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let addr: SocketAddr = std::env::var("CODEX_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .unwrap_or_else(|_| SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8080)));

    let db_path =
        PathBuf::from(std::env::var("CODEX_DB_PATH").unwrap_or_else(|_| "data/codex.redb".into()));
    let preview_dir = PathBuf::from(
        std::env::var("CODEX_PREVIEW_DIR").unwrap_or_else(|_| "data/previews".into()),
    );
    let gallery_dir =
        PathBuf::from(std::env::var("CODEX_GALLERY_DIR").unwrap_or_else(|_| "data/gallery".into()));
    let static_dir = std::env::var("CODEX_STATIC_DIR").ok().map(PathBuf::from);
    let nai_token = std::env::var("CODEX_NAI_TOKEN").expect("CODEX_NAI_TOKEN required");

    let cfg = ServerConfig {
        addr,
        db_path,
        preview_dir,
        gallery_dir,
        static_dir,
        nai_token,
    };

    serve(cfg).await
}
