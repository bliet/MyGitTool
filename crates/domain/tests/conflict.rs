use domain::{ConflictHunk, MergeChunk, Resolution, merged_text};

fn hunk(ours: &str, theirs: &str) -> ConflictHunk {
    ConflictHunk {
        base: "base\n".into(),
        ours: ours.into(),
        theirs: theirs.into(),
    }
}

#[test]
fn resolutions_pick_or_combine_the_sides() {
    let h = hunk("ours\n", "theirs\n");
    assert_eq!(Resolution::Ours.apply(&h), "ours\n");
    assert_eq!(Resolution::Theirs.apply(&h), "theirs\n");
    assert_eq!(Resolution::OursThenTheirs.apply(&h), "ours\ntheirs\n");
    assert_eq!(Resolution::TheirsThenOurs.apply(&h), "theirs\nours\n");
    assert_eq!(Resolution::Custom("mine\n".into()).apply(&h), "mine\n");
}

#[test]
fn combining_at_the_end_of_a_file_keeps_the_lines_apart() {
    let h = hunk("ours", "theirs");
    assert_eq!(Resolution::OursThenTheirs.apply(&h), "ours\ntheirs");
}

#[test]
fn the_merged_text_needs_every_conflict_decided() {
    let chunks = [
        MergeChunk::Clean("top\n".into()),
        MergeChunk::Conflict(hunk("a\n", "b\n")),
        MergeChunk::Clean("middle\n".into()),
        MergeChunk::Conflict(hunk("c\n", "d\n")),
    ];
    assert_eq!(merged_text(&chunks, &[Some(Resolution::Ours), None]), None);
    assert_eq!(
        merged_text(&chunks, &[Some(Resolution::Ours), Some(Resolution::Theirs)]).as_deref(),
        Some("top\na\nmiddle\nd\n")
    );
}
