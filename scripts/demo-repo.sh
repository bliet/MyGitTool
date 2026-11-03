#!/usr/bin/env bash
# Builds the demo repository used for the README screenshots.
#
#   scripts/demo-repo.sh <target-dir> [--conflict]
#
# The repository has several local branches, two remotes with their own
# branches, and a few tags. With --conflict it is left in the middle of a
# merge with a conflict in src/storage.rs, ready for the conflict resolver.
set -euo pipefail

target=${1:?usage: demo-repo.sh <target-dir> [--conflict]}
conflict=${2:-}

rm -rf "$target"
mkdir -p "$target"
target=$(cd "$target" && pwd)
work="$target/taskline"
origin="$target/origin.git"
upstream="$target/upstream.git"

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
RANDOM=42
clock=$(date -d '2026-09-01 09:00:00 +0200' +%s)

tick() { # moves the clock 40 to 160 minutes on, and to the next morning after 18:00
    clock=$((clock + 2400 + RANDOM % 7200))
    local offset=$((2 * 3600)) # the commits are in UTC+2
    local midnight=$(((clock + offset) / 86400 * 86400 - offset))
    if ((clock - midnight >= 18 * 3600)); then
        clock=$((midnight + 86400 + 9 * 3600 + RANDOM % 1800))
    fi
}
as() { # as <name> <email> <message>: commits as someone, a while after the last commit
    tick
    GIT_AUTHOR_NAME=$1 GIT_AUTHOR_EMAIL=$2 GIT_COMMITTER_NAME=$1 GIT_COMMITTER_EMAIL=$2 \
        GIT_AUTHOR_DATE="@$clock +0200" GIT_COMMITTER_DATE="@$clock +0200" \
        git commit -q -m "$3"
}
alice() { as "Alice Martin" alice@example.com "$1"; }
bob() { as "Bob Chen" bob@example.com "$1"; }
carla() { as "Carla Diaz" carla@example.com "$1"; }
merge() { # merge <branch> <message>, as Alice
    tick
    GIT_AUTHOR_NAME="Alice Martin" GIT_AUTHOR_EMAIL=alice@example.com \
        GIT_COMMITTER_NAME="Alice Martin" GIT_COMMITTER_EMAIL=alice@example.com \
        GIT_AUTHOR_DATE="@$clock +0200" GIT_COMMITTER_DATE="@$clock +0200" \
        git merge -q --no-ff "$1" -m "$2"
}
tag() { git tag -a "$1" -m "Release $1"; }
write() { mkdir -p "$(dirname "$1")"; cat > "$1"; git add "$1"; }

git init -q -b main "$work"
cd "$work"
git config user.name "Alice Martin"
git config user.email alice@example.com

write README.md <<'EOF'
# Taskline

A tiny command-line task list.
EOF
alice "Initial commit"

write Cargo.toml <<'EOF'
[package]
name = "taskline"
version = "0.1.0"
edition = "2024"
EOF
write src/main.rs <<'EOF'
mod task;

fn main() {
    let tasks = vec![task::Task::new("Write the README")];
    for task in &tasks {
        println!("{task}");
    }
}
EOF
write src/task.rs <<'EOF'
use std::fmt;

pub struct Task {
    pub title: String,
    pub done: bool,
}

impl Task {
    pub fn new(title: &str) -> Self {
        Self { title: title.to_owned(), done: false }
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mark = if self.done { "x" } else { " " };
        write!(f, "[{mark}] {}", self.title)
    }
}
EOF
alice "Add the task type and a first main"
tag v0.1.0

write src/storage.rs <<'EOF'
use crate::task::Task;
use std::fs;
use std::io;

/// The file the task list lives in.
pub const DEFAULT_FILE: &str = "tasks.txt";

/// Reads the task list from a file, one task per line.
pub fn load(path: &str) -> io::Result<Vec<Task>> {
    let text = fs::read_to_string(path)?;
    Ok(text.lines().map(Task::new).collect())
}

/// Writes the task list to a file, one task per line.
pub fn save(path: &str, tasks: &[Task]) -> io::Result<()> {
    let text: Vec<String> = tasks.iter().map(|t| t.title.clone()).collect();
    fs::write(path, text.join("\n"))
}

/// Adds a task at the end of the list.
pub fn append(path: &str, task: Task) -> io::Result<()> {
    let mut tasks = load(path)?;
    tasks.push(task);
    save(path, &tasks)
}

/// Removes every task that is done.
pub fn clean_up(path: &str) -> io::Result<usize> {
    let tasks = load(path)?;
    let before = tasks.len();
    let open: Vec<Task> = tasks.into_iter().filter(|t| !t.done).collect();
    save(path, &open)?;
    Ok(before - open.len())
}
EOF
write src/main.rs <<'EOF'
mod storage;
mod task;

fn main() -> std::io::Result<()> {
    let tasks = storage::load(storage::DEFAULT_FILE).unwrap_or_default();
    for task in &tasks {
        println!("{task}");
    }
    storage::save(storage::DEFAULT_FILE, &tasks)
}
EOF
bob "Store tasks in a text file"

git switch -q -c feature/due-dates
write src/task.rs <<'EOF'
use std::fmt;

pub struct Task {
    pub title: String,
    pub done: bool,
    pub due: Option<String>,
}

impl Task {
    pub fn new(title: &str) -> Self {
        Self { title: title.to_owned(), done: false, due: None }
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mark = if self.done { "x" } else { " " };
        write!(f, "[{mark}] {}", self.title)?;
        if let Some(due) = &self.due {
            write!(f, " (due {due})")?;
        }
        Ok(())
    }
}
EOF
carla "Tasks can have a due date"

git switch -q main
write .gitignore <<'EOF'
/target
tasks.txt
EOF
alice "Ignore the build output and the task file"

git switch -q feature/due-dates
write README.md <<'EOF'
# Taskline

A tiny command-line task list.

Tasks can have a due date, shown next to the title.
EOF
carla "Document due dates"

git switch -q main
merge feature/due-dates "Merge branch 'feature/due-dates'"
sed -i 's/^version = .*/version = "0.2.0"/' Cargo.toml && git add Cargo.toml
alice "Release 0.2.0"
tag v0.2.0

# The commit the diff screenshot shows: word-level changes in one file.
write src/storage.rs <<'EOF'
use crate::task::Task;
use std::fs;
use std::io::{self, ErrorKind::NotFound};
use std::path::Path;

/// The file the task list lives in.
pub const DEFAULT_FILE: &str = "tasks.txt";

/// Reads the task list from a file, one task per line. A missing
/// file is an empty list.
pub fn load(path: &Path) -> io::Result<Vec<Task>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    Ok(text.lines().filter(|l| !l.is_empty()).map(Task::new).collect())
}

/// Writes the task list to a file, one task per line, through a
/// temporary file so a crash never leaves half a list behind.
pub fn save(path: &Path, tasks: &[Task]) -> io::Result<()> {
    let text: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
    let temp = path.with_extension("tmp");
    fs::write(&temp, text.join("\n") + "\n")?;
    fs::rename(temp, path)
}

/// Adds a task at the end of the list.
pub fn append(path: &Path, task: Task) -> io::Result<()> {
    let mut tasks = load(path)?;
    tasks.push(task);
    save(path, &tasks)
}

/// Removes every task that is done.
pub fn clean_up(path: &Path) -> io::Result<usize> {
    let tasks = load(path)?;
    let before = tasks.len();
    let open: Vec<Task> = tasks.into_iter().filter(|t| !t.done).collect();
    save(path, &open)?;
    Ok(before - open.len())
}
EOF
bob "Save the task list atomically and treat a missing file as empty"

git switch -q -c release/0.3
sed -i 's/^version = .*/version = "0.3.0"/' Cargo.toml && git add Cargo.toml
alice "Prepare 0.3.0"
tag v0.3.0-rc1

git switch -q main
git switch -q -c feature/export-csv
write src/export.rs <<'EOF'
use crate::task::Task;

/// Formats the task list as CSV with a header row.
pub fn to_csv(tasks: &[Task]) -> String {
    let mut out = String::from("title,done\n");
    for task in tasks {
        out.push_str(&format!("\"{}\",{}\n", task.title.replace('"', "\"\""), task.done));
    }
    out
}
EOF
carla "Export the task list as CSV"
# One side of the conflict: the file name and the clean-up in storage.rs.
sed -i 's|"tasks.txt"|"tasks.csv"|; s|Err(e) => return Err(e),|Err(e) => return Err(io::Error::new(e.kind(), format!("{}: {e}", path.display()))),|; s|/// Removes every task that is done.|/// Removes the done tasks and returns how many there were.|' src/storage.rs
git add src/storage.rs
carla "Keep tasks in CSV and name the file in read errors"

git switch -q main
git switch -q -c fix/empty-titles
write src/task.rs <<'EOF'
use std::fmt;

pub struct Task {
    pub title: String,
    pub done: bool,
    pub due: Option<String>,
}

impl Task {
    pub fn new(title: &str) -> Self {
        Self { title: title.trim().to_owned(), done: false, due: None }
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mark = if self.done { "x" } else { " " };
        write!(f, "[{mark}] {}", self.title)?;
        if let Some(due) = &self.due {
            write!(f, " (due {due})")?;
        }
        Ok(())
    }
}
EOF
bob "Trim whitespace around task titles"

git switch -q main
# The other side of the conflict.
sed -i 's|"tasks.txt"|".taskline"|; s|/// The file the task list lives in.|/// The file the task list lives in, relative to the home directory.|; s|/// Removes every task that is done.|/// Removes every finished task.|' src/storage.rs
git add src/storage.rs
bob "Move the task list to a dot file"

git switch -q -c feature/priorities
sed -i 's/    pub due: Option<String>,/    pub due: Option<String>,\n    pub priority: u8,/; s/due: None }/due: None, priority: 0 }/' src/task.rs
git add src/task.rs
alice "Add a priority to tasks"

# Remotes: origin has most branches, upstream is the project's main repo.
git init -q --bare "$origin"
git init -q --bare "$upstream"
git remote add origin "$origin"
git remote add upstream "$upstream"
git push -q origin main feature/export-csv release/0.3 feature/due-dates --tags
git push -q upstream main --tags

# A branch that only exists on the remote, and one where the remote is ahead.
git switch -q -c feature/search main
write src/search.rs <<'EOF'
use crate::task::Task;

/// The tasks whose title contains `needle`, ignoring case.
pub fn search<'a>(tasks: &'a [Task], needle: &str) -> Vec<&'a Task> {
    let needle = needle.to_lowercase();
    tasks.iter().filter(|t| t.title.to_lowercase().contains(&needle)).collect()
}
EOF
carla "Search tasks by title"
git push -q origin feature/search
git switch -q main
git branch -q -D feature/search

git switch -q release/0.3
write CHANGELOG.md <<'EOF'
# Changelog

## 0.3.0

- Atomic saves
- CSV export
EOF
alice "Add a changelog for 0.3.0"
git push -q origin release/0.3
git reset -q --hard HEAD~1

git switch -q main
git fetch -q origin
git fetch -q upstream
git branch -q -u origin/main main
git branch -q -u origin/feature/export-csv feature/export-csv
git branch -q -u origin/release/0.3 release/0.3
git commit-graph write --reachable 2>/dev/null || true

if [[ $conflict == --conflict ]]; then
    git merge -q --no-ff feature/export-csv -m "Merge branch 'feature/export-csv'" >/dev/null 2>&1 || true
fi

echo "$work"
