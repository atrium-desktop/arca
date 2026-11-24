//! Integration coverage for file operations across `arca-engine` and
//! `arca-xdg`: anti-clobber renames, collision-free folder creation,
//! clipboard copy/move, drag-and-drop payloads, and the FreeDesktop trash
//! roundtrip with undo.

use arca_engine::config::Config;
use arca_engine::{AppState, TransferAction};
use test_support::fs::TestDir;

fn state_in(dir: &TestDir) -> AppState {
    AppState::with_config(
        dir.join("arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.str(),
    )
}

fn select(app: &mut AppState, name: &str) {
    let index = app
        .visible()
        .iter()
        .position(|&entry| app.entries()[entry].name == name)
        .unwrap_or_else(|| panic!("{name} is not visible"));
    app.select_visible(index);
}

#[test]
fn rename_refuses_to_clobber_then_succeeds_to_a_free_name() {
    let dir = TestDir::new("rename");
    dir.write("a.txt", b"a");
    dir.write("b.txt", b"b");
    let mut app = state_in(&dir);

    assert!(!app.rename("a.txt", "b.txt"), "rename must not clobber");
    assert_eq!(std::fs::read(dir.join("b.txt")).unwrap(), b"b");
    assert!(dir.join("a.txt").exists(), "the source is untouched");

    assert!(app.rename("a.txt", "c.txt"));
    assert!(!dir.join("a.txt").exists());
    assert_eq!(std::fs::read(dir.join("c.txt")).unwrap(), b"a");
}

#[test]
fn create_folder_picks_unique_names_and_undo_removes_it() {
    let dir = TestDir::new("mkdir");
    let mut app = state_in(&dir);

    let first = app.create_folder().expect("first folder");
    let second = app.create_folder().expect("second folder");
    assert_eq!(first, "New Folder");
    assert_eq!(second, "New Folder 2");
    assert!(dir.join(&first).is_dir());
    assert!(dir.join(&second).is_dir());

    app.undo();
    assert!(!dir.join(&second).exists(), "undo removes the last folder");
    app.redo();
    assert!(dir.join(&second).is_dir(), "redo recreates it");
}

#[test]
fn clipboard_copy_and_paste_roundtrip_with_undo() {
    let dir = TestDir::new("copy");
    dir.write("source.txt", b"payload");
    let destination = dir.subdir("destination");
    let mut app = state_in(&dir);

    select(&mut app, "source.txt");
    app.yank_selected(TransferAction::Copy);
    let payload = app.clipboard_payload().expect("clipboard payload");
    assert!(
        payload.contains("source.txt"),
        "the payload names the source, got {payload:?}"
    );

    app.navigate(destination.to_str().unwrap());
    app.paste();
    let copied = destination.join("source.txt");
    assert!(copied.exists(), "paste creates the copy");
    assert_eq!(std::fs::read(&copied).unwrap(), b"payload");
    assert!(dir.join("source.txt").exists(), "a copy keeps the source");

    app.undo();
    assert!(!copied.exists(), "undo removes the copied file");
}

#[test]
fn drop_payload_moves_every_selected_item() {
    let dir = TestDir::new("drop");
    dir.write("one.txt", b"1");
    dir.write("two.txt", b"2");
    let target = dir.subdir("target");
    let mut app = state_in(&dir);

    select(&mut app, "one.txt");
    app.select_range_visible(2);
    assert_eq!(app.selected_count(), 2);
    let selected: Vec<std::path::PathBuf> = app
        .selected_paths()
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect();
    let payload = arca_xdg::FileClipboard::new(selected, TransferAction::Move).to_uri_list();

    assert!(app.drop_into(&payload, target.to_str().unwrap(), TransferAction::Move));
    assert!(!dir.join("one.txt").exists());
    assert!(!dir.join("two.txt").exists());
    assert!(target.join("one.txt").exists());
    assert!(target.join("two.txt").exists());
}

#[test]
fn trash_roundtrip_through_engine_and_undo() {
    let dir = TestDir::new("trash");
    let trash_root = dir.subdir("trash-root");
    dir.write("junk.txt", b"junk");
    let mut app = AppState::with_config_and_trash_root(
        dir.join("arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.str(),
        Some(trash_root.clone()),
    );
    select(&mut app, "junk.txt");

    app.trash_selected();
    assert!(!dir.join("junk.txt").exists(), "the file left the listing");
    let trashed = arca_xdg::trash::list_trash_in_root(&trash_root).expect("list scratch trash");
    assert_eq!(trashed.len(), 1, "exactly one trash record was written");
    assert_eq!(trashed[0].original_name, "junk.txt");
    assert!(
        trashed[0].deletion_date.len() >= 10,
        "the record carries a deletion date: {:?}",
        trashed[0].deletion_date
    );

    app.undo();
    assert!(dir.join("junk.txt").exists(), "undo restores the file");
    assert!(
        arca_xdg::trash::list_trash_in_root(&trash_root)
            .expect("list scratch trash")
            .is_empty(),
        "the trash record is gone after restore"
    );
}

#[test]
fn purge_permanently_removes_a_trashed_record() {
    let dir = TestDir::new("purge");
    let trashed_file = dir.write("gone.txt", b"gone");
    let root = dir.subdir("custom-trash");
    let items =
        arca_xdg::trash::trash_paths_record(&[&trashed_file], Some(&root)).expect("trash it");
    assert!(!trashed_file.exists());
    assert_eq!(items.len(), 1);

    arca_xdg::trash::purge_trash_item(&items[0]).expect("purge it");
    assert!(!items[0].file_path.exists(), "the payload is deleted");
    assert!(!items[0].info_path.exists(), "the metadata is deleted");
    assert!(arca_xdg::trash::list_trash_in_root(&root)
        .expect("list custom root")
        .is_empty());
}
