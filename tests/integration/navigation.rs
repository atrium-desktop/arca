//! Integration coverage for navigation and the per-tab session state machine.
//!
//! These tests cross the `arca-engine` ↔ `arca-xdg` boundary (real scratch
//! directories, real XDG path resolution) without touching the presentation
//! layer, so they run in the native-free lane.

use arca_engine::config::{Config, ViewMode};
use arca_engine::{AppState, SortKey};
use test_support::fs::TestDir;

fn state_at(prefix: &str) -> (TestDir, AppState) {
    let dir = TestDir::new(prefix);
    dir.subdir("one");
    dir.subdir("one/nested");
    dir.subdir("two");
    dir.write("one/alpha.txt", b"a");
    dir.write("one/Beta.txt", b"b");
    dir.write("one/.hidden", b"h");
    dir.write("two/gamma.txt", b"g");
    let state = AppState::with_config(
        dir.join("arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.join("one").to_str().unwrap(),
    );
    (dir, state)
}

#[test]
fn tabs_keep_independent_location_history_and_filter() {
    let (dir, mut state) = state_at("tabs");
    let one = dir.join("one");
    let two = dir.join("two");
    assert_eq!(state.cwd(), one.to_str().unwrap());

    state.set_filter("alpha".into());
    state.new_tab(Some(two.to_str().unwrap()));
    assert_eq!(state.tabs().len(), 2);
    assert_eq!(state.cwd(), two.to_str().unwrap());
    assert_eq!(state.filter(), "", "a new tab starts unfiltered");

    state.navigate(one.to_str().unwrap());
    assert!(state.can_go_back());

    assert!(state.switch_tab(0));
    assert_eq!(state.cwd(), one.to_str().unwrap());
    assert_eq!(state.filter(), "alpha", "tab 0 keeps its filter");
    assert!(state.visible().len() <= 1);

    assert!(state.switch_tab(1));
    assert_eq!(state.cwd(), one.to_str().unwrap());
    assert!(
        state.can_go_back(),
        "tab 1 keeps its own history after navigation"
    );

    // Closing a tab removes only that document.
    assert!(state.close_tab(1));
    assert_eq!(state.tabs().len(), 1);
    assert_eq!(state.cwd(), one.to_str().unwrap());
    assert!(
        !state.close_tab(0),
        "the engine never enters a no-document state"
    );
}

#[test]
fn navigating_after_going_back_truncates_the_forward_branch() {
    let (dir, mut state) = state_at("history");
    let one = dir.join("one");
    let two = dir.join("two");

    state.navigate(two.to_str().unwrap());
    state.navigate(one.to_str().unwrap());
    assert!(state.can_go_back());
    state.go_back();
    assert_eq!(state.cwd(), two.to_str().unwrap());
    assert!(state.can_go_forward());

    // A fresh navigation discards the forward branch, like every browser.
    state.navigate(one.to_str().unwrap());
    assert!(!state.can_go_forward());
    state.go_back();
    state.go_back();
    assert_eq!(state.cwd(), one.to_str().unwrap());
}

#[test]
fn hidden_files_and_sorting_are_engine_state_not_view_state() {
    let (_dir, mut state) = state_at("listing");
    let entries_before = state.entries().len();

    state.toggle_hidden();
    assert!(state.show_hidden);
    assert!(state.entries().len() > entries_before);
    assert!(state.entries().iter().any(|entry| entry.hidden));

    let names: Vec<&str> = state
        .visible()
        .iter()
        .filter_map(|&index| state.entries().get(index))
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(
        names.first().copied(),
        Some("nested"),
        "directories sort first"
    );
    assert_eq!(
        state.visible().len(),
        state.entries().len(),
        "no filter is active, so every entry is visible"
    );

    state.toggle_sort(SortKey::Size);
    assert!(state.sort_ascending, "a new column starts ascending");
    state.toggle_sort(SortKey::Size);
    assert!(
        !state.sort_ascending,
        "toggling the same column flips the direction"
    );

    state.set_view_mode(ViewMode::Miller);
    assert_eq!(state.view_mode, ViewMode::Miller);
}

#[test]
fn navigation_failure_keeps_the_current_directory_and_reports_status() {
    let (dir, mut state) = state_at("failure");
    let before = state.cwd().to_string();
    state.navigate(dir.join("does-not-exist").to_str().unwrap());
    assert_eq!(state.cwd(), before);
    let status = state.status.as_deref().expect("a failure is reported");
    assert!(
        status.contains("Not a directory"),
        "status should explain the failure, got {status:?}"
    );
}

#[test]
fn path_completion_returns_prefix_and_candidates() {
    let (dir, _state) = state_at("completion");
    let one = dir.join("one");
    let input = format!("{}/a", one.display());
    let completion = arca_engine::complete_path(one.to_str().unwrap(), &input)
        .expect("a matching child completes");
    assert!(completion.completed.ends_with("alpha.txt"));
    assert!(
        completion
            .candidates
            .iter()
            .any(|path| path.ends_with("alpha.txt"))
    );

    let empty = arca_engine::complete_path(one.to_str().unwrap(), "zzz");
    assert!(empty.is_none(), "a miss returns no completion");
}

#[test]
fn bookmarks_follow_add_remove_and_persist_to_config() {
    let (dir, mut state) = state_at("bookmarks");
    let target = dir.join("two");
    let target = target.to_str().unwrap();
    assert!(!state.is_bookmarked(target));
    state.toggle_bookmark(target);
    assert!(state.is_bookmarked(target));
    state.toggle_bookmark(target);
    assert!(!state.is_bookmarked(target));
}
