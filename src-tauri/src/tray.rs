//! System tray (Windows) / menu bar extra (macOS).

use crate::i18n::{tr, Lang, Text};
use crate::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "main-tray";

fn menu(app: &AppHandle, lang: Lang) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", tr(lang, Text::TrayShow), true, None::<&str>)?;
    let start = MenuItem::with_id(app, "queue_start", tr(lang, Text::TrayQueueStart), true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "queue_stop", tr(lang, Text::TrayQueueStop), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", tr(lang, Text::TrayQuit), true, None::<&str>)?;
    Menu::with_items(app, &[&show, &start, &stop, &separator, &quit])
}

pub fn create(app: &AppHandle, lang: Lang) -> tauri::Result<()> {
    let menu = menu(app, lang)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Veltrix")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => crate::capture::focus_main(app),
            "queue_start" => app.state::<AppState>().manager.start_queue(),
            "queue_stop" => app.state::<AppState>().manager.stop_queue(),
            "quit" => crate::request_exit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::capture::focus_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuild the menu after the language setting changed.
pub fn set_language(app: &AppHandle, lang: Lang) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match menu(app, lang) {
            Ok(m) => {
                let _ = tray.set_menu(Some(m));
            }
            Err(e) => tracing::warn!(error = %e, "tray menu could not be rebuilt"),
        }
    }
}

pub fn update_tooltip(app: &AppHandle, lang: Lang, total_speed: u64, active: usize) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let text = if active == 0 {
            "Veltrix".to_string()
        } else {
            tr(lang, Text::TrayActive)
                .replace("{active}", &active.to_string())
                .replace("{speed}", &human_bytes(total_speed, lang))
        };
        let _ = tray.set_tooltip(Some(text));
    }
}

fn human_bytes(b: u64, lang: Lang) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        let n = format!("{v:.1}");
        let n = if lang == Lang::De { n.replace('.', ",") } else { n };
        format!("{n} {}", UNITS[i])
    }
}
