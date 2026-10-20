//! Project-first storage. A project owns its original pixels; raster files are exports.
use crate::{
    document::{self, Document},
    library::{self, EntryKind},
    storage::{self, Settings},
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn is_project(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("myimage"))
}

pub fn source_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

pub fn image_document(path: &Path) -> Result<Document, String> {
    Ok(Document {
        image: Arc::new(
            image::ImageReader::open(path)
                .map_err(|e| e.to_string())?
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .decode()
                .map_err(|e| e.to_string())?
                .into_rgba8(),
        ),
        annotations: Vec::new(),
        geometry: Default::default(),
    })
}

pub fn capture(
    image: Arc<image::RgbaImage>,
    root: &Path,
    at: chrono::DateTime<chrono::Local>,
) -> Result<PathBuf, String> {
    document::save_new_project(
        &Document {
            image,
            annotations: vec![],
            geometry: Default::default(),
        },
        &root.join(at.format("%Y/%m/%d").to_string()),
        &at.format("%H%M%S-%f").to_string(),
    )
}

fn fingerprint(image: &image::RgbaImage) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    image.dimensions().hash(&mut hash);
    image.as_raw().hash(&mut hash);
    hash.finish()
}

#[derive(Default)]
pub struct Migration {
    pub links: BTreeMap<PathBuf, PathBuf>,
    pub warnings: Vec<String>,
}

/// Read-only originals, verified atomic projects. Matching never relies on a name alone.
/// Run once for the old library, off the UI thread. Exports made later aren't imported.
pub fn migrate(settings: &Settings) -> Result<Migration, String> {
    let index = library::scan(&settings.output_dir)?;
    let mut result = Migration {
        links: settings.project_sources.clone(),
        warnings: index.warnings,
    };
    let mut projects: Vec<_> = index
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Project)
        .map(|e| e.path.clone())
        .collect();
    let mut images: Vec<_> = index
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Image)
        .map(|e| e.path.clone())
        .collect();
    for path in &settings.recent_files {
        if path.is_file() {
            let list = if is_project(path) {
                &mut projects
            } else {
                &mut images
            };
            if !list.contains(path) {
                list.push(path.clone());
            }
        }
    }
    let mut originals: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for path in projects {
        if let Ok(doc) = document::load_project(&path) {
            originals
                .entry(fingerprint(&doc.image))
                .or_default()
                .push(path);
        }
    }
    for path in images {
        let key = source_key(&path);
        // A deliberately deleted project is never recreated during migration retry.
        if result.links.contains_key(&key) {
            continue;
        }
        let imported = (|| {
            let doc = image_document(&path)?;
            let hash = fingerprint(&doc.image);
            let matching = originals
                .get(&hash)
                .and_then(|paths| {
                    paths.iter().find(|p| {
                        document::load_project(p).is_ok_and(|saved| saved.image == doc.image)
                    })
                })
                .cloned();
            let project = if let Some(path) = matching {
                path
            } else {
                let folder = if path.starts_with(&settings.output_dir) {
                    path.parent().unwrap().to_path_buf()
                } else {
                    storage::projects_folder(settings)
                };
                let saved = document::save_new_project(
                    &doc,
                    &folder,
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Imported image"),
                )?;
                let verified = document::load_project(&saved)?;
                if verified.image != doc.image || verified.edit_state() != doc.edit_state() {
                    return Err("Project verification failed; original retained".to_string());
                }
                originals.entry(hash).or_default().push(saved.clone());
                saved
            };
            Ok(project)
        })();
        match imported {
            Ok(project) => {
                result.links.insert(key, project);
            }
            Err(error) => result.warnings.push(format!("{}: {error}", path.display())),
        }
    }
    Ok(result)
}

pub fn remap_settings(settings: &mut Settings, links: &BTreeMap<PathBuf, PathBuf>) {
    let remap = |path: &PathBuf| {
        links
            .get(&source_key(path))
            .cloned()
            .unwrap_or_else(|| path.clone())
    };
    let mut seen = std::collections::HashSet::new();
    settings.recent_files = settings
        .recent_files
        .iter()
        .map(remap)
        .filter(|p| is_project(p) && seen.insert(p.clone()))
        .collect();
    settings.library.favorites = settings.library.favorites.iter().map(remap).collect();
    settings.library.hidden_recents = settings.library.hidden_recents.iter().map(remap).collect();
    let root = source_key(&settings.output_dir);
    settings
        .managed_image_sources
        .extend(links.keys().filter(|p| p.starts_with(&root)).cloned());
    settings.project_sources = links.clone();
}

/// Only files inside MyImage's own storage are owned companions. External imports
/// remain the user's originals. Recheck pixels before retiring a legacy capture.
pub fn owned_companions(settings: &Settings, projects: &[PathBuf]) -> Vec<PathBuf> {
    settings
        .project_sources
        .iter()
        .filter(|(source, project)| {
            settings.managed_image_sources.contains(*source)
                && projects.contains(project)
                && source.is_file()
                && !is_project(source)
        })
        .map(|(source, _)| source.clone())
        .collect()
}

pub fn retire_verified_captures(settings: &Settings) -> Vec<String> {
    let root = source_key(&settings.output_dir);
    let mut warnings = Vec::new();
    for (source, project) in &settings.project_sources {
        let Ok(relative) = source.strip_prefix(&root) else {
            continue;
        };
        let components: Vec<_> = relative.components().collect();
        // Old captures have the app's YYYY/MM/DD/name.png layout. Exports and
        // imported originals in other folders are not automatically retired.
        if components.len() != 4 || !source.is_file() {
            continue;
        }
        let date = format!(
            "{}/{}/{}",
            components[0].as_os_str().to_string_lossy(),
            components[1].as_os_str().to_string_lossy(),
            components[2].as_os_str().to_string_lossy()
        );
        if chrono::NaiveDate::parse_from_str(&date, "%Y/%m/%d").is_err() {
            continue;
        }
        let verified = image_document(source).and_then(|original| {
            document::load_project(project).map(|saved| saved.image == original.image)
        });
        if verified == Ok(true) {
            warnings.extend(
                crate::library_files::run(crate::library_files::Operation::Trash(vec![
                    source.clone(),
                ]))
                .errors,
            );
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixels(value: u8) -> image::RgbaImage {
        image::RgbaImage::from_pixel(19, 13, image::Rgba([value, 17, 42, 120]))
    }
    fn fixture(root: &Path, name: &str, value: u8) -> PathBuf {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        pixels(value).save(&path).unwrap();
        path
    }
    #[test]
    fn migration_reuses_edited_projects_verifies_pixels_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        let original = fixture(&root, "2026/10/21/capture.png", 80);
        let external = fixture(dir.path(), "external.png", 90);
        let mut edited = image_document(&original).unwrap();
        edited.geometry.flip_horizontal = true;
        let project =
            document::save_new_project(&edited, &root.join("Projects"), "Renamed project").unwrap();
        let collision =
            document::save_new_project(&image_document(&external).unwrap(), &root, "capture")
                .unwrap();
        let mut settings = Settings {
            output_dir: root.clone(),
            recent_files: vec![original.clone(), project.clone(), external.clone()],
            ..Default::default()
        };
        settings.library.favorites.insert(original.clone());
        let migration = migrate(&settings).unwrap();
        assert!(migration.warnings.is_empty());
        assert_eq!(migration.links[&source_key(&original)], project);
        assert_eq!(migration.links[&source_key(&external)], collision);
        remap_settings(&mut settings, &migration.links);
        assert_eq!(
            settings.recent_files,
            vec![project.clone(), collision.clone()]
        );
        assert!(settings.library.favorites.contains(&project));
        assert_eq!(
            owned_companions(&settings, &[project.clone(), collision]),
            vec![source_key(&original)]
        );
        assert_eq!(
            document::load_project(&project).unwrap().edit_state(),
            edited.edit_state()
        );
        assert!(original.is_file() && external.is_file());
        let before = library::scan(&root).unwrap().entries.len();
        assert_eq!(migrate(&settings).unwrap().links, migration.links);
        assert_eq!(library::scan(&root).unwrap().entries.len(), before);
        // Deleting a project doesn't cause migration retry to resurrect it.
        fs::remove_file(&project).unwrap();
        assert_eq!(
            migrate(&settings).unwrap().links[&source_key(&original)],
            project
        );
        assert!(!project.exists());
    }
    #[test]
    fn conversion_failure_retains_original_and_new_projects_never_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let original = fixture(dir.path(), "same.png", 15);
        let doc = image_document(&original).unwrap();
        let one = document::save_new_project(&doc, dir.path(), "same").unwrap();
        let two = document::save_new_project(&doc, dir.path(), "same").unwrap();
        assert_ne!(one, two);
        assert_eq!(document::load_project(&one).unwrap().image, doc.image);
        let bad = dir.path().join("broken.png");
        fs::write(&bad, b"corrupt").unwrap();
        let result = migrate(&Settings {
            output_dir: dir.path().to_owned(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert!(!result.links.contains_key(&bad));
        assert_eq!(fs::read(bad).unwrap(), b"corrupt");
        assert!(original.exists());
    }
}
