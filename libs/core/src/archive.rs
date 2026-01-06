use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::{CoreResult, CoreStorage};

/// 单个归档文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub name: String,
    pub size: u64,
    pub created_at: String,
}

/// 归档创建结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveResult {
    pub archives: Vec<ArchiveInfo>,
    pub deleted_records: usize,
}

/// 可归档的日期信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivableDate {
    pub date: String,
    pub image_count: usize,
    pub total_size: u64,
}

/// 归档管理器
pub struct ArchiveManager<'a> {
    gallery_dir: &'a Path,
    storage: &'a CoreStorage,
}

impl<'a> ArchiveManager<'a> {
    pub fn new(gallery_dir: &'a Path, storage: &'a CoreStorage) -> Self {
        Self {
            gallery_dir,
            storage,
        }
    }

    /// 列出所有归档文件
    pub async fn list_archives(&self) -> CoreResult<Vec<ArchiveInfo>> {
        let gallery_dir = self.gallery_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut archives = Vec::new();
            if !gallery_dir.exists() {
                return Ok(archives);
            }

            for entry in fs::read_dir(&gallery_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "zip" {
                            if let Some(name) = path.file_name() {
                                let metadata = fs::metadata(&path)?;
                                let created = metadata
                                    .created()
                                    .or_else(|_| metadata.modified())
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                let created_dt: chrono::DateTime<chrono::Local> = created.into();
                                archives.push(ArchiveInfo {
                                    name: name.to_string_lossy().to_string(),
                                    size: metadata.len(),
                                    created_at: created_dt.to_rfc3339(),
                                });
                            }
                        }
                    }
                }
            }

            // 按创建时间降序排列
            archives.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            Ok(archives)
        })
        .await
        .map_err(|e| anyhow!("join error: {e}"))?
    }

    /// 列出所有可归档的日期（今天之前的日期文件夹）
    pub async fn list_archivable_dates(&self) -> CoreResult<Vec<ArchivableDate>> {
        let gallery_dir = self.gallery_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let today = Local::now().format("%Y-%m-%d").to_string();
            let mut dates = Vec::new();

            if !gallery_dir.exists() {
                return Ok(dates);
            }

            for entry in fs::read_dir(&gallery_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_string();
                        // 检查是否是日期格式的文件夹（YYYY-MM-DD）
                        if name_str.len() == 10 && name_str.chars().nth(4) == Some('-') {
                            // 只包含今天之前的文件夹
                            if name_str.as_str() < today.as_str() {
                                // 统计文件数量和总大小
                                let mut image_count = 0;
                                let mut total_size = 0u64;
                                if let Ok(dir_entries) = fs::read_dir(&path) {
                                    for file_entry in dir_entries.flatten() {
                                        if file_entry.path().is_file() {
                                            image_count += 1;
                                            if let Ok(meta) = file_entry.metadata() {
                                                total_size += meta.len();
                                            }
                                        }
                                    }
                                }
                                dates.push(ArchivableDate {
                                    date: name_str,
                                    image_count,
                                    total_size,
                                });
                            }
                        }
                    }
                }
            }

            // 按日期降序排列（最新的在前）
            dates.sort_by(|a, b| b.date.cmp(&a.date));
            Ok(dates)
        })
        .await
        .map_err(|e| anyhow!("join error: {e}"))?
    }

    /// 创建归档：归档所有今天之前的日期
    pub async fn create_archives(&self) -> CoreResult<ArchiveResult> {
        let archivable = self.list_archivable_dates().await?;
        let dates: Vec<String> = archivable.into_iter().map(|d| d.date).collect();
        if dates.is_empty() {
            return Err(anyhow!(
                "no directories to archive (only today's images exist)"
            ));
        }
        self.create_archives_for_dates(&dates).await
    }

    /// 创建归档：仅归档指定的日期
    pub async fn create_archives_for_dates(&self, dates: &[String]) -> CoreResult<ArchiveResult> {
        use zip::write::SimpleFileOptions;

        if dates.is_empty() {
            return Err(anyhow!("no dates specified for archiving"));
        }

        let today = Local::now().format("%Y-%m-%d").to_string();
        let gallery_dir = self.gallery_dir.to_path_buf();
        let dates = dates.to_vec();

        // 在阻塞线程中执行压缩操作
        let (created_archives, dates_to_archive) = tokio::task::spawn_blocking(move || {
            // 验证并收集需要归档的日期文件夹
            let mut dirs_to_archive: Vec<PathBuf> = Vec::new();
            if !gallery_dir.exists() {
                return Err(anyhow!("gallery directory does not exist"));
            }

            for date in &dates {
                // 验证日期格式
                if date.len() != 10 || date.chars().nth(4) != Some('-') {
                    return Err(anyhow!("invalid date format: {}", date));
                }
                // 不能归档今天的
                if date.as_str() >= today.as_str() {
                    return Err(anyhow!("cannot archive today's or future dates: {}", date));
                }
                let dir_path = gallery_dir.join(date);
                if dir_path.exists() && dir_path.is_dir() {
                    dirs_to_archive.push(dir_path);
                }
            }

            if dirs_to_archive.is_empty() {
                return Err(anyhow!(
                    "no valid directories found for the specified dates"
                ));
            }

            // 按日期排序
            dirs_to_archive.sort();

            // 收集实际要归档的日期
            let dates_to_archive: Vec<String> = dirs_to_archive
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .collect();

            let mut created_archives = Vec::new();

            // 为每个日期创建单独的压缩包
            for dir in &dirs_to_archive {
                let date_str = dir.file_name().unwrap().to_string_lossy().to_string();
                let archive_name = format!("archive_{}.zip", date_str);
                let archive_path = gallery_dir.join(&archive_name);

                // 如果归档文件已存在，跳过该日期
                if archive_path.exists() {
                    info!(archive=%archive_name, "archive already exists, skipping");
                    continue;
                }

                // 创建 zip 文件
                let file = fs::File::create(&archive_path)?;
                let mut zip = zip::ZipWriter::new(file);

                let options = SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Zstd)
                    .compression_level(Some(19));

                // 添加该日期文件夹中的所有文件
                for entry in fs::read_dir(dir)? {
                    let entry = entry?;
                    let file_path = entry.path();
                    if file_path.is_file() {
                        let file_name = file_path.file_name().unwrap().to_string_lossy();
                        let zip_path = format!("{}/{}", date_str, file_name);

                        zip.start_file(&zip_path, options)?;
                        let f = fs::File::open(&file_path)?;
                        let mut reader = std::io::BufReader::with_capacity(128 * 1024, f);
                        std::io::copy(&mut reader, &mut zip)?;
                    }
                }

                zip.finish()?;

                // 删除已归档的文件夹
                fs::remove_dir_all(dir)?;

                // 记录归档信息
                let metadata = fs::metadata(&archive_path)?;
                let created_dt: chrono::DateTime<chrono::Local> =
                    std::time::SystemTime::now().into();
                created_archives.push(ArchiveInfo {
                    name: archive_name,
                    size: metadata.len(),
                    created_at: created_dt.to_rfc3339(),
                });

                info!(date=%date_str, "archived date folder");
            }

            Ok::<_, anyhow::Error>((created_archives, dates_to_archive))
        })
        .await
        .map_err(|e| anyhow!("join error: {e}"))??;

        // 删除数据库中对应日期的记录
        let deleted_records = self.delete_records_by_dates(&dates_to_archive).await?;
        info!(deleted=%deleted_records, dates=?dates_to_archive, "deleted archived records from database");

        Ok(ArchiveResult {
            archives: created_archives,
            deleted_records,
        })
    }

    /// 删除归档文件
    pub async fn delete_archive(&self, name: &str) -> CoreResult<bool> {
        // 安全检查：防止路径遍历攻击
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return Err(anyhow!("invalid archive name"));
        }

        // 确保是 .zip 文件
        if !name.ends_with(".zip") {
            return Err(anyhow!("invalid archive name"));
        }

        let archive_path = self.gallery_dir.join(name);
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            if !archive_path.exists() {
                return Ok(false);
            }

            fs::remove_file(&archive_path)?;
            info!(name=%name, "archive deleted");
            Ok(true)
        })
        .await
        .map_err(|e| anyhow!("join error: {e}"))?
    }

    /// 获取归档文件路径
    pub fn get_archive_path(&self, name: &str) -> CoreResult<PathBuf> {
        // 安全检查：防止路径遍历攻击
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return Err(anyhow!("invalid archive name"));
        }

        // 确保是 .zip 文件
        if !name.ends_with(".zip") {
            return Err(anyhow!("invalid archive name"));
        }

        let archive_path = self.gallery_dir.join(name);
        if !archive_path.exists() {
            return Err(anyhow!("archive not found"));
        }

        Ok(archive_path)
    }

    /// 删除指定日期范围内的所有记录（仅删除数据库记录）
    async fn delete_records_by_dates(&self, dates: &[String]) -> CoreResult<usize> {
        let storage = self.storage.clone();
        let dates = dates.to_vec();

        tokio::task::spawn_blocking(move || {
            // 获取所有记录
            let records = storage.list_recent_records(10000)?;

            // 找出需要删除的记录 ID
            let ids_to_delete: Vec<Uuid> = records
                .into_iter()
                .filter(|r| {
                    let record_date = r.created_at.format("%Y-%m-%d").to_string();
                    dates.contains(&record_date)
                })
                .map(|r| r.id)
                .collect();

            // 批量删除（不删除文件，因为文件已经被归档了）
            let mut deleted = 0;
            for id in &ids_to_delete {
                if storage.delete_record_without_files(*id)? {
                    deleted += 1;
                }
            }

            Ok(deleted)
        })
        .await
        .map_err(|e| anyhow!("join error: {e}"))?
    }
}
