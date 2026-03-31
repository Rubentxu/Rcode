//! OpenCode Web UI - Tauri Entry Point
//!
//! This binary runs the Tauri desktop application which hosts the Leptos web UI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("OpenCode Web UI starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            tracing::info!("OpenCode Rust Desktop initialized");

            // Log the app data directory
            if let Some(app_data) = app.path().app_data_dir().ok() {
                tracing::info!("App data directory: {:?}", app_data);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
