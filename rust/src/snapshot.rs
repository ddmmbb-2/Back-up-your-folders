use crate::{model::{SnapshotEntry, SnapshotMeta, APP_ID, FORMAT_VERSION}, path_guard::Guard};
use anyhow::{bail, Context, Result};
use chrono::Local;
use std::{fs::{self, File, OpenOptions}, io::{self, Read, Write}, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// Snapshot metadata lives in ZIP archive comment, not inside the user's tree.
/// This lets any ordinary ZIP tool extract the original files unchanged.
pub fn read_snapshot(path: &Path) -> Result<SnapshotEntry> {
    let file = File::open(path).context("無法讀取 ZIP")?;
    let zip = ZipArchive::new(file).context("不是可讀取的 ZIP")?;
    let meta: SnapshotMeta = serde_json::from_slice(zip.comment()).context("ZIP 缺少有效版本備註")?;
    if meta.app != APP_ID || meta.schema_version != FORMAT_VERSION {
        bail!("不支援的快照格式");
    }
    if !meta.id.chars().all(|c| c.is_ascii_digit() || c == '-') || meta.id.len() > 40 {
        bail!("快照識別碼格式錯誤");
    }
    Ok(SnapshotEntry { path: path.to_path_buf(), zip_bytes: fs::metadata(path)?.len(), meta })
}

pub fn list_snapshots(store: &Path) -> Result<Vec<SnapshotEntry>> {
    if !store.is_dir() { bail!("版本目錄不存在：{}", store.display()); }
    let mut entries = Vec::new();
    for ent in fs::read_dir(store).context("無法掃描版本目錄")? {
        let ent = ent?;
        let path = ent.path();
        let is_zip = path.extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("zip"));
        if !is_zip || !ent.file_type()?.is_file() { continue; }
        if let Ok(info) = read_snapshot(&path) { entries.push(info); }
    }
    entries.sort_by(|a,b| b.meta.id.cmp(&a.meta.id));
    Ok(entries)
}

struct PartialFile(PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
}

pub fn create_snapshot(
    source: &Path,
    store: &Path,
    title: &str,
    description: &str,
    on_progress: &mut dyn FnMut(&str, f32),
) -> Result<SnapshotEntry> {
    if title.trim().is_empty() { bail!("請輸入版本名稱"); }
    if title.chars().count() > 120 || description.chars().count() > 4000 {
        bail!("版本名稱或描述過長");
    }
    fs::create_dir_all(store).context("無法建立版本保存位置")?;
    let guard = Guard::new(source, store)?;
    on_progress("掃描來源檔案…", 0.02);
    let files = guard.inventory()?;
    let file_count = files.iter().filter(|e| !e.is_dir).count() as u64;
    let source_bytes = files.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    let now = Local::now();
    let mut id = now.format("%Y%m%d-%H%M%S-%3f").to_string();
    let mut final_path = guard.store.join(format!("{id}.zip"));
    for attempt in 1..100 {
        if !final_path.exists() { break; }
        id = format!("{}-{attempt:02}", now.format("%Y%m%d-%H%M%S-%3f"));
        final_path = guard.store.join(format!("{id}.zip"));
    }
    if final_path.exists() { bail!("版本識別碼重複，請重試"); }
    let meta = SnapshotMeta {
        app: APP_ID.to_string(),
        schema_version: FORMAT_VERSION,
        id,
        created_at: now.to_rfc3339(),
        title: title.trim().to_owned(),
        description: description.trim().to_owned(),
        source: guard.source.to_string_lossy().to_string(),
        file_count,
        source_bytes,
    };
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let tmp_path = guard.store.join(format!(".folderversions-{nonce}.partial"));
    let tmp_file = OpenOptions::new().write(true).create_new(true).open(&tmp_path)?;
    let _cleanup = PartialFile(tmp_path.clone());
    let mut archive = ZipWriter::new(tmp_file);
    archive.set_comment(serde_json::to_string(&meta)?);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));
    let mut completed = 0u64;
    for entry in &files {
        let full_path = guard.source.join(&entry.relative);
        if entry.is_dir {
            archive.add_directory(format!("{}/", entry.key), options)
                .with_context(|| format!("無法封存資料夾：{}", entry.key))?;
        } else {
            // If a source changed during the scan, abort rather than silently record stale data.
            let before = fs::metadata(&full_path)?;
            if before.len() != entry.size {
                bail!("備份過程檔案大小發生變動，請重試：{}", entry.key);
            }
            archive.start_file(&entry.key, options)
                .with_context(|| format!("無法寫入 ZIP 項目：{}", entry.key))?;
            let mut input = File::open(&full_path)
                .with_context(|| format!("無法讀取：{}", full_path.display()))?;
            let written = io::copy(&mut input, &mut archive)?;
            if written != entry.size { bail!("備份時檔案大小改變：{}", entry.key); }
            completed += 1;
            if completed == file_count || completed % 8 == 0 {
                let progress = completed as f32 / file_count.max(1) as f32;
                on_progress(&format!("壓縮中：{completed}/{file_count} 個檔案"), 0.06 + 0.78 * progress);
            }
        }
    }
    on_progress("完成 ZIP，檢查壓縮內容…", 0.86);
    let zip_file = archive.finish().context("無法完成 ZIP")?;
    zip_file.sync_all().context("無法將備份寫入磁碟")?;
    drop(zip_file);
    validate_snapshot_archive(&tmp_path, &meta)?;
    if final_path.exists() { bail!("目標版本檔已存在，拒絕覆寫"); }
    fs::rename(&tmp_path, &final_path).context("無法將 ZIP 暫存檔轉為正式版本")?;
    on_progress("快照完成", 1.0);
    read_snapshot(&final_path)
}

fn validate_snapshot_archive(path: &Path, expected: &SnapshotMeta) -> Result<()> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    let actual: SnapshotMeta = serde_json::from_slice(zip.comment())?;
    if actual.id != expected.id || actual.file_count != expected.file_count { bail!("ZIP 版本資訊不一致"); }
    let mut count = 0u64;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut item = zip.by_index(i)?;
        if !item.is_dir() {
            count += 1;
            total += io::copy(&mut item, &mut io::sink())
                .with_context(|| format!("ZIP 完整性檢查失敗：{}", item.name()))?;
        }
    }
    if count != expected.file_count || total != expected.source_bytes {
        bail!("ZIP 驗證失敗：檔案數或解壓後總大小不一致");
    }
    Ok(())
}

pub fn delete_snapshot(entry: &SnapshotEntry, store: &Path) -> Result<()> {
    let store = fs::canonicalize(store)?;
    let file = fs::canonicalize(&entry.path)?;
    if file.parent() != Some(store.as_path()) || !file.is_file() {
        bail!("拒絕刪除版本目錄外的檔案");
    }
    let actual = read_snapshot(&file)?;
    if actual.meta.id != entry.meta.id { bail!("版本識別資料不符，拒絕刪除"); }
    fs::remove_file(&file).context("刪除版本失敗")
}
