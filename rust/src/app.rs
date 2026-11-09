use crate::{
    model::{RestoreMode, SnapshotEntry, WorkerEvent},
    restore, snapshot,
};
use chrono::DateTime;
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};
use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Backup,
    Restore,
}

pub struct FolderApp {
    // Portable mode: both paths come ONLY from the running EXE; no settings file.
    source: PathBuf,
    store: PathBuf,
    page: Page,
    title: String,
    description: String,
    snapshots: Vec<SnapshotEntry>,
    selected_id: Option<String>,
    status: String,
    error: bool,
    busy: bool,
    progress: f32,
    progress_label: String,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
}

impl FolderApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let source = std::env::current_exe()
            .expect("無法判斷程式所在位置")
            .parent()
            .expect("EXE 沒有父資料夾")
            .to_path_buf();
        let store = source.join(".versions");
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            source,
            store,
            page: Page::Backup,
            title: String::new(),
            description: String::new(),
            snapshots: Vec::new(),
            selected_id: None,
            status: String::new(),
            error: false,
            busy: false,
            progress: 0.0,
            progress_label: String::new(),
            tx,
            rx,
        };
        app.start_list(&cc.egui_ctx);
        app
    }

    fn note_error(&mut self, message: String) {
        self.error = true;
        self.status = message;
    }

    fn note_ok(&mut self, message: String) {
        self.error = false;
        self.status = message;
    }

    fn begin(&mut self, label: &str) {
        self.busy = true;
        self.progress = 0.0;
        self.progress_label = label.to_owned();
        self.status.clear();
        self.error = false;
    }

    fn start_list(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        if !self.store.is_dir() {
            self.snapshots.clear();
            self.selected_id = None;
            return;
        }
        // Reloading after success must not erase the user-facing result message.
        self.busy = true;
        self.progress = 0.0;
        self.progress_label = "讀取版本…".to_owned();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let store = self.store.clone();
        thread::spawn(move || {
            let result = snapshot::list_snapshots(&store).map_err(|e| format!("{e:#}"));
            let _ = tx.send(WorkerEvent::Listed(result));
            ctx.request_repaint();
        });
    }

    fn start_backup(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        if self.title.trim().is_empty() {
            self.note_error("請輸入名稱。".to_owned());
            return;
        }
        self.begin("正在建立快照…");
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let source = self.source.clone();
        let store = self.store.clone();
        let title = self.title.clone();
        let description = self.description.clone();
        thread::spawn(move || {
            let result = snapshot::create_snapshot(
                &source, &store, &title, &description,
                &mut |label, fraction| {
                    let _ = tx.send(WorkerEvent::Progress {
                        label: label.to_owned(), fraction,
                    });
                    ctx.request_repaint();
                },
            ).map_err(|e| format!("{e:#}"));
            let _ = tx.send(WorkerEvent::BackedUp(result));
            ctx.request_repaint();
        });
    }

    fn start_restore(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(selected) = self.selected_id.as_ref().and_then(|id|
            self.snapshots.iter().find(|e| &e.meta.id == id)).cloned()
        else {
            self.note_error("請先選擇版本。".to_owned());
            return;
        };
        self.begin("正在準備還原…");
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let source = self.source.clone();
        let store = self.store.clone();
        thread::spawn(move || {
            // Always validate and compare in the worker before restoring.
            // The user-facing action is intentionally only one click.
            let result = (|| -> anyhow::Result<String> {
                let plan = restore::preview(&source, &store, selected)?;
                restore::restore(
                    &source, &store, &plan,
                    RestoreMode::Overwrite,
                    true, // A rescue snapshot is automatic and cannot be disabled in this UI.
                    &mut |label, fraction| {
                        let _ = tx.send(WorkerEvent::Progress {
                            label: label.to_owned(), fraction,
                        });
                        ctx.request_repaint();
                    },
                )
            })().map_err(|e| format!("{e:#}"));
            let _ = tx.send(WorkerEvent::Restored(result));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let mut refresh = false;
        while let Ok(event) = self.rx.try_recv() {
            match event {
                WorkerEvent::Progress { label, fraction } => {
                    self.progress_label = label;
                    self.progress = fraction.clamp(0.0, 1.0);
                }
                WorkerEvent::Listed(result) => {
                    self.busy = false;
                    match result {
                        Ok(items) => {
                            self.snapshots = items;
                            // Keep an existing selection only if it still exists.
                            if self.selected_id.as_ref().is_some_and(|id|
                                !self.snapshots.iter().any(|e| &e.meta.id == id)) {
                                self.selected_id = None;
                            }
                        }
                        Err(e) => self.note_error(format!("讀取版本失敗：{e}")),
                    }
                }
                WorkerEvent::BackedUp(result) => {
                    self.busy = false;
                    match result {
                        Ok(item) => {
                            self.note_ok(format!("存檔完成：{}", item.meta.title));
                            self.title.clear();
                            self.description.clear();
                            refresh = true;
                        }
                        Err(e) => self.note_error(format!("存檔失敗：{e}")),
                    }
                }
                WorkerEvent::Restored(result) => {
                    self.busy = false;
                    match result {
                        Ok(_) => {
                            self.note_ok("恢復完成（還原前狀態已自動存成快照）。".to_owned());
                            refresh = true;
                        }
                        Err(e) => {
                            self.note_error(format!(
                                "恢復未完成：{e}。若有檔案已被覆蓋，可從還原前安全備份再次恢復。"
                            ));
                            refresh = true;
                        }
                    }
                }
            }
        }
        if refresh {
            self.start_list(ctx);
        }
    }

    fn draw_backup(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("名稱").size(16.0));
        ui.add_enabled(!self.busy, egui::TextEdit::singleline(&mut self.title)
            .hint_text("例如：新增登入功能")
            .desired_width(f32::INFINITY));

        ui.add_space(15.0);
        ui.label(egui::RichText::new("修改內容").size(16.0));
        ui.add_enabled(!self.busy, egui::TextEdit::multiline(&mut self.description)
            .hint_text("本次修改了什麼？（可留空）")
            .desired_rows(5)
            .desired_width(f32::INFINITY));

        ui.add_space(18.0);
        if ui.add_enabled(!self.busy, egui::Button::new("存檔")
            .min_size(egui::vec2(140.0, 38.0))).clicked() {
            self.start_backup(ctx);
        }
    }

    fn draw_restore(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(16.0);
        if self.snapshots.is_empty() {
            ui.label("目前沒有快照。");
            return;
        }
        ui.label(egui::RichText::new("選擇版本").size(16.0));
        ui.add_space(6.0);
        let current_label = self.selected_id.as_ref()
            .and_then(|id| self.snapshots.iter().find(|e| &e.meta.id == id))
            .map(version_label)
            .unwrap_or_else(|| "請選擇…".to_owned());
        ui.add_enabled_ui(!self.busy, |ui| {
            egui::ComboBox::from_id_salt("snapshot-choice")
                .width(ui.available_width().min(540.0))
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for entry in &self.snapshots {
                        ui.selectable_value(
                            &mut self.selected_id,
                            Some(entry.meta.id.clone()),
                            version_label(entry),
                        );
                    }
                });
        });
        if let Some(entry) = self.selected_id.as_ref()
            .and_then(|id| self.snapshots.iter().find(|e| &e.meta.id == id)) {
            if !entry.meta.description.is_empty() {
                ui.add_space(12.0);
                ui.label(entry.meta.description.as_str());
            }
        }
        ui.add_space(18.0);
        if ui.add_enabled(!self.busy && self.selected_id.is_some(),
            egui::Button::new("恢復").min_size(egui::vec2(140.0, 38.0))).clicked() {
            self.start_restore(ctx);
        }
        ui.add_space(9.0);
        ui.label(egui::RichText::new("將覆蓋同名檔案，其他檔案不刪除；恢復前會自動備份現況。")
            .small().color(Color32::GRAY));
    }
}

impl eframe::App for FolderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Folder Versions");
            ui.add_space(15.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.page, Page::Backup, "快照備份");
                ui.add_space(18.0);
                ui.selectable_value(&mut self.page, Page::Restore, "復原");
            });
            ui.separator();
            match self.page {
                Page::Backup => self.draw_backup(ui, ctx),
                Page::Restore => self.draw_restore(ui, ctx),
            }
            if self.busy {
                ui.add_space(20.0);
                ui.label(self.progress_label.as_str());
                ui.add(egui::ProgressBar::new(self.progress).show_percentage());
            }
            if !self.status.is_empty() {
                ui.add_space(14.0);
                let color = if self.error {
                    Color32::from_rgb(255, 130, 130)
                } else {
                    Color32::from_rgb(142, 211, 160)
                };
                ui.colored_label(color, self.status.as_str());
            }
        });
    }
}

fn version_label(entry: &SnapshotEntry) -> String {
    let timestamp = DateTime::parse_from_rfc3339(&entry.meta.created_at)
        .map(|dt| dt.format("%Y/%m/%d %H:%M").to_string())
        .unwrap_or_else(|_| entry.meta.id.clone());
    format!("{timestamp}  ·  {}", entry.meta.title)
}

fn configure_fonts(ctx: &egui::Context) {
    let mut candidates = Vec::new();
    if let Some(windir) = std::env::var_os("WINDIR") {
        let fonts = PathBuf::from(windir).join("Fonts");
        for f in ["msjh.ttc", "msjh.ttf", "msyh.ttc", "mingliu.ttc", "simsun.ttc"] {
            candidates.push(fonts.join(f));
        }
    }
    candidates.push(PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"));
    candidates.push(PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"));
    if let Some(data) = candidates.into_iter().find_map(|p| fs::read(p).ok()) {
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert("system_cjk".to_owned(), FontData::from_owned(data).into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().insert(0, "system_cjk".to_owned());
        }
        ctx.set_fonts(fonts);
    }
}
