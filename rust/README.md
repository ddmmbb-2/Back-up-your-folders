# Folder Versions v0.1（極簡版）

Windows 用 Rust 撰寫的單一 EXE 離線快照工具。不用設定、不用 Git、不用網路。

## 操作

把 `folder-versions.exe` 放進要備份的資料夾，雙擊執行。

- **快照備份**（開啟預設頁面）：輸入「名稱」、「修改內容（選填）」，按「存檔」。
- **復原**：從下拉清單挑一份歷史快照，按「恢復」。

不需要也不能在 GUI 更換來源資料夾或存檔位置。程式永遠以 **EXE 所在資料夾** 為來源，自動在同一層建立 `.versions` 子資料夾放 ZIP 快照。備份會排除 `.versions` 及目前執行中的 EXE。搬動 EXE 到別的專案，就會管理新位置的檔案；各專案的歷史版本互不干擾。

```
myproject/
├── folder-versions.exe
├── index.php
├── src/
└── .versions/
    ├── 20261109-112000-123.zip
    └── 20261109-140000-456.zip
```

「恢復」採用**覆蓋還原**：同名檔案會回復成快照中的內容；快照裡沒有的現有檔案**保留、不刪除**。為防止按錯，還原前程式會自動建立一份「還原前安全備份」，在 `.versions` 裡一樣能找到。操作中會顯示進度與結果；ZIP 會先驗證內容，再執行還原。

## Windows 建置

安裝 Rust stable 及適用的 MSVC C++ Build Tools 後，雙擊 `build_windows.bat`，或在專案根目錄執行：

```powershell
cargo build --release
```

EXE 輸出：`target/release/folder-versions.exe`。發行版採靜態 CRT 設定；目標是單一免安裝 EXE，不用額外安裝 Rust/Python/Git。也可將原始碼放 GitHub，手動啟動 `Actions > Build Windows EXE` 取得 Windows 執行檔。

> 使用 `cargo run` 時，程式會認定 `target/debug/` 是要備份的資料夾；請在正式測試時將編譯完成的 EXE 複製到測試專案資料夾。

## 重要限制

- 不支援 symlink / junction；發現時直接顯示錯誤。
- 遇到檔案與資料夾同名衝突，拒絕覆蓋。
- ZIP 與來源在同一磁碟時，無法防範整顆磁碟故障。
- 還原前先停用會寫入該資料夾的程式（例如本機伺服器）。
- 整個資料夾的多檔覆蓋不是原子操作；斷電或權限不足可能只恢復一部分。自動安全備份用來協助復原。
- 若需要把整個資料夾**完全變回**某版本（連新增檔案都刪除），目前的「覆蓋還原」不會做到。這版優先遵照簡單且不刪檔的操作。

## 專案模組

`src/app.rs` GUI／工作流程；`snapshot.rs` ZIP 快照；`restore.rs` 還原；`path_guard.rs` 路徑保護；`model.rs` 資料模型。全部離線、無資料庫與登入。

## 驗證狀態

本環境尚未安裝 Rust 編譯器，因此未執行 `cargo build` / `cargo test`，也尚未完成 Windows 實測。請先使用測試資料夾檢驗備份與復原結果。
