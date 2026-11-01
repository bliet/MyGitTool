use domain::{ConflictHunk, ConflictSide, MergeChunk, OngoingOperation, Resolution, SideKind};
use ui::view::conflict::{
    Resolver, clean_preview, progress_text, shown, side_titles, take_label, whole_file_text,
};
use ui::view::diff::Direction;

fn hunk(ours: &str, theirs: &str) -> MergeChunk {
    MergeChunk::Conflict(ConflictHunk {
        base: String::new(),
        ours: ours.into(),
        theirs: theirs.into(),
    })
}

fn resolver() -> Resolver {
    Resolver::new(vec![
        MergeChunk::Clean("top\n".into()),
        hunk("a\n", "b\n"),
        MergeChunk::Clean("middle\n".into()),
        hunk("c\n", "d\n"),
        hunk("e\n", "f\n"),
    ])
}

#[test]
fn the_file_is_merged_once_every_conflict_is_decided() {
    let mut resolver = resolver();
    assert_eq!(resolver.conflict_count(), 3);
    resolver.resolve(0, Some(Resolution::Ours));
    resolver.resolve(1, Some(Resolution::Theirs));
    assert_eq!(resolver.resolved_count(), 2);
    assert_eq!(resolver.merged(), None);

    resolver.resolve(2, Some(Resolution::Custom("mine\n".into())));
    assert_eq!(
        resolver.merged().as_deref(),
        Some("top\na\nmiddle\nd\nmine\n")
    );

    resolver.resolve(1, None);
    assert_eq!(resolver.resolved_count(), 2);
    assert_eq!(resolver.merged(), None);
}

#[test]
fn navigation_starts_at_the_first_undecided_conflict_and_wraps() {
    let mut resolver = resolver();
    resolver.resolve(0, Some(Resolution::Ours));
    assert_eq!(resolver.step(None, Direction::Next), Some(1));
    assert_eq!(resolver.step(Some(2), Direction::Next), Some(0));
    assert_eq!(resolver.step(Some(0), Direction::Previous), Some(2));
    assert_eq!(Resolver::new(Vec::new()).step(None, Direction::Next), None);
}

#[test]
fn long_clean_parts_are_shortened() {
    let text: String = (1..=10).map(|n| format!("{n}\n")).collect();
    let preview = clean_preview(&text, 3);
    assert_eq!(preview.head, "1\n2\n3\n");
    assert_eq!(preview.hidden, 4);
    assert_eq!(preview.tail, "8\n9\n10\n");
    assert_eq!(clean_preview("1\n2\n", 3).hidden, 0);
}

#[test]
fn texts_explain_the_sides() {
    assert_eq!(
        side_titles(Some(OngoingOperation::Rebase)),
        ("Ours: rebasing onto", "Theirs: your commit")
    );
    assert_eq!(side_titles(None), ("Ours", "Theirs"));
    assert_eq!(progress_text(1, 3), "1 of 3 conflicts resolved");
    assert_eq!(
        whole_file_text(SideKind::Text, SideKind::Deleted),
        "Ours changed the file and theirs deleted the file. Choose the version to keep."
    );
    assert_eq!(
        take_label(ConflictSide::Theirs, SideKind::Deleted),
        "Delete the file (theirs)"
    );
    assert_eq!(take_label(ConflictSide::Ours, SideKind::Binary), "Use ours");
    assert_eq!(shown("a\r\nb\r\n"), "a\r\nb");
}
