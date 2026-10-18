//! OpenDownload Manager desktop application (Tauri 2).

mod capture;
mod commands;
mod native_host;
mod tray;

use odm_core::model::EngineEvent;
use odm_core::{Manager, Store};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager as _, RunEvent, WindowEvent};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    pub manager: Manager,
    pub data_dir: PathBuf,
}

static EXITING: AtomicBool = AtomicBool::new(false);

fn init_logging(data_dir: &std::path::Path) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter = EnvFilter::try_from_env("ODM_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let log_dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("odm.log");
    // Simple size cap: start a fresh log when it grows beyond 5 MiB.
    if std::fs::metadata(&log_path).map(|m| m.len() > 5 * 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&log_path, log_dir.join("odm.log.1"));
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();
    let registry = tracing_subscriber::registry().with(filter).with(fmt::layer().with_writer(std::io::stderr));
    match file {
        Some(f) => {
            let _ = registry.with(fmt::layer().with_ansi(false).with_writer(std::sync::Mutex::new(f))).try_init();
        }
        None => {
            let _ = registry.try_init();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = odm_ipc::transport::app_data_dir();
    init_logging(&data_dir);
    let start_hidden = std::env::args().any(|a| a == "--background");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            capture::focus_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            let store = Store::open(&data_dir.join("odm.sqlite"))?;
            let manager = tauri::async_runtime::block_on(async { Manager::start(store) })?;
            let offers = capture::Offers::default();
            app.manage(AppState { manager: manager.clone(), data_dir: data_dir.clone() });
            app.manage(offers.clone());

            forward_events(app.handle().clone(), manager.clone());
            start_ipc(app.handle().clone(), manager.clone(), offers, data_dir.clone());
            tray::create(app.handle())?;

            if native_host::host_executable().is_some() {
                if let Err(e) = native_host::register(&data_dir, &manager.settings().chromium_extension_ids) {
                    tracing::warn!(error = %e, "browser integration not registered");
                }
            }
            if !start_hidden {
                capture::focus_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.manager.settings().minimize_to_tray && !EXITING.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_downloads,
            commands::add_download,
            commands::probe_url,
            commands::pause_downloads,
            commands::resume_downloads,
            commands::cancel_downloads,
            commands::retry_downloads,
            commands::remove_downloads,
            commands::set_priority,
            commands::set_in_queue,
            commands::move_in_queue,
            commands::set_credentials,
            commands::start_queue,
            commands::stop_queue,
            commands::get_settings,
            commands::update_settings,
            commands::open_file,
            commands::show_in_folder,
            commands::is_executable,
            commands::resolve_offer,
            commands::pending_offers,
            commands::integration_status,
            commands::register_browser_integration,
            commands::unregister_browser_integration,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                if !EXITING.swap(true, Ordering::SeqCst) {
                    // Pause running downloads and persist their state first.
                    api.prevent_exit();
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(state) = app.try_state::<AppState>() {
                            state.manager.shutdown().await;
                        }
                        app.exit(0);
                    });
                }
            }
        });
}

pub fn request_exit(app: &tauri::AppHandle) {
    EXITING.store(true, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(state) = app.try_state::<AppState>() {
            state.manager.shutdown().await;
        }
        app.exit(0);
    });
}

fn forward_events(app: tauri::AppHandle, manager: Manager) {
    let mut rx = manager.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!(n, "UI event stream lagged");
                    let _ = app.emit("engine-resync", ());
                    continue;
                }
                Err(_) => break,
            };
            let settings = manager.settings();
            match &event {
                EngineEvent::Completed { file_name, .. } if settings.notify_on_complete => {
                    let _ = app
                        .notification()
                        .builder()
                        .title("Download abgeschlossen")
                        .body(file_name)
                        .show();
                }
                EngineEvent::Failed { file_name, error, .. } if settings.notify_on_error => {
                    let _ = app
                        .notification()
                        .builder()
                        .title("Download fehlgeschlagen")
                        .body(format!("{file_name}: {}", error.message))
                        .show();
                }
                EngineEvent::Progress { total_speed, items } => {
                    tray::update_tooltip(&app, *total_speed, items.len());
                }
                _ => {}
            }
            let _ = app.emit("engine-event", &event);
        }
    });
}

fn start_ipc(app: tauri::AppHandle, manager: Manager, offers: capture::Offers, data_dir: PathBuf) {
    let handler: Arc<dyn odm_ipc::server::Handler> = Arc::new(
        move |origin: Option<String>, req: odm_ipc::Envelope<odm_ipc::Request>| -> odm_ipc::server::HandlerFuture {
            let (app, manager, offers) = (app.clone(), manager.clone(), offers.clone());
            Box::pin(async move { offers.handle(app, manager, origin, req).await })
        },
    );
    tauri::async_runtime::spawn(async move {
        if let Err(e) = odm_ipc::server::serve(&data_dir, handler).await {
            tracing::error!(error = %e, "browser integration endpoint unavailable");
        }
    });
}
