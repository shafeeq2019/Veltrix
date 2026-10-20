//! The few texts the native side shows itself (tray menu, tooltip,
//! notifications). Everything else is translated in the web UI (`src/i18n`).

use odm_core::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

#[derive(Debug, Clone, Copy)]
pub enum Text {
    TrayShow,
    TrayQueueStart,
    TrayQueueStop,
    TrayQuit,
    /// Tooltip while downloads run; `{active}` and `{speed}` are replaced.
    TrayActive,
    NotifyCompleted,
    NotifyFailed,
    /// `{port}` and `{error}` are replaced.
    PortUnavailable,
}

impl Lang {
    /// Language from the setting; "system" (or anything unknown) follows the OS.
    pub fn from_settings(settings: &Settings) -> Self {
        match settings.language.as_str() {
            "de" => Lang::De,
            "en" => Lang::En,
            _ => Self::system(),
        }
    }

    fn system() -> Self {
        let tag = sys_locale::get_locale().or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        });
        tag.map(|t| Self::from_tag(&t)).unwrap_or(Lang::En)
    }

    /// "de-DE", "de_AT.UTF-8", … ⇒ German; everything else ⇒ English.
    fn from_tag(tag: &str) -> Self {
        if tag.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("de")) {
            Lang::De
        } else {
            Lang::En
        }
    }
}

pub fn tr(lang: Lang, text: Text) -> &'static str {
    use Text::*;
    match (lang, text) {
        (Lang::De, TrayShow) => "OpenDownload Manager öffnen",
        (Lang::En, TrayShow) => "Open OpenDownload Manager",
        (Lang::De, TrayQueueStart) => "Warteschlange starten",
        (Lang::En, TrayQueueStart) => "Start queue",
        (Lang::De, TrayQueueStop) => "Warteschlange anhalten",
        (Lang::En, TrayQueueStop) => "Stop queue",
        (Lang::De, TrayQuit) => "Beenden",
        (Lang::En, TrayQuit) => "Quit",
        (Lang::De, TrayActive) => "OpenDownload Manager – {active} aktiv, {speed}/s",
        (Lang::En, TrayActive) => "OpenDownload Manager – {active} active, {speed}/s",
        (Lang::De, NotifyCompleted) => "Download abgeschlossen",
        (Lang::En, NotifyCompleted) => "Download complete",
        (Lang::De, NotifyFailed) => "Download fehlgeschlagen",
        (Lang::En, NotifyFailed) => "Download failed",
        (Lang::De, PortUnavailable) => "Port {port} ist nicht verfügbar: {error}",
        (Lang::En, PortUnavailable) => "Port {port} is not available: {error}",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_locale_tags() {
        assert_eq!(Lang::from_tag("de-DE"), Lang::De);
        assert_eq!(Lang::from_tag("de_AT.UTF-8"), Lang::De);
        assert_eq!(Lang::from_tag("DE"), Lang::De);
        assert_eq!(Lang::from_tag("en-US"), Lang::En);
        assert_eq!(Lang::from_tag("fr_FR"), Lang::En);
        assert_eq!(Lang::from_tag(""), Lang::En);
    }

    #[test]
    fn explicit_setting_wins() {
        let mut s = Settings { language: "de".into(), ..Default::default() };
        assert_eq!(Lang::from_settings(&s), Lang::De);
        s.language = "en".into();
        assert_eq!(Lang::from_settings(&s), Lang::En);
        assert_eq!(tr(Lang::En, Text::NotifyFailed), "Download failed");
    }
}
