//! Undo/Redo operations stack for file manager actions.

use arca_xdg::trash::TrashItem;
use std::path::PathBuf;

/// Reversible file operation.
#[derive(Clone, Debug)]
pub enum UndoAction {
    /// File(s) moved to XDG Trash, restorable via TrashItem info.
    Trash(Vec<TrashItem>),
    /// Renamed file: original path -> new path.
    Rename {
        original: PathBuf,
        new_path: PathBuf,
    },
    /// Created folder: path to created empty directory.
    CreateFolder(PathBuf),
    /// Moved file(s): original locations -> new destinations.
    Move {
        sources: Vec<PathBuf>,
        destinations: Vec<PathBuf>,
    },
    /// Copied file(s): list of newly created copied files.
    Copy { destinations: Vec<PathBuf> },
}

/// Transactional undo/redo stack for file manager state.
#[derive(Clone, Debug, Default)]
pub struct UndoStack {
    undo_items: Vec<UndoAction>,
    redo_items: Vec<UndoAction>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, action: UndoAction) {
        self.undo_items.push(action);
        self.redo_items.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_items.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_items.is_empty()
    }

    /// Undo the most recent reversible action. Returns human-readable feedback.
    pub fn undo(&mut self) -> Result<String, String> {
        let Some(action) = self.undo_items.pop() else {
            return Err("Nothing to undo".into());
        };

        match action {
            UndoAction::Trash(items) => {
                let mut restored = 0;
                let count = items.len();
                for item in &items {
                    if arca_xdg::trash::restore_trash_item(item).is_ok() {
                        restored += 1;
                    }
                }
                self.redo_items.push(UndoAction::Trash(items));
                Ok(format!("Restored {restored} of {count} item(s) from Trash"))
            }
            UndoAction::Rename { original, new_path } => {
                let orig_name = original
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let new_name = new_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                crate::ops::rename_path(&new_path, &original)
                    .map_err(|e| format!("Cannot undo rename: {e}"))?;
                self.redo_items.push(UndoAction::Rename {
                    original: original.clone(),
                    new_path: new_path.clone(),
                });
                Ok(format!("Renamed '{new_name}' back to '{orig_name}'"))
            }
            UndoAction::CreateFolder(path) => {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if path.exists() {
                    let _ = std::fs::remove_dir(&path);
                }
                self.redo_items.push(UndoAction::CreateFolder(path));
                Ok(format!("Removed created folder '{name}'"))
            }
            UndoAction::Move {
                sources,
                destinations,
            } => {
                let mut moved = 0;
                let mut redone_dsts = Vec::new();
                for (src, dst) in sources.iter().zip(destinations.iter()) {
                    if dst.exists() {
                        if let Some(src_parent) = src.parent() {
                            if let Ok(new_dst) = crate::ops::move_into(dst, src_parent) {
                                redone_dsts.push(new_dst);
                                moved += 1;
                            }
                        }
                    }
                }
                self.redo_items.push(UndoAction::Move {
                    sources: redone_dsts,
                    destinations: destinations.clone(),
                });
                Ok(format!("Undid move of {moved} item(s)"))
            }
            UndoAction::Copy { destinations } => {
                let mut removed = 0;
                for dst in &destinations {
                    if dst.exists() {
                        let res = if dst.is_dir() {
                            std::fs::remove_dir_all(dst)
                        } else {
                            std::fs::remove_file(dst)
                        };
                        if res.is_ok() {
                            removed += 1;
                        }
                    }
                }
                self.redo_items.push(UndoAction::Copy { destinations });
                Ok(format!("Removed {removed} copied item(s)"))
            }
        }
    }

    /// Redo the most recently undone action.
    pub fn redo(&mut self) -> Result<String, String> {
        let Some(action) = self.redo_items.pop() else {
            return Err("Nothing to redo".into());
        };

        match action {
            UndoAction::Trash(items) => {
                let paths: Vec<&std::path::Path> =
                    items.iter().map(|i| i.original_path.as_path()).collect();
                if let Ok(new_items) = arca_xdg::trash::trash_paths_record(&paths, None) {
                    let count = new_items.len();
                    self.undo_items.push(UndoAction::Trash(new_items));
                    Ok(format!("Moved {count} item(s) to Trash"))
                } else {
                    Err("Cannot redo move to trash".into())
                }
            }
            UndoAction::Rename { original, new_path } => {
                let orig_name = original
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let new_name = new_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                crate::ops::rename_path(&original, &new_path)
                    .map_err(|e| format!("Cannot redo rename: {e}"))?;
                self.undo_items
                    .push(UndoAction::Rename { original, new_path });
                Ok(format!("Renamed '{orig_name}' to '{new_name}'"))
            }
            UndoAction::CreateFolder(path) => {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                std::fs::create_dir_all(&path)
                    .map_err(|e| format!("Cannot recreate folder: {e}"))?;
                self.undo_items.push(UndoAction::CreateFolder(path));
                Ok(format!("Recreated folder '{name}'"))
            }
            UndoAction::Move {
                sources,
                destinations,
            } => {
                let mut moved = 0;
                let mut redone_dsts = Vec::new();
                for (src, dst) in sources.iter().zip(destinations.iter()) {
                    if src.exists() {
                        if let Some(dst_parent) = dst.parent() {
                            if let Ok(new_dst) = crate::ops::move_into(src, dst_parent) {
                                redone_dsts.push(new_dst);
                                moved += 1;
                            }
                        }
                    }
                }
                self.undo_items.push(UndoAction::Move {
                    sources: destinations,
                    destinations: redone_dsts,
                });
                Ok(format!("Redid move of {moved} item(s)"))
            }
            UndoAction::Copy { destinations: _ } => {
                Ok("Redo copy not applicable; re-paste from clipboard".into())
            }
        }
    }
}
