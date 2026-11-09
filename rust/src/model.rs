use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APP_ID: &str = "folder-versions";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub app: String,
    pub schema_version: u32,
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub description: String,
    pub source: String,
    pub file_count: u64,
    pub source_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct SnapshotEntry {
    pub path: PathBuf,
    pub meta: SnapshotMeta,
    pub zip_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    Overwrite,
    Exact,
}

#[derive(Debug, Clone)]
pub struct RestorePlan {
    pub selected: SnapshotEntry,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub extra: Vec<String>,
    pub unchanged: usize,
    pub dirs: usize,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Progress { label: String, fraction: f32 },
    Listed(Result<Vec<SnapshotEntry>, String>),
    BackedUp(Result<SnapshotEntry, String>),
    Restored(Result<String, String>),
}

