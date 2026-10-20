use chrono::{DateTime, Local};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

/// Scale the complete UI so explicitly sized labels and their hit targets stay aligned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTextSize {
    Small,
    #[default]
    Normal,
    Large,
    VeryLarge,
}

impl UiTextSize {
    pub const ALL: [Self; 4] = [Self::Small, Self::Normal, Self::Large, Self::VeryLarge];

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Normal => "Normal",
            Self::Large => "Large",
            Self::VeryLarge => "Very Large",
        }
    }

    pub fn factor(self) -> f32 {
        match self {
            Self::Small => 0.9,
            Self::Normal => 1.0,
            Self::Large => 1.15,
            Self::VeryLarge => 1.3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub guide_version: u8,
    pub ui_text_size: UiTextSize,
    pub gui_finish: crate::appearance::GuiFinish,
    pub icon_style: crate::icon_style::IconStyle,
    pub pointer_style: crate::pointer_style::PointerStyle,
    pub output_dir: PathBuf,
    pub display_id: Option<u32>,
    pub shape: String,
    pub corner_radius: u32,
    pub delay_seconds: u32,
    pub capture_mode: crate::capture::CaptureMode,
    pub capture_destination: crate::capture::CaptureDestination,
    pub include_pointer: bool,
    pub capture_on_release: bool,
    pub recent_files: Vec<PathBuf>,
    pub recents_open: bool,
    pub library: crate::library::BrowserPreferences,
    pub editor_toolbar: Vec<String>,
    #[serde(default)]
    pub toolbar_layout_version: u8,
}

impl Default for Settings {
    fn default() -> Self {
        let base = UserDirs::new()
            .map(|dirs| dirs.picture_dir().unwrap_or(dirs.home_dir()).to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            guide_version: 0,
            ui_text_size: UiTextSize::Normal,
            gui_finish: Default::default(),
            icon_style: Default::default(),
            pointer_style: Default::default(),
            output_dir: base.join("MyImage"),
            display_id: None,
            shape: "rectangle".into(),
            corner_radius: 24,
            delay_seconds: 0,
            capture_mode: Default::default(),
            capture_destination: Default::default(),
            include_pointer: false,
            capture_on_release: false,
            recent_files: Vec::new(),
            recents_open: true,
            library: Default::default(),
            editor_toolbar: crate::editor::default_toolbar(),
            toolbar_layout_version: 1,
        }
    }
}

/// Editable documents live in the ordinary browsable library tree.
pub fn projects_folder(settings: &Settings) -> PathBuf {
    settings.output_dir.join("Projects")
}

pub fn project_save_directory(settings: &Settings, existing_project: Option<&Path>) -> PathBuf {
    existing_project
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| projects_folder(settings))
}

pub fn remember_recent(settings: &mut Settings, path: PathBuf) {
    settings.library.hidden_recents.remove(&path);
    settings.recent_files.retain(|p| p != &path);
    settings.recent_files.insert(0, path);
    settings.recent_files.truncate(24);
}

/// Opening an existing thumbnail preserves the complete visible strip, including
/// library entries not yet in persisted history. Only a new file joins the front.
pub fn remember_opened_recent(settings: &mut Settings, path: PathBuf, visible: &[PathBuf]) {
    let mut ordered = Vec::new();
    if !visible.contains(&path) {
        ordered.push(path.clone());
    }
    ordered.extend_from_slice(visible);
    ordered.extend_from_slice(&settings.recent_files);
    let mut seen = std::collections::HashSet::new();
    ordered.retain(|p| seen.insert(p.clone()));
    ordered.truncate(24);
    settings.library.hidden_recents.remove(&path);
    settings.recent_files = ordered;
}

fn settings_path() -> Result<PathBuf, String> {
    ProjectDirs::from("com", "MyImage", "MyImage")
        .map(|dirs| dirs.config_dir().join("settings.json"))
        .ok_or_else(|| "Cannot locate the settings folder.".to_string())
}

pub fn preset_library_path(demo: bool) -> Result<PathBuf, String> {
    if demo {
        return Ok(std::env::temp_dir()
            .join("MyImage-demo")
            .join("annotation-presets.json"));
    }
    ProjectDirs::from("com", "MyImage", "MyImage")
        .map(|dirs| dirs.data_local_dir().join("annotation-presets.json"))
        .ok_or_else(|| "Cannot locate the annotation preset folder.".into())
}

pub fn load_settings() -> Result<Settings, String> {
    let path = settings_path()?;
    match fs::read(&path) {
        Ok(data) => serde_json::from_slice(&data)
            .map_err(|e| format!("Cannot read {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    let parent = path.parent().ok_or("Invalid settings path.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temporary, settings).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Publishes a complete PNG atomically; existing captures are never overwritten.
pub fn save_capture(
    image: &image::RgbaImage,
    root: &Path,
    captured_at: DateTime<Local>,
) -> Result<PathBuf, String> {
    let directory = root.join(captured_at.format("%Y/%m/%d").to_string());
    fs::create_dir_all(&directory)
        .map_err(|e| format!("Cannot create {}: {e}", directory.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        image
            .write_to(&mut writer, image::ImageFormat::Png)
            .map_err(|e| format!("PNG encoding failed: {e}"))?;
        writer.flush().map_err(|e| e.to_string())?;
    }
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    let stem = captured_at.format("%H%M%S-%f").to_string();
    for index in 0..1000 {
        let name = if index == 0 {
            format!("{stem}.png")
        } else {
            format!("{stem}-{index}.png")
        };
        let destination = directory.join(name);
        match temporary.persist_noclobber(&destination) {
            Ok(_) => return Ok(destination),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => {
                return Err(format!(
                    "Cannot save {}: {}",
                    destination.display(),
                    error.error
                ));
            }
        }
    }
    Err("Could not allocate a unique capture filename.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn fast_capture_is_opt_in_and_persists_without_changing_destination() {
        let old: Settings = serde_json::from_str(r#"{"include_pointer":true}"#).unwrap();
        assert!(!old.capture_on_release);
        let settings = Settings {
            capture_on_release: true,
            capture_destination: crate::capture::CaptureDestination::Clipboard,
            ..old
        };
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert!(restored.capture_on_release);
        assert!(restored.include_pointer);
        assert_eq!(
            restored.capture_destination,
            crate::capture::CaptureDestination::Clipboard
        );
    }

    #[test]
    fn project_storage_is_browsable_and_switching_roots_preserves_existing_projects() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            output_dir: root.path().join("Pictures/MyImage"),
            ..Settings::default()
        };
        let directory = project_save_directory(&settings, None);
        assert_eq!(directory, settings.output_dir.join("Projects"));
        fs::create_dir_all(&directory).unwrap();
        let project = directory.join("Editable.myimage");
        let doc = crate::document::Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                8,
                6,
                image::Rgba([40, 80, 120, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        };
        crate::document::save_project(&doc, &project).unwrap();
        let bytes = fs::read(&project).unwrap();
        assert!(
            crate::library::scan(&settings.output_dir)
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.path == project)
        );
        settings.output_dir = root.path().join("Custom library");
        let settings: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(
            project_save_directory(&settings, None),
            settings.output_dir.join("Projects")
        );
        assert_eq!(project_save_directory(&settings, Some(&project)), directory);
        assert_eq!(fs::read(project).unwrap(), bytes);
    }

    #[test]
    fn gui_finish_defaults_for_existing_settings_and_survives_restart() {
        use crate::appearance::GuiFinish;
        let legacy: Settings =
            serde_json::from_str(r#"{"include_pointer":true,"ui_text_size":"large"}"#).unwrap();
        assert_eq!(legacy.gui_finish, GuiFinish::CobaltSatin);
        assert_eq!(legacy.icon_style, crate::icon_style::IconStyle::StudioColor);
        assert_eq!(legacy.ui_text_size, UiTextSize::Large);
        for finish in GuiFinish::ALL {
            for icon_style in crate::icon_style::IconStyle::ALL {
                let mut settings = legacy.clone();
                settings.gui_finish = finish;
                settings.icon_style = icon_style;
                let restored: Settings =
                    serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
                assert_eq!(restored.gui_finish, finish);
                assert_eq!(restored.icon_style, icon_style);
                assert_eq!(restored.output_dir, legacy.output_dir);
                assert_eq!(restored.ui_text_size, legacy.ui_text_size);
                assert!(restored.include_pointer);
            }
        }
    }

    #[test]
    fn pointer_style_defaults_to_standard_and_survives_restart() {
        use crate::pointer_style::PointerStyle;
        let legacy: Settings = serde_json::from_str(r#"{"include_pointer":true}"#).unwrap();
        assert_eq!(legacy.pointer_style, PointerStyle::Standard);
        assert_eq!(legacy.toolbar_layout_version, 0);
        assert_eq!(Settings::default().toolbar_layout_version, 1);
        for pointer_style in PointerStyle::ALL {
            let settings = Settings {
                pointer_style,
                ..legacy.clone()
            };
            let restored: Settings =
                serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
            assert_eq!(restored.pointer_style, pointer_style);
            assert!(restored.include_pointer);
        }
    }

    #[test]
    fn text_size_defaults_for_old_settings_and_survives_restart() {
        let legacy: Settings = serde_json::from_str(r#"{"include_pointer":true}"#).unwrap();
        assert_eq!(legacy.ui_text_size, UiTextSize::Normal);
        assert!(legacy.include_pointer);
        for size in UiTextSize::ALL {
            let settings = Settings {
                ui_text_size: size,
                ..Settings::default()
            };
            let saved = serde_json::to_vec(&settings).unwrap();
            let restored: Settings = serde_json::from_slice(&saved).unwrap();
            assert_eq!(restored.ui_text_size, size);
            assert_eq!(restored.output_dir, settings.output_dir);
        }
    }

    #[test]
    fn png_round_trip_preserves_alpha_and_does_not_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let at = Local.with_ymd_and_hms(2026, 10, 17, 21, 22, 23).unwrap();
        let mut image = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        image.put_pixel(0, 0, image::Rgba([100, 80, 60, 0]));
        let first = save_capture(&image, directory.path(), at).unwrap();
        let second = save_capture(&image, directory.path(), at).unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with(directory.path().join("2026/10/17")));
        assert_eq!(image::open(first).unwrap().into_rgba8(), image);
        assert_eq!(
            fs::read_dir(directory.path().join("2026/10/17"))
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    fn invalid_output_folder_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("file");
        fs::write(&file, b"not a directory").unwrap();
        assert!(save_capture(&image::RgbaImage::new(2, 2), &file, Local::now()).is_err());
    }
}
