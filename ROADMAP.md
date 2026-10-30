# Roadmap

Tick each item (`[x]`) when it is merged. Milestones are built in order, and each one leaves the app runnable.

## M0: Scaffolding

- [x] Cargo workspace with crates `domain`, `application`, `infra-git`, `ui`, `app`
- [x] Shared dependency versions in `[workspace.dependencies]` (dioxus 0.7, git2 with `vendored-libgit2`, similar, notify, tokio, thiserror, anyhow, tempfile)
- [x] `app` launches an empty Dioxus desktop window with a three-column layout
- [x] Open a repo: from the CLI argument, else the current directory, else a folder picker. The recent-repos list is persisted in the user config dir.
- [x] CI (GitHub Actions) on Linux, Windows and macOS runs `fmt --check`, `clippy -D warnings` and `test`
- [x] `infra-git` test fixture helper (`TestRepo`) in `tests/common/` for building temp repositories (tests stay out of `src/`, see CLAUDE.md)

## M1: Read-only viewer

- [x] Domain types: `CommitId`, `Commit`, `Signature`, `Branch` (local/remote, upstream, ahead/behind), `Remote`, `Tag`, `FileChange`, `ChangeKind` (added, modified, deleted, renamed, type-changed)
- [x] Ports `RepositoryReader` plus the git2 adapter
- [x] **Left column:** tree of the local branches (grouped by `/` prefix) and the remotes, each with its branches. The current branch is highlighted, and ahead/behind counts are shown.
- [x] **Middle column:** history across all refs in topological order. Each row shows the one-line message, author, short hash, date and ref badges.
- [x] Lazy, paged loading and a virtualized list for large repositories
- [x] Commit graph lanes (layout computed in `application`, drawn in `ui`)
- [x] An "Uncommitted changes" row at the top when the work tree is dirty
- [x] Double-clicking a branch filters the history to that branch. A filter chip with ✕ (or `Esc`) clears it.
- [x] Selecting a branch on the left scrolls the history to the branch's tip
- [x] **Right column:** details of the selected commit: full and short hash (copyable), parents (clickable), author and committer with dates, full message, refs, and the changed files with A/M/D/R status and +/- line counts
- [x] Toggle between a tree and a flat list of changed files
- [x] Resizable columns
- [x] `F5` and a refresh button

## M2: Diff window

- [x] Use case `ComputeFileDiff`: old and new content via the port, a line diff with `similar`, aligned side-by-side rows, and changes highlighted within lines
- [x] Double-clicking a changed file opens a **separate window** (one per file)
- [x] Side-by-side view with line numbers, synchronized scrolling, and colours for added, removed and changed lines
- [x] `F7` jumps to the next change and `F6` to the previous one (wrapping, with a status-bar hint). `Esc` closes the window.
- [x] Edge cases: binary files ("Binary file changed"), renames (show both paths), added and deleted files (one empty side), very large files
- [x] Works for the uncommitted-changes row too (HEAD vs. work tree)

## M3: Remote operations

- [ ] Port `RemoteOperations` with progress reporting, plus the git2 adapter
- [ ] Credential chain: SSH agent, default SSH keys, git credential helper, then a UI prompt (`CredentialProvider`). Nothing is stored.
- [ ] **Fetch.** On a local branch it fetches the upstream's remote. On a remote branch or remote node it fetches that remote.
- [ ] **Pull** (current branch only): fetch, then fast-forward, merge or rebase according to `pull.rebase` / `pull.ff`
- [ ] **Push** a local branch to its upstream (no force push). Offer to set the upstream if none is set.
- [ ] A progress indicator and an error display that doesn't block the UI

## M4: Branch operations

- [ ] Dirty-work-tree guard: block the operation and list the dirty files. A checkout that doesn't touch modified files is allowed.
- [ ] **Checkout** a local branch. On a remote branch, create or switch to the local tracking branch.
- [ ] **Create branch** from a branch or a commit (name dialog with validation)
- [ ] **Delete branch.** Local: refuse an unmerged branch unless the user confirms. Remote: delete it on the server after a confirm dialog.
- [ ] **Merge branch into current branch.** Fast-forward when possible, otherwise create a merge commit.
- [ ] **Rebase current branch onto this one**
- [ ] Conflict handling for merge, rebase and pull: list the conflicting files, then offer *Abort* or *Leave for external resolution*
- [ ] A banner when the repo is mid-merge or mid-rebase, with Abort and Continue buttons

## M5: Polish

- [ ] File system watcher (`notify`) on `.git/` and the work tree, debounced to about 300 ms, triggering an automatic refresh
- [ ] Performance pass on large repos (Linux kernel–sized history)
- [ ] Settings: theme (light, dark or system), date format, default file view (tree or flat)
- [ ] Remember the window layout (column widths, window size)
- [ ] Packaged release builds for Linux, Windows and macOS

## Later

- [ ] Auto-stash before checkout, pull, merge and rebase (and reapply afterwards)
- [ ] In-app three-way conflict resolver
- [ ] Staging, unstaging and committing
- [ ] Unified (inline) mode in the diff window
- [ ] Multiple repositories in tabs
- [ ] Optional credential storage in the OS keychain
- [ ] Tags: create, delete and push
- [ ] Stash list in the left column
- [ ] Search and filter the history by message, author or hash
- [ ] Force push (`--force-with-lease`) with a confirmation
