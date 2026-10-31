use application::RepoChanges;
use ui::view::refresh::Reload;

#[test]
fn work_tree_changes_only_reload_the_status() {
    let reload = Reload::after(RepoChanges {
        git_dir: false,
        work_tree: true,
    });
    assert_eq!(
        reload,
        Reload {
            refs: false,
            work_tree: true,
            ongoing: false
        }
    );
}

#[test]
fn git_dir_changes_reload_refs_status_and_ongoing() {
    let reload = Reload::after(RepoChanges {
        git_dir: true,
        work_tree: false,
    });
    assert_eq!(
        reload,
        Reload {
            refs: true,
            work_tree: true,
            ongoing: true
        }
    );
}
