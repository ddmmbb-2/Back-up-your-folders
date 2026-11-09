use anyhow::{anyhow, bail, Context, Result};
use std::{collections::HashSet, fs, path::{Component, Path, PathBuf}};
use walkdir::WalkDir;


/// Case-insensitive component comparison on Windows prevents .VERSIONS / drive-case bypasses.
fn is_under(path: &Path, base: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        let a: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
        let b: Vec<String> = base.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
        a.starts_with(&b)
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.starts_with(base)
    }
}

/// The backup store and portable EXE are never included in backups or removed by exact restore.
#[derive(Clone)]
pub struct Guard {
    pub source: PathBuf,
    pub store: PathBuf,
    pub exe: Option<PathBuf>,
}

impl Guard {
    pub fn new(source: &Path, store: &Path) -> Result<Self> {
        if !source.is_dir() {
            bail!("來源資料夾不存在：{}", source.display());
        }
        if !store.is_dir() {
            bail!("版本保存位置不存在：{}", store.display());
        }
        let source = fs::canonicalize(source).context("無法解析來源資料夾")?;
        let store = fs::canonicalize(store).context("無法解析備份資料夾")?;
        if is_under(&source, &store) && is_under(&store, &source) {
            bail!("版本儲存位置不可直接等於來源資料夾");
        }
        // A source directory within a backup store would cause unexpected recursion.
        if is_under(&source, &store) {
            bail!("來源資料夾不可位於版本保存位置之內");
        }
        let exe = std::env::current_exe().ok().and_then(|p| fs::canonicalize(p).ok());
        Ok(Self { source, store, exe })
    }

    pub fn protected(&self, path: &Path) -> bool {
        if is_under(&self.store, &self.source) && is_under(path, &self.store) {
            return true;
        }
        // Windows filesystems are generally case-insensitive; protect .VERSIONS too.
        if let Ok(rel) = path.strip_prefix(&self.source) {
            if let Some(first) = rel.components().next() {
                if first.as_os_str().to_string_lossy().eq_ignore_ascii_case(".versions") {
                    return true;
                }
            }
        }
        self.exe.as_ref().is_some_and(|exe| is_under(path, exe) && is_under(exe, path))
    }

    pub fn contains_protected_descendant(&self, path: &Path) -> bool {
        (is_under(&self.store, &self.source) && is_under(&self.store, path)) ||
        self.exe.as_ref().is_some_and(|exe| is_under(exe, path)) ||
        is_under(&self.source.join(".versions"), path)
    }

    pub fn inventory(&self) -> Result<Vec<DiskEntry>> {
        let mut entries = Vec::new();
        for result in WalkDir::new(&self.source)
            .min_depth(1)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.protected(e.path()))
        {
            let entry = result.context("無法掃描來源資料夾")?;
            if entry.file_type().is_symlink() {
                bail!("不支援符號連結或 junction，請先排除：{}", entry.path().display());
            }
            if !entry.file_type().is_file() && !entry.file_type().is_dir() {
                bail!("不支援特殊檔案：{}", entry.path().display());
            }
            let relative = entry.path().strip_prefix(&self.source)?;
            let key = relative_key(relative)?;
            let size = if entry.file_type().is_file() {
                entry.metadata().context("無法取得檔案大小")?.len()
            } else { 0 };
            entries.push(DiskEntry { relative: relative.to_path_buf(), key, is_dir: entry.file_type().is_dir(), size });
        }
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        let mut seen = HashSet::new();
        for e in &entries {
            if !seen.insert(e.key.to_lowercase()) {
                bail!("來源資料夾存在大小寫不同但名稱衝突的檔案：{}", e.key);
            }
        }
        Ok(entries)
    }

    pub fn assure_unprotected(&self, rel: &Path) -> Result<()> {
        let absolute = self.source.join(rel);
        if self.protected(&absolute) {
            bail!("快照包含保護中的路徑：{}", rel.display());
        }
        Ok(())
    }

    pub fn assure_no_symlinks(&self, rel: &Path) -> Result<()> {
        let mut current = self.source.clone();
        for segment in rel.components() {
            if let Component::Normal(name) = segment {
                current.push(name);
                match fs::symlink_metadata(&current) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        bail!("目標路徑含符號連結，不安全：{}", current.display());
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                    Err(e) => return Err(e).with_context(|| format!("無法檢查 {}", current.display())),
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DiskEntry {
    pub relative: PathBuf,
    pub key: String,
    pub is_dir: bool,
    pub size: u64,
}

pub fn relative_key(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(os) => {
                let s = os.to_str().ok_or_else(|| anyhow!("檔名不是 Unicode：{}", path.display()))?;
                validate_component(s)?;
                parts.push(s);
            }
            _ => bail!("無效的相對路徑：{}", path.display()),
        }
    }
    if parts.is_empty() { bail!("路徑不可為空"); }
    Ok(parts.join("/"))
}

pub fn zip_path(key: &str) -> Result<PathBuf> {
    if key.is_empty() || key.starts_with('/') || key.contains('\\') {
        bail!("ZIP 項目路徑不安全：{key}");
    }
    let mut result = PathBuf::new();
    for part in key.split('/') {
        validate_component(part)?;
        result.push(part);
    }
    Ok(result)
}

fn validate_component(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.ends_with('.') || name.ends_with(' ') {
        bail!("不安全的檔名：{name}");
    }
    if name.chars().any(|c| c.is_control() || "<>:\"|?*/\\".contains(c)) {
        bail!("檔名包含 Windows 不支援的字元：{name}");
    }
    // Windows device names are reserved even with an extension.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) ||
        (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) &&
         stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0') {
        bail!("Windows 保留檔名：{name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_traversal() {
        for bad in ["../file", "a/../file", "/absolute", "C:/bad", "a\\..\\bad", "a//b", "CON.txt", "name."] {
            assert!(zip_path(bad).is_err(), "allowed: {bad}");
        }
    }
    #[test]
    fn supports_chinese_names() {
        assert_eq!(zip_path("原始碼/測試.php").unwrap(), PathBuf::from("原始碼").join("測試.php"));
    }
}
