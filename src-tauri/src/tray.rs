//! System tray (Windows) / menu bar extra (macOS).

use crate::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "main-tray";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "OpenDownload Manager öffnen", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "queue_start", "Warteschlange starten", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "queue_stop", "Warteschlange anhalten", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &start, &stop, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("OpenDownload Manager")
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

pub fn update_tooltip(app: &AppHandle, total_speed: u64, active: usize) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let text = if active == 0 {
            "OpenDownload Manager".to_string()
        } else {
            format!("OpenDownload Manager – {active} aktiv, {}/s", human_bytes(total_speed))
        };
        let _ = tray.set_tooltip(Some(text));
    }
}

fn human_bytes(b: u64) -> String {
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
        format!("{v:.1} {}", UNITS[i])
    }
}
