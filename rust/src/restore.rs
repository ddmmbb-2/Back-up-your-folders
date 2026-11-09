use crate::{
    model::{RestoreMode, RestorePlan, SnapshotEntry, SnapshotMeta},
    path_guard::{zip_path, Guard},
    snapshot::{create_snapshot, read_snapshot},
};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::{HashMap, HashSet}, fs::{self, File, OpenOptions}, io::{self, Read, Write}, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use zip::ZipArchive;

#[derive(Clone)]
struct ArchiveItem {
    index: usize,
    path: PathBuf,
    key: String,
    is_dir: bool,
    size: u64,
}

fn norm(path: &str) -> String { path.to_lowercase() }

fn inspect(entry: &SnapshotEntry, guard: &Guard) -> Result<Vec<ArchiveItem>> {
    let mut zip = ZipArchive::new(File::open(&entry.path)?)?;
    let meta: SnapshotMeta = serde_json::from_slice(zip.comment())?;
    if meta.id != entry.meta.id || meta.schema_version != entry.meta.schema_version {
        bail!("版本檔在選取後遭到變更，請重新整理");
    }
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let mut files = 0u64;
    for i in 0..zip.len() {
        let item = zip.by_index(i)?;
        let is_dir = item.is_dir();
        if item.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            bail!("ZIP 含符號連結，拒絕還原");
        }
        let raw = item.name();
        let key = if is_dir { raw.strip_suffix('/').unwrap_or(raw) } else { raw };
        let relative = zip_path(key)?;
        guard.assure_unprotected(&relative)?;
        if !seen.insert(norm(key)) { bail!("ZIP 包含重複或大小寫衝突的路徑：{key}"); }
        if !is_dir { files += 1; total = total.checked_add(item.size()).context("ZIP 檔案大小溢位")?; }
        items.push(ArchiveItem { index: i, path: relative, key: key.to_owned(), is_dir, size: item.size() });
    }
    if files != meta.file_count || total != meta.source_bytes {
        bail!("ZIP 內容與版本中繼資料不一致");
    }
    let file_keys: HashSet<String> = items.iter().filter(|i| !i.is_dir).map(|i| norm(&i.key)).collect();
    for item in &items {
        for ancestor in item.path.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
            let rel = ancestor.to_string_lossy().replace('\\', "/");
            if file_keys.contains(&norm(&rel)) {
                bail!("ZIP 路徑衝突：檔案同時作為資料夾 {}", rel);
            }
        }
    }
    Ok(items)
}

fn digest_reader(mut reader: impl Read) -> Result<[u8; 32]> {
    let mut digest = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        digest.update(&buf[..n]);
    }
    Ok(digest.finalize().into())
}

pub fn preview(source: &Path, store: &Path, selected: SnapshotEntry) -> Result<RestorePlan> {
    let guard = Guard::new(source, store)?;
    let items = inspect(&selected, &guard)?;
    let disk = guard.inventory()?;
    let disk_files: HashMap<String, _> = disk.iter()
        .filter(|i| !i.is_dir)
        .map(|i| (norm(&i.key), i)).collect();
    let mut archive_files = HashSet::new();
    let mut archive_dirs = HashSet::new();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0usize;
    let mut zip = ZipArchive::new(File::open(&selected.path)?)?;
    for item in &items {
        if item.is_dir { archive_dirs.insert(norm(&item.key)); continue; }
        let key = norm(&item.key);
        archive_files.insert(key.clone());
        if let Some(existing) = disk_files.get(&key) {
            if existing.size == item.size {
                let from_zip = digest_reader(zip.by_index(item.index)?)?;
                let from_disk = digest_reader(File::open(guard.source.join(&existing.relative))?)?;
                if from_zip == from_disk {
                    unchanged += 1;
                    continue;
                }
            }
            changed.push(item.key.clone());
        } else {
            added.push(item.key.clone());
        }
        // ZIP paths may omit explicit directory entries.
    }
    for item in &items {
        for parent in item.path.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
            archive_dirs.insert(norm(&parent.to_string_lossy().replace('\\', "/")));
        }
    }
    let mut extra = Vec::new();
    for i in &disk {
        let key = norm(&i.key);
        if i.is_dir && !archive_dirs.contains(&key) && !guard.contains_protected_descendant(&guard.source.join(&i.relative)) {
            extra.push(format!("{}/", i.key));
        }
        if !i.is_dir && !archive_files.contains(&key) {
            extra.push(i.key.clone());
        }
    }
    extra.sort();
    Ok(RestorePlan { selected, added, changed, extra, unchanged, dirs: archive_dirs.len() })
}

/// Extracts to staging first. Before touching current files, optionally creates a rescue ZIP.
/// Per-file replacement is atomic; multi-file restoration is not transactionally atomic.
pub fn restore(
    source: &Path,
    store: &Path,
    plan: &RestorePlan,
    mode: RestoreMode,
    rescue: bool,
    on_progress: &mut dyn FnMut(&str, f32),
) -> Result<String> {
    let guard = Guard::new(source, store)?;
    let currently_selected = read_snapshot(&plan.selected.path)?;
    if currently_selected.meta.id != plan.selected.meta.id || currently_selected.zip_bytes != plan.selected.zip_bytes {
        bail!("備份檔已發生變動，請重新選擇版本");
    }
    let items = inspect(&plan.selected, &guard)?;
    on_progress("正在驗證 ZIP 並解壓至暫存目錄…", 0.04);
    let stage = tempfile::Builder::new().prefix(".folderversions_restore_").tempdir_in(&guard.store)?;
    let mut zip = ZipArchive::new(File::open(&plan.selected.path)?)?;
    let file_count = items.iter().filter(|i| !i.is_dir).count();
    let mut extracted = 0usize;
    for item in &items {
        let target = stage.path().join(&item.path);
        if item.is_dir {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
        let mut src = zip.by_index(item.index)?;
        let mut dst = OpenOptions::new().write(true).create_new(true).open(&target)?;
        let written = io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
        if written != item.size { bail!("解壓驗證失敗：{}", item.key); }
        extracted += 1;
        if extracted == file_count || extracted % 8 == 0 {
            on_progress(&format!("檢查內容：{extracted}/{file_count}"), 0.04 + 0.38 * extracted as f32 / file_count.max(1) as f32);
        }
    }
    // Inspect all destination paths BEFORE modifying anything.
    for item in &items {
        guard.assure_no_symlinks(&item.path)?;
        for parent in item.path.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
            let target_dir = guard.source.join(parent);
            if let Ok(meta) = fs::symlink_metadata(&target_dir) {
                if !meta.is_dir() { bail!("檔案與資料夾衝突：{}", parent.display()); }
            }
        }
        let dst = guard.source.join(&item.path);
        if let Ok(meta) = fs::symlink_metadata(&dst) {
            if item.is_dir && !meta.is_dir() { bail!("檔案與資料夾衝突：{}", item.key); }
            if !item.is_dir && !meta.is_file() { bail!("檔案與資料夾衝突：{}", item.key); }
        }
    }
    let delete_paths: Vec<PathBuf> = if mode == RestoreMode::Exact {
        let all_files: HashSet<String> = items.iter().filter(|i| !i.is_dir).map(|i| norm(&i.key)).collect();
        let mut all_dirs: HashSet<String> = items.iter().filter(|i| i.is_dir).map(|i| norm(&i.key)).collect();
        for item in &items {
            for anc in item.path.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
                all_dirs.insert(norm(&anc.to_string_lossy().replace('\\', "/")));
            }
        }
        let mut extra = Vec::new();
        for e in guard.inventory()? {
            if e.is_dir {
                if !all_dirs.contains(&norm(&e.key)) && !guard.contains_protected_descendant(&guard.source.join(&e.relative)) {
                    extra.push(e.relative);
                }
            } else if !all_files.contains(&norm(&e.key)) {
                extra.push(e.relative);
            }
        }
        extra
    } else { Vec::new() };
    for rel in &delete_paths { guard.assure_no_symlinks(rel)?; }
    let rescue_name = if rescue {
        on_progress("正在建立還原前安全備份…", 0.44);
        let before = create_snapshot(&guard.source, &guard.store,
            &format!("還原前安全備份：{}", plan.selected.meta.title.chars().take(70).collect::<String>()),
            &format!("即將還原至版本 {}", plan.selected.meta.id),
            &mut |text, progress| on_progress(text, 0.44 + 0.12 * progress),
        )?;
        Some(before.path.file_name().unwrap_or_default().to_string_lossy().to_string())
    } else { None };
    on_progress("正在寫入檔案…", 0.57);
    let mut directories: Vec<_> = items.iter().filter(|i| i.is_dir).collect();
    directories.sort_by_key(|i| i.path.components().count());
    for dir in directories { fs::create_dir_all(guard.source.join(&dir.path))?; }
    let files: Vec<_> = items.iter().filter(|i| !i.is_dir).collect();
    for (idx, item) in files.iter().enumerate() {
        let target = guard.source.join(&item.path);
        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
        guard.assure_no_symlinks(&item.path)?;
        atomic_copy(&stage.path().join(&item.path), &target)
            .with_context(|| format!("寫入失敗：{}", item.key))?;
        if idx % 8 == 0 || idx + 1 == files.len() {
            on_progress(&format!("還原檔案：{}/{}", idx + 1, files.len()),
                0.57 + (if mode == RestoreMode::Exact { 0.29 } else { 0.42 }) * (idx + 1) as f32 / files.len().max(1) as f32);
        }
    }
    if mode == RestoreMode::Exact {
        let mut deletions = delete_paths;
        deletions.sort_by_key(|rel| std::cmp::Reverse(rel.components().count()));
        for rel in &deletions {
            let full = guard.source.join(rel);
            guard.assure_no_symlinks(rel)?;
            if full.is_file() { fs::remove_file(&full).with_context(|| format!("無法移除舊檔：{}", rel.display()))?; }
            else if full.is_dir() {
                // Never recursively remove: it could contain an EXE or a protected backup directory.
                match fs::remove_dir(&full) {
                    Ok(()) => {},
                    Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => {},
                    Err(e) => return Err(e).with_context(|| format!("無法移除空資料夾：{}", rel.display())),
                }
            }
        }
    }
    on_progress("還原完成", 1.0);
    let mut message = format!("已還原：{}（{} 個檔案；{}）", plan.selected.meta.title, file_count,
        if mode == RestoreMode::Exact { "完整還原" } else { "覆蓋還原" });
    if let Some(name) = rescue_name { message.push_str(&format!("。還原前安全備份：{name}")); }
    Ok(message)
}

fn atomic_copy(from: &Path, to: &Path) -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let parent = to.parent().context("還原路徑沒有父目錄")?;
    let mut tmp = parent.join(format!(".folderversions-tmp-{nonce}-{}", std::process::id()));
    for attempt in 0..8 {
        if !tmp.exists() { break; }
        tmp = parent.join(format!(".folderversions-tmp-{nonce}-{attempt}"));
    }
    let result = (|| -> Result<()> {
        let mut input = File::open(from)?;
        let mut output = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
        io::copy(&mut input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        drop(output);
        fs::rename(&tmp, to).context("無法以新檔替換舊檔（可能被其他程式占用）")?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result
}
