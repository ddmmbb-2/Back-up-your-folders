#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod model;
mod path_guard;
mod restore;
mod snapshot;

use eframe::egui;

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([580.0, 410.0])
            .with_min_inner_size([480.0, 350.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Folder Versions v0.1",
        opts,
        Box::new(|cc| Ok(Box::new(app::FolderApp::new(cc)))),
    )
}

#[cfg(test)]
mod test_workflow;
