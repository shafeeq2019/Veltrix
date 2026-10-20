//! Tauri commands: the typed interface between the React UI and the engine.

use crate::capture::{OfferResolution, OfferView, Offers};
use crate::native_host::{self, IntegrationStatus};
use crate::remote::{self, RemoteInfo, RemoteServer};
use crate::AppState;
use odm_core::category::is_executable_type;
use odm_core::model::{AddDownloadRequest, DownloadId, DownloadView, Priority, ProbeInfo};
use odm_core::{DownloadError, ErrorCode, Settings};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// Error shape returned to the frontend.
#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: ErrorCode,
    pub message: String,
}

impl From<DownloadError> for CmdError {
    fn from(e: DownloadError) -> Self {
        CmdError { code: e.code, message: e.message }
    }
}

fn internal(msg: impl ToString) -> CmdError {
    CmdError { code: ErrorCode::Internal, message: msg.to_string() }
}

type CmdResult<T> = Result<T, CmdError>;

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub queue_running: bool,
    pub bandwidth_estimate: u64,
    pub platform: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_dir.to_string_lossy().into(),
        queue_running: state.manager.queue_running(),
        bandwidth_estimate: state.manager.bandwidth_estimate(),
        platform: std::env::consts::OS.into(),
    }
}

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadView> {
    state.manager.list()
}

#[tauri::command]
pub fn add_download(state: State<'_, AppState>, request: AddDownloadRequest) -> CmdResult<DownloadView> {
    Ok(state.manager.add(request)?)
}

#[tauri::command]
pub async fn probe_url(
    state: State<'_, AppState>,
    url: String,
    referrer: Option<String>,
    username: Option<String>,
    password: Option<String>,
) -> CmdResult<ProbeInfo> {
    Ok(state.manager.probe(&url, referrer.as_deref(), username, password).await?)
}

#[tauri::command]
pub fn pause_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        state.manager.pause(id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn resume_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        state.manager.resume(id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        state.manager.cancel(id)?;
    }
    Ok(())
}

#[tauri::command]
pub fn retry_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>) -> CmdResult<()> {
    for id in ids {
        state.manager.retry(id)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_downloads(state: State<'_, AppState>, ids: Vec<DownloadId>, delete_files: bool) -> CmdResult<()> {
    for id in ids {
        state.manager.remove(id, delete_files).await?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_priority(state: State<'_, AppState>, ids: Vec<DownloadId>, priority: Priority) -> CmdResult<()> {
    for id in ids {
        state.manager.set_priority(id, priority)?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_in_queue(state: State<'_, AppState>, ids: Vec<DownloadId>, in_queue: bool) -> CmdResult<()> {
    for id in ids {
        state.manager.set_in_queue(id, in_queue)?;
    }
    Ok(())
}

#[tauri::command]
pub fn move_in_queue(state: State<'_, AppState>, id: DownloadId, delta: i64) -> CmdResult<()> {
    Ok(state.manager.move_in_queue(id, delta)?)
}

#[tauri::command]
pub fn set_credentials(
    state: State<'_, AppState>,
    id: DownloadId,
    username: String,
    password: Option<String>,
) -> CmdResult<()> {
    Ok(state.manager.set_credentials(id, username, password)?)
}

#[tauri::command]
pub fn start_queue(state: State<'_, AppState>) {
    state.manager.start_queue();
}

#[tauri::command]
pub fn stop_queue(state: State<'_, AppState>) {
    state.manager.stop_queue();
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.manager.settings()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: State<'_, RemoteServer>,
    mut settings: Settings,
) -> CmdResult<Settings> {
    let old = state.manager.settings();
    if settings.remote_view_enabled && settings.remote_view_token.trim().is_empty() {
        settings.remote_view_token = remote::new_token();
    }
    let new = state.manager.update_settings(settings)?;
    if new.chromium_extension_ids != old.chromium_extension_ids && native_host::host_executable().is_some() {
        let _ = native_host::register(&state.data_dir, &new.chromium_extension_ids);
    }
    if new.language != old.language {
        crate::tray::set_language(&app, crate::i18n::Lang::from_settings(&new));
    }
    remote.apply(&state.manager, &new);
    Ok(new)
}

#[tauri::command]
pub fn remote_view_info(state: State<'_, AppState>, remote: State<'_, RemoteServer>) -> RemoteInfo {
    remote.info(&state.manager.settings())
}

/// Issue a new secret; phones that scanned the old QR code lose access.
#[tauri::command]
pub fn remote_view_reset_token(state: State<'_, AppState>, remote: State<'_, RemoteServer>) -> CmdResult<Settings> {
    let mut s = state.manager.settings();
    s.remote_view_token = remote::new_token();
    let new = state.manager.update_settings(s)?;
    remote.apply(&state.manager, &new);
    Ok(new)
}

/// Open a finished file with the default application. Executable types
/// require `confirmed = true` (the UI asks the user first).
#[tauri::command]
pub fn open_file(app: AppHandle, state: State<'_, AppState>, id: DownloadId, confirmed: bool) -> CmdResult<()> {
    let path = state.manager.completed_path(id)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if is_executable_type(&name) && !confirmed {
        return Err(CmdError {
            code: ErrorCode::ConfirmationRequired,
            message: format!("{name} is a program. Opening it will run it."),
        });
    }
    tracing::info!(%id, "opening file on user request");
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(internal)
}

#[tauri::command]
pub fn show_in_folder(app: AppHandle, state: State<'_, AppState>, id: DownloadId) -> CmdResult<()> {
    let path = state.manager.location(id)?;
    if path.is_dir() {
        app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(internal)
    } else {
        app.opener().reveal_item_in_dir(&path).map_err(internal)
    }
}

#[tauri::command]
pub fn is_executable(file_name: String) -> bool {
    is_executable_type(&file_name)
}

#[tauri::command]
pub fn resolve_offer(
    state: State<'_, AppState>,
    offers: State<'_, Offers>,
    offer_id: String,
    resolution: OfferResolution,
) -> CmdResult<()> {
    Ok(offers.resolve(&state.manager, &offer_id, resolution)?)
}

#[tauri::command]
pub fn pending_offers(state: State<'_, AppState>, offers: State<'_, Offers>) -> Vec<OfferView> {
    offers.pending_views(&state.manager)
}

#[tauri::command]
pub fn integration_status(state: State<'_, AppState>) -> IntegrationStatus {
    native_host::status(&state.data_dir)
}

#[tauri::command]
pub fn register_browser_integration(state: State<'_, AppState>) -> CmdResult<IntegrationStatus> {
    native_host::register(&state.data_dir, &state.manager.settings().chromium_extension_ids).map_err(internal)
}

#[tauri::command]
pub fn unregister_browser_integration(state: State<'_, AppState>) -> IntegrationStatus {
    native_host::unregister(&state.data_dir)
}
