use std::io::{Cursor, Read};

use rand::Rng;
use zip::ZipArchive;

pub fn normalize_seed(seed: i64) -> u64 {
    if seed == -1 {
        let mut rng = rand::rng();
        rng.random_range(1_000_000_000u64..=9_999_999_999u64)
    } else {
        seed.max(0) as u64
    }
}

pub fn extract_file_by_name(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut file = archive.by_name(name).ok()?;
    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf).ok()?;
    Some(buf)
}

pub const fn default_true() -> bool {
    true
}
