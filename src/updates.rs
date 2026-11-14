//! Read-only release discovery. Download/install remains an explicit external action.
use serde::{Deserialize, Serialize};
use std::{
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(any(target_os = "macos", test))]
#[path = "updates/github.rs"]
mod github;
#[cfg(target_os = "windows")]
#[path = "updates/windows.rs"]
mod windows;

pub const RELEASES_URL: &str = "https://github.com/gokcol/MyImage/releases";
pub const STORE_URL: &str = "https://apps.microsoft.com/detail/9PL3HWB23J38";
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Deserialize)]
pub struct ReleaseNotes {
    pub version: String,
    pub build: u32,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub store_version: String,
    pub changes: Vec<String>,
    pub macos: Vec<String>,
    pub windows: Vec<String>,
}

pub fn notes() -> &'static ReleaseNotes {
    static NOTES: std::sync::OnceLock<ReleaseNotes> = std::sync::OnceLock::new();
    NOTES.get_or_init(|| {
        serde_json::from_str(include_str!("../release-notes.json"))
            .expect("validated bundled release notes")
    })
}

pub fn release_id() -> String {
    format!("v{}-build{}", notes().version, notes().build)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub automatic: bool,
    pub last_attempt: Option<u64>,
    pub seen_release: Option<String>,
    pub dismissed_update: Option<String>,
}

impl Preferences {
    pub fn due(&self, now: u64) -> bool {
        self.automatic
            && self
                .last_attempt
                .is_none_or(|last| now < last || now - last >= DAY)
    }

    pub fn unseen_upgrade(&self, existing_user: bool) -> bool {
        self.seen_release
            .as_ref()
            .map_or(existing_user, |seen| seen != &release_id())
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug)]
pub enum Outcome {
    Current,
    Available {
        id: String,
        title: String,
        notes: String,
        url: String,
    },
    #[cfg(any(target_os = "windows", test))]
    StoreInstallRequired,
}

#[derive(Default)]
pub struct State {
    pub open: bool,
    pub result: Option<Result<Outcome, String>>,
    receiver: Option<mpsc::Receiver<Result<Outcome, String>>>,
    started: Option<Instant>,
    // Do not start a second native call if a timed-out worker hasn't returned.
    timed_out: bool,
    pub pending_notes: bool,
}

impl State {
    pub fn new(preferences: &Preferences, existing_user: bool) -> Self {
        Self {
            pending_notes: preferences.unseen_upgrade(existing_user),
            ..Self::default()
        }
    }

    pub fn checking(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn start(&mut self, ctx: &eframe::egui::Context, parent: Option<isize>) {
        if self.checking() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.result = None;
        self.started = Some(Instant::now());
        self.timed_out = false;
        let ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("release-check".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                let result = {
                    let _ = parent;
                    github::check()
                };
                #[cfg(target_os = "windows")]
                let result = windows::check(parent);
                let _ = tx.send(result);
                ctx.request_repaint();
            }) {
            Ok(_) => self.receiver = Some(rx),
            Err(_) => {
                self.result = Some(Err("Could not start the update check. Try again.".into()))
            }
        }
    }

    pub fn poll(&mut self, ctx: &eframe::egui::Context) {
        let Some(rx) = &self.receiver else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                if !self.timed_out {
                    self.result = Some(result);
                }
                self.receiver = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.result = Some(Err("The update check stopped. Try again.".into()));
                self.receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                if !self.timed_out && self.started.is_some_and(|t| t.elapsed() > CHECK_TIMEOUT) {
                    self.timed_out = true;
                    self.result = Some(Err("The update service did not respond in time. You can open the release page instead.".into()));
                }
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }
    }

    pub fn notification(&self, preferences: &Preferences) -> Option<&str> {
        match self.result.as_ref()? {
            Ok(Outcome::Available { id, .. })
                if preferences.dismissed_update.as_ref() != Some(id) =>
            {
                Some(id)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_metadata_matches_both_packages() {
        let n = notes();
        assert_eq!(n.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            n.store_version,
            include_str!("../packaging/windows/msix/version.txt").trim()
        );
        let plist = include_str!("../packaging/macos/Info.plist");
        for (key, expected) in [
            ("CFBundleVersion", n.build.to_string()),
            ("CFBundleShortVersionString", n.version.clone()),
        ] {
            let value = plist
                .split_once(&format!("<key>{key}</key>"))
                .unwrap()
                .1
                .split_once("<string>")
                .unwrap()
                .1
                .split_once("</string>")
                .unwrap()
                .0;
            assert_eq!(value, expected);
        }
        assert!(!n.changes.is_empty() && !n.macos.is_empty() && !n.windows.is_empty());
    }

    #[test]
    fn checks_are_opt_in_throttled_and_tolerate_clock_rollback() {
        let mut p: Preferences = serde_json::from_str("{}").unwrap();
        assert!(!p.due(DAY * 2));
        p.automatic = true;
        assert!(p.due(DAY));
        p.last_attempt = Some(DAY);
        assert!(!p.due(DAY + 1));
        assert!(p.due(DAY * 2));
        assert!(p.due(DAY - 1));
    }

    #[test]
    fn upgrade_notes_and_dismissal_are_per_release() {
        let mut p = Preferences::default();
        assert!(!p.unseen_upgrade(false));
        assert!(p.unseen_upgrade(true));
        p.seen_release = Some(release_id());
        assert!(!p.unseen_upgrade(true));
        p.seen_release = Some("v2.2.1-build100".into());
        assert!(p.unseen_upgrade(true));
        let state = State {
            result: Some(Ok(Outcome::Available {
                id: "new".into(),
                title: "New".into(),
                notes: String::new(),
                url: STORE_URL.into(),
            })),
            ..State::default()
        };
        assert_eq!(state.notification(&p), Some("new"));
        p.dismissed_update = Some("new".into());
        assert!(state.notification(&p).is_none());
        p.dismissed_update = Some("old".into());
        assert!(state.notification(&p).is_some());
        let state = State {
            result: Some(Ok(Outcome::StoreInstallRequired)),
            ..State::default()
        };
        assert!(state.notification(&p).is_none());
    }

    #[test]
    fn failures_timeout_and_late_results_never_claim_an_update() {
        let ctx = eframe::egui::Context::default();
        let (tx, rx) = mpsc::channel();
        let mut state = State {
            receiver: Some(rx),
            started: Some(Instant::now() - CHECK_TIMEOUT - Duration::from_secs(1)),
            ..State::default()
        };
        state.poll(&ctx);
        assert!(matches!(state.result, Some(Err(_))));
        assert!(
            state.checking(),
            "don't overlap a native call that hasn't returned"
        );
        tx.send(Ok(Outcome::Current)).unwrap();
        state.poll(&ctx);
        assert!(
            matches!(state.result, Some(Err(_))),
            "ignore the late result"
        );
        assert!(!state.checking());
        assert!(state.notification(&Preferences::default()).is_none());
        let (tx, rx) = mpsc::channel();
        state.receiver = Some(rx);
        drop(tx);
        state.poll(&ctx);
        assert!(!state.checking());
        assert!(matches!(state.result, Some(Err(_))));
    }
}
