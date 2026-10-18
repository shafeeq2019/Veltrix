//! Handling of download offers coming from the browser extension.
//!
//! An offer either starts immediately (auto start / "always take over" for
//! its file type) or is shown to the user in the download dialog. The
//! browser keeps its own download paused until the decision arrives; when
//! the user ignores the offer, or nothing happens within the timeout, the
//! browser simply continues.

use odm_core::filename;
use odm_core::model::{AddDownloadRequest, StartMode};
use odm_core::{category, url_util, ErrorCode, Manager};
use odm_ipc::{CaptureConfig, Decision, Envelope, IpcErrorCode, Offer, OfferSource, Request, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager as _};
use tokio::sync::oneshot;

const DECISION_TIMEOUT: Duration = Duration::from_secs(120);

/// What the frontend shows in the capture dialog. Cookies are deliberately
/// not part of it.
#[derive(Debug, Clone, Serialize)]
pub struct OfferView {
    pub offer_id: String,
    pub url: String,
    pub domain: Option<String>,
    pub file_name: String,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub category: String,
    pub extension: Option<String>,
    pub save_dir: PathBuf,
    pub estimated_secs: Option<u64>,
    pub source: OfferSource,
    pub has_cookies: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OfferAction {
    Now,
    Queue,
    Ignore,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfferResolution {
    pub action: OfferAction,
    #[serde(default)]
    pub always_for_type: bool,
    pub file_name: Option<String>,
    pub save_dir: Option<PathBuf>,
}

struct Pending {
    offer: Offer,
    tx: oneshot::Sender<Decision>,
}

#[derive(Clone, Default)]
pub struct Offers {
    pending: Arc<Mutex<HashMap<String, Pending>>>,
}

pub fn capture_config(manager: &Manager) -> CaptureConfig {
    let c = manager.settings().capture;
    CaptureConfig {
        enabled: c.enabled,
        auto_start: c.auto_start,
        file_types: c.file_types,
        always_take_over: c.always_take_over,
        excluded_domains: c.excluded_domains,
        excluded_file_types: c.excluded_file_types,
        excluded_url_patterns: c.excluded_url_patterns,
        min_size_bytes: c.min_size_bytes,
        capture_private: c.capture_private,
        send_referrer: c.send_referrer,
    }
}

fn offer_file_name(o: &Offer) -> String {
    let url = url::Url::parse(o.final_url.as_deref().unwrap_or(&o.url)).ok();
    match o.filename.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(n) => filename::sanitize_file_name(n),
        None => filename::choose_file_name(None, url.as_ref(), None, o.mime.as_deref()),
    }
}

fn add_request(o: &Offer, start: StartMode, file_name: Option<String>, save_dir: Option<PathBuf>) -> AddDownloadRequest {
    AddDownloadRequest {
        url: o.url.clone(),
        file_name: Some(file_name.unwrap_or_else(|| offer_file_name(o))),
        save_dir,
        referrer: o.referrer.clone(),
        start,
        total_size: o.size,
        mime: o.mime.clone(),
        cookie_header: o.cookies.clone(),
        ..Default::default()
    }
}

impl Offers {
    /// Entry point for every authenticated IPC request.
    pub async fn handle(&self, app: AppHandle, manager: Manager, origin: Option<String>, req: Envelope<Request>) -> Response {
        match req.body {
            Request::Ping => Response::Pong { app_version: env!("CARGO_PKG_VERSION").into() },
            Request::GetConfig => Response::Config { config: capture_config(&manager) },
            Request::Offer(offer) => {
                tracing::info!(
                    origin = origin.as_deref().unwrap_or("?"),
                    url = %url_util::redact_url(&offer.url),
                    "download offered by browser"
                );
                self.handle_offer(app, manager, offer).await
            }
        }
    }

    async fn handle_offer(&self, app: AppHandle, manager: Manager, offer: Offer) -> Response {
        let settings = manager.settings();
        if offer.source == OfferSource::BrowserDownload && !settings.capture.enabled {
            return Response::Decision { decision: Decision::Ignored };
        }
        if manager.find_unfinished_by_url(&offer.url).is_some() {
            focus_main(&app);
            return Response::Decision { decision: Decision::Duplicate };
        }
        let name = offer_file_name(&offer);
        let ext = filename::extension_of(&name);
        let always = ext.as_ref().map(|e| settings.capture.always_take_over.contains(e)).unwrap_or(false);
        if offer.source == OfferSource::BrowserDownload && (settings.capture.auto_start || always) {
            return match manager.add(add_request(&offer, StartMode::Now, None, None)) {
                Ok(_) => Response::Decision { decision: Decision::Accepted },
                Err(e) if e.code == ErrorCode::Duplicate => Response::Decision { decision: Decision::Duplicate },
                Err(e) => Response::error(IpcErrorCode::Internal, e.message),
            };
        }

        let offer_id = uuid::Uuid::new_v4().simple().to_string();
        let estimate = manager.bandwidth_estimate();
        let view = OfferView {
            offer_id: offer_id.clone(),
            url: offer.url.clone(),
            domain: url_util::domain_of(&offer.url),
            category: category::classify(&name, offer.mime.as_deref(), &settings.custom_categories),
            extension: ext,
            file_name: name,
            size: offer.size,
            mime: offer.mime.clone(),
            save_dir: settings.download_dir.clone(),
            estimated_secs: match (offer.size, estimate) {
                (Some(s), e) if e > 0 => Some(s / e),
                _ => None,
            },
            source: offer.source,
            has_cookies: offer.cookies.is_some(),
        };
        let (tx, rx) = oneshot::channel();
        self.lock().insert(offer_id.clone(), Pending { offer, tx });
        focus_main(&app);
        if let Err(e) = app.emit("capture-offer", &view) {
            tracing::warn!(error = %e, "cannot show capture dialog");
        }
        let decision = match tokio::time::timeout(DECISION_TIMEOUT, rx).await {
            Ok(Ok(d)) => d,
            _ => {
                self.lock().remove(&offer_id);
                let _ = app.emit("capture-offer-expired", &offer_id);
                Decision::Ignored
            }
        };
        Response::Decision { decision }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.pending.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Called by the frontend when the user answered the dialog.
    pub fn resolve(&self, manager: &Manager, offer_id: &str, r: OfferResolution) -> Result<(), odm_core::DownloadError> {
        let Some(pending) = self.lock().remove(offer_id) else {
            return Err(odm_core::DownloadError::invalid_state("This download offer has expired"));
        };
        let decision = match r.action {
            OfferAction::Ignore => Decision::Ignored,
            OfferAction::Now | OfferAction::Queue => {
                let start = if r.action == OfferAction::Now { StartMode::Now } else { StartMode::Queue };
                let req = add_request(&pending.offer, start, r.file_name.clone(), r.save_dir.clone());
                match manager.add(req) {
                    Ok(_) => {
                        if r.action == OfferAction::Now {
                            Decision::Accepted
                        } else {
                            Decision::Queued
                        }
                    }
                    Err(e) => {
                        // Let the browser continue so nothing is lost.
                        let _ = pending.tx.send(Decision::Ignored);
                        return Err(e);
                    }
                }
            }
        };
        if r.always_for_type {
            let name = r.file_name.clone().unwrap_or_else(|| offer_file_name(&pending.offer));
            if let Some(ext) = filename::extension_of(&name) {
                let mut s = manager.settings();
                if !s.capture.always_take_over.contains(&ext) {
                    s.capture.always_take_over.push(ext);
                    let _ = manager.update_settings(s);
                }
            }
        }
        let _ = pending.tx.send(decision);
        Ok(())
    }

    /// Offers that are still waiting (e.g. window was reloaded).
    pub fn pending_views(&self, manager: &Manager) -> Vec<OfferView> {
        let settings = manager.settings();
        self.lock()
            .iter()
            .map(|(id, p)| {
                let name = offer_file_name(&p.offer);
                OfferView {
                    offer_id: id.clone(),
                    url: p.offer.url.clone(),
                    domain: url_util::domain_of(&p.offer.url),
                    category: category::classify(&name, p.offer.mime.as_deref(), &settings.custom_categories),
                    extension: filename::extension_of(&name),
                    file_name: name,
                    size: p.offer.size,
                    mime: p.offer.mime.clone(),
                    save_dir: settings.download_dir.clone(),
                    estimated_secs: None,
                    source: p.offer.source,
                    has_cookies: p.offer.cookies.is_some(),
                }
            })
            .collect()
    }
}

pub fn focus_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
