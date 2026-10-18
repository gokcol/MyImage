//! Persistent annotation snapshots, separate from image projects and the clipboard.
use crate::document::{self, Annotation};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
};

const VERSION: u32 = 1;
const MAX_LIBRARY_BYTES: usize = 16 * 1024 * 1024;
const MAX_PRESETS: usize = 128;
const MAX_PRESET_BYTES: usize = 4 * 1024 * 1024 - 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: u64,
    pub name: String,
    pub annotations: Vec<Annotation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    version: u32,
    pub entries: Vec<Preset>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: VERSION,
            entries: Vec::new(),
        }
    }
}

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty()
        || name.chars().count() > 64
        || name.len() > 256
        || name.chars().any(char::is_control)
    {
        return Err("Use a preset name of 1–64 characters on one line.".into());
    }
    Ok(())
}

fn validate(library: &Library) -> Result<(), String> {
    if library.version != VERSION {
        return Err(format!(
            "Preset library version {} is not supported.",
            library.version
        ));
    }
    if library.entries.len() > MAX_PRESETS {
        return Err("The preset library can hold up to 128 presets.".into());
    }
    let mut ids = BTreeSet::new();
    for preset in &library.entries {
        if preset.id == 0 || !ids.insert(preset.id) {
            return Err("The preset library contains invalid or duplicate identities.".into());
        }
        validate_name(&preset.name)?;
        if preset.annotations.is_empty() {
            return Err("A preset must contain at least one annotation.".into());
        }
        document::validate_annotations(&preset.annotations)?;
        if serde_json::to_vec(&preset.annotations)
            .map_or(true, |bytes| bytes.len() > MAX_PRESET_BYTES)
        {
            return Err("This preset exceeds the annotation data limit.".into());
        }
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Library, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library::default()),
        Err(e) => return Err(format!("Cannot open the preset library: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_LIBRARY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_LIBRARY_BYTES {
        return Err("The preset library exceeds the 16 MiB limit.".into());
    }
    let library: Library = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Cannot read the preset library: {e}"))?;
    validate(&library)?;
    Ok(library)
}

fn save(path: &Path, library: &Library) -> Result<(), String> {
    validate(library)?;
    let bytes = serde_json::to_vec(library).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_LIBRARY_BYTES {
        return Err(
            "The preset library is full (16 MiB). Remove unused presets before adding more.".into(),
        );
    }
    let parent = path.parent().ok_or("Invalid preset library location.")?;
    fs::create_dir_all(parent).map_err(|e| format!("Cannot create the preset folder: {e}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .persist(path)
        .map_err(|e| format!("Cannot save the preset library: {e}"))?;
    Ok(())
}

pub enum Change {
    Add {
        name: String,
        annotations: Vec<Annotation>,
    },
    Rename {
        id: u64,
        name: String,
    },
    Delete {
        id: u64,
    },
}

/// Read again before each write: preserve other additions and never silently
/// replace an unreadable or newer library with an empty catalog.
pub fn change(path: &Path, change: Change) -> Result<(Library, Option<u64>), String> {
    let mut library = load(path)?;
    let mut added = None;
    match change {
        Change::Add { name, annotations } => {
            let used: BTreeSet<_> = library.entries.iter().map(|p| p.id).collect();
            let id = (1..)
                .find(|id| !used.contains(id))
                .ok_or("No preset identity available.")?;
            library.entries.push(Preset {
                id,
                name: name.trim().into(),
                annotations,
            });
            added = Some(id);
        }
        Change::Rename { id, name } => {
            let entry = library
                .entries
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or("This preset no longer exists. Refresh the library.")?;
            entry.name = name.trim().into();
        }
        Change::Delete { id } => {
            let at = library
                .entries
                .iter()
                .position(|p| p.id == id)
                .ok_or("This preset no longer exists. Refresh the library.")?;
            library.entries.remove(at);
        }
    }
    save(path, &library)?;
    Ok((library, added))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{document::AnnotationKind, markup_tests::object};

    pub(crate) fn artwork() -> Vec<Annotation> {
        let kinds = [
            AnnotationKind::Text,
            AnnotationKind::Arrow,
            AnnotationKind::Rectangle,
            AnnotationKind::Ellipse,
            AnnotationKind::Step,
            AnnotationKind::Callout,
            AnnotationKind::Highlighter,
            AnnotationKind::Freehand,
            AnnotationKind::Blur,
            AnnotationKind::Pixelate,
        ];
        kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| {
                let mut a = object(
                    kind,
                    [30. + i as f32 * 30., 40.],
                    [60. + i as f32 * 30., 90.],
                );
                a.layer.name = format!("Artwork {i}");
                if matches!(kind, AnnotationKind::Text | AnnotationKind::Callout) {
                    a.text = "Reusable ✓".into();
                }
                if kind == AnnotationKind::Freehand {
                    a.points = vec![a.start, a.end];
                }
                if i < 3 {
                    a.group_id = Some(1);
                    a.group_layer.name = "Review kit".into();
                }
                a.layer.hidden = i == 7;
                a.layer.locked = i == 4;
                a
            })
            .collect()
    }
    fn add(name: &str) -> Change {
        Change::Add {
            name: name.into(),
            annotations: artwork(),
        }
    }

    #[test]
    fn catalog_roundtrip_preserves_every_kind_style_and_group_and_reloads_before_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/presets.json");
        assert!(load(&path).unwrap().entries.is_empty());
        let (first, id) = change(&path, add("  Review kit ✓  ")).unwrap();
        assert_eq!(id, Some(1));
        assert_eq!(first.entries[0].name, "Review kit ✓");
        assert_eq!(first.entries[0].annotations, artwork());
        assert_eq!(load(&path).unwrap(), first);
        change(&path, add("Second")).unwrap();
        let (renamed, _) = change(
            &path,
            Change::Rename {
                id: 1,
                name: "Renamed".into(),
            },
        )
        .unwrap();
        assert_eq!(renamed.entries.len(), 2);
        assert_eq!(renamed.entries[1].name, "Second");
        assert_eq!(renamed.entries[0].annotations, first.entries[0].annotations);
        change(&path, Change::Delete { id: 1 }).unwrap();
        assert_eq!(load(&path).unwrap().entries.len(), 1);
        assert_eq!(load(&path).unwrap().entries[0].id, 2);
    }

    #[test]
    fn unreadable_or_future_catalog_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        for bytes in [
            b"broken".as_slice(),
            b"{\"version\":99,\"entries\":[]}",
            b"{\"version\":1,\"entries\":[",
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(change(&path, add("New")).is_err());
            assert!(
                change(
                    &path,
                    Change::Rename {
                        id: 1,
                        name: "New".into()
                    }
                )
                .is_err()
            );
            assert!(change(&path, Change::Delete { id: 1 }).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn invalid_changes_leave_existing_catalog_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        change(&path, add("Original")).unwrap();
        let original = fs::read(&path).unwrap();
        let mut invalid = artwork();
        invalid[0].group_layer.name = "Inconsistent group".into();
        for change_request in [
            Change::Add {
                name: "Invalid".into(),
                annotations: invalid,
            },
            Change::Add {
                name: "Empty".into(),
                annotations: vec![],
            },
            add(" "),
            add(&"a".repeat(65)),
            add("Two\nlines"),
            Change::Rename {
                id: 1,
                name: String::new(),
            },
            Change::Delete { id: 999 },
        ] {
            assert!(change(&path, change_request).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }

    #[test]
    fn bounded_catalog_rejects_too_many_entries_duplicate_ids_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut library = Library::default();
        for id in 1..=MAX_PRESETS as u64 {
            library.entries.push(Preset {
                id,
                name: format!("Preset {id}"),
                annotations: artwork(),
            });
        }
        save(&path, &library).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(change(&path, add("Overflow")).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        library.entries[1].id = 1;
        assert!(save(&path, &library).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_LIBRARY_BYTES as u64 + 1).unwrap();
        assert!(load(&path).unwrap_err().contains("16 MiB"));
        assert!(change(&path, add("Oversize")).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            MAX_LIBRARY_BYTES as u64 + 1
        );
    }

    #[test]
    fn failed_destination_returns_error_without_creating_a_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("not-a-folder");
        fs::write(&parent, b"keep").unwrap();
        assert!(change(&parent.join("presets.json"), add("Test")).is_err());
        assert_eq!(fs::read(parent).unwrap(), b"keep");
    }
}
