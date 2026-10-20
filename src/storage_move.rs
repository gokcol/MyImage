//! Copy/verify, commit preferences, then retire originals. Interruption never
//! leaves the saved library pointing at files that have already been removed.
use crate::{library, storage::Settings};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub struct PreparedMove {
    pub destination: PathBuf,
    pub paths: BTreeMap<PathBuf, PathBuf>,
    canonical_paths: BTreeMap<PathBuf, PathBuf>,
    source_root: PathBuf,
    source_base: PathBuf,
    destination_root: PathBuf,
}

impl PreparedMove {
    pub fn settings(&self, current: &Settings) -> Settings {
        let mut next = current.clone();
        next.output_dir = self.destination.clone();
        for path in &mut next.recent_files {
            self.remap(path);
        }
        next.library.favorites = next
            .library
            .favorites
            .into_iter()
            .map(|mut p| {
                self.remap(&mut p);
                p
            })
            .collect();
        next.library.hidden_recents = next
            .library
            .hidden_recents
            .into_iter()
            .map(|mut p| {
                self.remap(&mut p);
                p
            })
            .collect();
        next.managed_image_sources = next
            .managed_image_sources
            .into_iter()
            .map(|mut path| {
                self.remap(&mut path);
                path
            })
            .collect();
        next.project_sources = next
            .project_sources
            .into_iter()
            .map(|(mut source, mut project)| {
                self.remap(&mut source);
                self.remap(&mut project);
                (source, project)
            })
            .collect();
        next
    }
    pub fn remap(&self, path: &mut PathBuf) {
        if let Some(new) = self.paths.get(path).or_else(|| {
            fs::canonicalize(&*path)
                .ok()
                .and_then(|canonical| self.canonical_paths.get(&canonical))
        }) {
            *path = new.clone();
        }
    }
}

fn regular(path: &Path) -> Result<fs::Metadata, String> {
    let m = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if m.is_file() && !is_link(&m) {
        Ok(m)
    } else {
        Err(format!("Not a regular file: {}", path.display()))
    }
}
fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_type().is_symlink() || m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        m.file_type().is_symlink()
    }
}
fn directory(path: &Path) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if m.is_dir() && !is_link(&m) {
        Ok(())
    } else {
        Err(format!(
            "Choose a normal folder, not a link: {}",
            path.display()
        ))
    }
}
fn safe_parent(root: &Path, relative: &Path, create: bool) -> Result<PathBuf, String> {
    directory(root)?;
    let mut parent = root.to_owned();
    for part in relative.parent().unwrap_or(Path::new("")).components() {
        let Component::Normal(name) = part else {
            return Err("Invalid relative path".into());
        };
        parent.push(name);
        if create && !parent.exists() {
            fs::create_dir(&parent).map_err(|e| e.to_string())?;
        }
        directory(&parent)?;
    }
    Ok(parent)
}
fn equal(a: &mut File, b: &mut File) -> Result<bool, String> {
    a.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    b.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let size = a.metadata().map_err(|e| e.to_string())?.len();
    if size != b.metadata().map_err(|e| e.to_string())?.len() {
        return Ok(false);
    }
    let mut remaining = size;
    let mut left = [0; 65536];
    let mut right = [0; 65536];
    while remaining > 0 {
        let n = remaining.min(left.len() as u64) as usize;
        a.read_exact(&mut left[..n]).map_err(|e| e.to_string())?;
        b.read_exact(&mut right[..n]).map_err(|e| e.to_string())?;
        if left[..n] != right[..n] {
            return Ok(false);
        }
        remaining -= n as u64;
    }
    Ok(a.metadata().map_err(|e| e.to_string())?.len() == size)
}

pub fn prepare(
    source: &Path,
    destination: &Path,
    move_existing: bool,
    mut progress: impl FnMut(usize, usize),
) -> Result<PreparedMove, String> {
    directory(destination)?;
    let destination_root = fs::canonicalize(destination).map_err(|e| e.to_string())?;
    let source_root = if source.exists() {
        directory(source)?;
        fs::canonicalize(source).map_err(|e| e.to_string())?
    } else {
        source.to_owned()
    };
    if move_existing
        && (source_root.starts_with(&destination_root)
            || destination_root.starts_with(&source_root))
    {
        return Err("Choose a separate destination, outside the current storage folder (and not its parent).".into());
    }
    // Fail before changing preferences if the selected destination is unwritable.
    tempfile::NamedTempFile::new_in(destination)
        .map_err(|e| format!("Cannot write to {}: {e}", destination.display()))?;
    let mut prepared = PreparedMove {
        destination: destination.to_owned(),
        paths: BTreeMap::new(),
        canonical_paths: BTreeMap::new(),
        source_base: source.to_owned(),
        source_root,
        destination_root,
    };
    if !move_existing {
        return Ok(prepared);
    }
    let index = library::scan(source)?;
    if !index.warnings.is_empty() {
        return Err(format!(
            "Cannot move an incomplete library scan: {}",
            index.warnings.join("\n")
        ));
    }
    let total = index.entries.len();
    for (i, entry) in index.entries.into_iter().enumerate() {
        let copied = (|| {
            let relative = entry.path.strip_prefix(source).map_err(|e| e.to_string())?;
            safe_parent(&prepared.source_root, relative, false)?;
            let before = regular(&entry.path)?;
            let parent = safe_parent(&prepared.destination_root, relative, true)?;
            let mut input = File::open(&entry.path).map_err(|e| e.to_string())?;
            let mut temporary =
                tempfile::NamedTempFile::new_in(&parent).map_err(|e| e.to_string())?;
            std::io::copy(&mut input, temporary.as_file_mut()).map_err(|e| e.to_string())?;
            if !equal(&mut input, temporary.as_file_mut())?
                || before.modified().ok() != regular(&entry.path)?.modified().ok()
            {
                return Err(format!(
                    "File changed during copying: {}",
                    entry.path.display()
                ));
            }
            if let Ok(modified) = before.modified() {
                temporary
                    .as_file()
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .map_err(|e| e.to_string())?;
            }
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            for suffix in 0..10000 {
                let name = if suffix == 0 {
                    relative.file_name().unwrap().to_os_string()
                } else {
                    let mut name = relative.file_stem().unwrap().to_os_string();
                    name.push(format!(" (moved {suffix})"));
                    if let Some(ext) = relative.extension() {
                        name.push(".");
                        name.push(ext);
                    }
                    name
                };
                let target = parent.join(name);
                match temporary.persist_noclobber(&target) {
                    Ok(_) => {
                        let mapped = destination.join(
                            target
                                .strip_prefix(&prepared.destination_root)
                                .map_err(|e| e.to_string())?,
                        );
                        return Ok(mapped);
                    }
                    Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                        temporary = e.file;
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
            Err("Too many conflicting filenames in destination".into())
        })();
        match copied {
            Ok(target) => {
                if let Ok(relative) = entry.path.strip_prefix(source) {
                    prepared
                        .canonical_paths
                        .insert(prepared.source_root.join(relative), target.clone());
                }
                prepared.paths.insert(entry.path, target);
            }
            Err(error) => {
                return Err(format!(
                    "{error}\nStorage has not switched; all original files remain. Any completed copies are in {}.",
                    destination.display()
                ));
            }
        }
        if i % 10 == 0 || i + 1 == total {
            progress(i + 1, total);
        }
    }
    Ok(prepared)
}

/// Call only AFTER successfully persisting the new settings and file references.
/// If a file changes externally, keep its original rather than deleting it.
pub fn retire_originals(prepared: PreparedMove) -> Vec<String> {
    let mut warnings = Vec::new();
    for (source, destination) in &prepared.paths {
        let result = (|| {
            let relative = source
                .strip_prefix(&prepared.source_base)
                .map_err(|e| e.to_string())?;
            safe_parent(&prepared.source_root, relative, false)?;
            regular(source)?;
            regular(destination)?;
            if fs::canonicalize(source).map_err(|e| e.to_string())?
                != prepared.source_root.join(relative)
            {
                return Err("Source path changed; original retained".to_string());
            }
            let relative = destination
                .strip_prefix(&prepared.destination)
                .map_err(|e| e.to_string())?;
            safe_parent(&prepared.destination_root, relative, false)?;
            if fs::canonicalize(destination).map_err(|e| e.to_string())?
                != prepared.destination_root.join(relative)
            {
                return Err("Destination path changed; original retained".to_string());
            }
            let mut a = File::open(source).map_err(|e| e.to_string())?;
            let mut b = File::open(destination).map_err(|e| e.to_string())?;
            if !equal(&mut a, &mut b)? {
                return Err("Source or destination changed; original retained".to_string());
            }
            drop(a);
            drop(b);
            fs::remove_file(source).map_err(|e| e.to_string())
        })();
        if let Err(error) = result {
            warnings.push(format!("{}: {error}", source.display()));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }
    #[test]
    fn verified_move_keeps_tree_bytes_dates_recents_favorites_and_projects() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        fs::create_dir(&new).unwrap();
        let png = fixture(&old, "2026/10/18/capture.png", b"capture bytes");
        let project = old.join("Projects/editable.myimage");
        fs::create_dir_all(project.parent().unwrap()).unwrap();
        let doc = crate::document::Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                4,
                5,
                image::Rgba([5, 9, 20, 255]),
            )),
            annotations: vec![],
            geometry: Default::default(),
        };
        crate::document::save_project(&doc, &project).unwrap();
        let external = fixture(temp.path(), "outside.png", b"outside");
        let unrelated = fixture(&old, "notes.txt", b"keep");
        let before = fs::read(&project).unwrap();
        let modified = fs::metadata(&png).unwrap().modified().unwrap();
        let mut settings = Settings {
            output_dir: old.clone(),
            recent_files: vec![project.clone(), png.clone(), external.clone()],
            ..Settings::default()
        };
        settings.library.favorites.insert(png.clone());
        settings.library.hidden_recents.insert(project.clone());
        let mut progress = vec![];
        let prepared =
            prepare(&old, &new, true, |done, total| progress.push((done, total))).unwrap();
        assert_eq!(progress.last(), Some(&(2, 2)));
        // Preparing (or quitting/failing to save settings now) never removes sources.
        assert!(png.exists() && project.exists());
        let next = prepared.settings(&settings);
        let next: Settings = serde_json::from_slice(&serde_json::to_vec(&next).unwrap()).unwrap();
        assert_eq!(
            next.recent_files,
            vec![
                new.join("Projects/editable.myimage"),
                new.join("2026/10/18/capture.png"),
                external.clone()
            ]
        );
        assert!(next.library.favorites.contains(&next.recent_files[1]));
        assert!(next.library.hidden_recents.contains(&next.recent_files[0]));
        assert_eq!(
            fs::metadata(&next.recent_files[1])
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        assert!(retire_originals(prepared).is_empty());
        assert!(!png.exists() && !project.exists());
        assert!(external.exists() && unrelated.exists());
        assert_eq!(fs::read(&next.recent_files[0]).unwrap(), before);
        assert_eq!(
            crate::document::load_project(&next.recent_files[0])
                .unwrap()
                .image,
            doc.image
        );
        assert_eq!(library::scan(&new).unwrap().entries.len(), 2);
    }
    #[test]
    fn conflicts_get_unique_names_and_current_references_follow_actual_targets() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        let p = fixture(&old, "Projects/file.myimage", b"source");
        let collision = fixture(&new, "Projects/file.myimage", b"existing");
        fixture(&new, "Projects/file (moved 1).myimage", b"existing2");
        let prepared = prepare(&old, &new, true, |_, _| {}).unwrap();
        let mut active = p.clone();
        prepared.remap(&mut active);
        assert_eq!(active, new.join("Projects/file (moved 2).myimage"));
        assert_eq!(fs::read(&active).unwrap(), b"source");
        assert!(retire_originals(prepared).is_empty());
        assert_eq!(fs::read(collision).unwrap(), b"existing");
    }
    #[test]
    fn future_files_only_does_not_move_or_remap_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        fs::create_dir(&new).unwrap();
        let p = fixture(&old, "a.png", b"image");
        let settings = Settings {
            output_dir: old.clone(),
            recent_files: vec![p.clone()],
            ..Settings::default()
        };
        let prepared = prepare(&old, &new, false, |_, _| {}).unwrap();
        let next = prepared.settings(&settings);
        assert_eq!(next.output_dir, new);
        assert_eq!(next.recent_files, settings.recent_files);
        assert!(retire_originals(prepared).is_empty());
        assert!(p.exists());
        assert!(!new.join("a.png").exists());
    }
    #[test]
    fn rejects_overlapping_roots_and_retains_originals_on_copy_failure() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        fs::create_dir(&new).unwrap();
        let p = fixture(&old, "a.png", b"a");
        fixture(&old, "second/b.png", b"b");
        let nested = old.join("nested");
        fs::create_dir(&nested).unwrap();
        for dest in [&old, &nested, temp.path()] {
            assert!(prepare(&old, dest, true, |_, _| {}).is_err());
        }
        // A file blocks creation of a directory in the destination.
        fixture(&new, "second", b"existing blocker");
        assert!(prepare(&old, &new, true, |_, _| {}).is_err());
        assert!(p.exists());
        assert!(old.join("second/b.png").exists());
        assert_eq!(fs::read(new.join("second")).unwrap(), b"existing blocker");
    }
    #[test]
    fn modified_sources_or_destinations_are_never_retired() {
        for change_source in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let old = temp.path().join("old");
            let new = temp.path().join("new");
            fs::create_dir(&new).unwrap();
            let source = fixture(&old, "a.png", b"original");
            let prepared = prepare(&old, &new, true, |_, _| {}).unwrap();
            fs::write(
                if change_source {
                    source.clone()
                } else {
                    new.join("a.png")
                },
                b"changed",
            )
            .unwrap();
            assert_eq!(retire_originals(prepared).len(), 1);
            assert!(source.exists());
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_targets_cannot_redirect_moves_outside_destination() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        let external = temp.path().join("external");
        fs::create_dir(&new).unwrap();
        fs::create_dir(&external).unwrap();
        let p = fixture(&old, "Projects/a.myimage", b"project");
        symlink(&external, new.join("Projects")).unwrap();
        assert!(prepare(&old, &new, true, |_, _| {}).is_err());
        assert!(p.exists());
        assert!(!external.join("a.myimage").exists());
        fs::remove_file(new.join("Projects")).unwrap();
        let prepared = prepare(&old, &new, true, |_, _| {}).unwrap();
        fs::rename(old.join("Projects"), old.join("original-projects")).unwrap();
        symlink(old.join("original-projects"), old.join("Projects")).unwrap();
        assert_eq!(retire_originals(prepared).len(), 1);
        assert!(old.join("original-projects/a.myimage").exists());
    }
    #[test]
    fn failed_second_copy_preserves_first_original_and_verified_copy() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        fs::create_dir(&new).unwrap();
        fixture(&old, "a/first.png", b"first");
        fixture(&old, "b/second.png", b"second");
        let order = library::scan(&old).unwrap().entries;
        let first = order[0].path.strip_prefix(&old).unwrap();
        let blocker = new.join(order[1].path.strip_prefix(&old).unwrap().parent().unwrap());
        assert!(
            prepare(&old, &new, true, |done, _| {
                if done == 1 {
                    fs::write(&blocker, b"blocked").unwrap();
                }
            })
            .is_err()
        );
        for entry in &order {
            assert!(entry.path.exists());
        }
        assert_eq!(
            fs::read(new.join(first)).unwrap(),
            fs::read(old.join(first)).unwrap()
        );
    }
    #[cfg(unix)]
    #[test]
    fn references_using_parent_aliases_follow_moved_files() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        fs::create_dir(&new).unwrap();
        fixture(&old, "a.png", b"image");
        let alias = temp.path().join("alias");
        symlink(&old, &alias).unwrap();
        let prepared = prepare(&old, &new, true, |_, _| {}).unwrap();
        let mut reference = alias.join("a.png");
        prepared.remap(&mut reference);
        assert_eq!(reference, new.join("a.png"));
    }
    #[test]
    fn first_run_without_an_old_folder_can_switch_safely() {
        let temp = tempfile::tempdir().unwrap();
        let new = temp.path().join("new");
        fs::create_dir(&new).unwrap();
        let prepared = prepare(&temp.path().join("missing"), &new, true, |_, _| {}).unwrap();
        assert!(prepared.paths.is_empty());
        assert!(retire_originals(prepared).is_empty());
    }
}
