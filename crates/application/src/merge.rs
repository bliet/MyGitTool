//! Line-based three-way merge, as `git merge-file` does it.

use domain::{ConflictHunk, MergeChunk};
use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};
use std::ops::Range;
use std::time::{Duration, Instant};

/// How long each diff may take before `similar` settles for a coarser one.
const TIMEOUT: Duration = Duration::from_secs(2);

/// A changed region of one side against the base.
#[derive(Debug, Clone)]
struct Hunk {
    base: Range<usize>,
    side: Range<usize>,
}

/// Merges the changes from `base` to `ours` and from `base` to `theirs`.
/// Changes on one side only are taken; changes on both sides that overlap
/// or touch become conflicts, unless they are the same. Line endings are
/// kept, so merging and joining the chunks reproduces the files exactly.
pub fn merge3(base: &str, ours: &str, theirs: &str) -> Vec<MergeChunk> {
    let base: Vec<&str> = base.split_inclusive('\n').collect();
    let sides: [Vec<&str>; 2] = [
        ours.split_inclusive('\n').collect(),
        theirs.split_inclusive('\n').collect(),
    ];
    let mut hunks: Vec<(usize, Hunk)> = Vec::new();
    for (side, lines) in sides.iter().enumerate() {
        hunks.extend(changes(&base, lines).into_iter().map(|h| (side, h)));
    }
    hunks.sort_by_key(|(_, h)| (h.base.start, h.base.end));

    let mut chunks = Vec::new();
    let mut clean = String::new();
    // Base lines up to `done` are merged; `offset` maps a base position
    // after the last region to the same position in each side.
    let mut done = 0;
    let mut offset = [0isize; 2];
    let mut i = 0;
    while i < hunks.len() {
        let start = hunks[i].1.base.start;
        let mut end = hunks[i].1.base.end;
        let mut j = i + 1;
        while j < hunks.len() && hunks[j].1.base.start <= end {
            end = end.max(hunks[j].1.base.end);
            j += 1;
        }
        clean.extend(base[done..start].iter().copied());

        let mut touched = [false; 2];
        let mut growth = [0isize; 2];
        for (side, hunk) in &hunks[i..j] {
            touched[*side] = true;
            growth[*side] += hunk.side.len() as isize - hunk.base.len() as isize;
        }
        // Positions stay within the side, as the hunks come from its diff.
        let text = |side: usize| -> String {
            let from = (start as isize + offset[side]) as usize;
            let to = (end as isize + offset[side] + growth[side]) as usize;
            sides[side][from..to].concat()
        };
        let (ours, theirs) = (text(0), text(1));
        match touched {
            [true, false] => clean.push_str(&ours),
            [false, true] => clean.push_str(&theirs),
            _ if ours == theirs => clean.push_str(&ours),
            _ => {
                if !clean.is_empty() {
                    chunks.push(MergeChunk::Clean(std::mem::take(&mut clean)));
                }
                chunks.push(MergeChunk::Conflict(ConflictHunk {
                    base: base[start..end].concat(),
                    ours,
                    theirs,
                }));
            }
        }
        offset[0] += growth[0];
        offset[1] += growth[1];
        done = end;
        i = j;
    }
    clean.extend(base[done..].iter().copied());
    if !clean.is_empty() {
        chunks.push(MergeChunk::Clean(clean));
    }
    chunks
}

/// The changed regions of `side` against `base`, in order.
fn changes(base: &[&str], side: &[&str]) -> Vec<Hunk> {
    let deadline = Instant::now().checked_add(TIMEOUT);
    let mut hunks: Vec<Hunk> = Vec::new();
    for op in capture_diff_slices_deadline(Algorithm::Myers, base, side, deadline) {
        let (tag, base_range, side_range) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        // Consecutive operations form one change.
        match hunks.last_mut() {
            Some(last)
                if last.base.end == base_range.start && last.side.end == side_range.start =>
            {
                last.base.end = base_range.end;
                last.side.end = side_range.end;
            }
            _ => hunks.push(Hunk {
                base: base_range,
                side: side_range,
            }),
        }
    }
    hunks
}
