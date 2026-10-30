# CLAUDE.md

Guidance for Claude (and humans) working on this repository.

## Project

MyGitTool is a desktop Git client written in Rust with Dioxus 0.7 (desktop) as the UI and git2 for Git access. The main window has three columns: the branch tree on the left, the commit history graph in the middle and the commit details on the right. A separate window shows a side-by-side diff. The feature scope and its current status live in [ROADMAP.md](ROADMAP.md). Read it before starting a feature, and tick off items when they are done.

## Architecture: Clean Architecture

Dependencies point **inward only**:

```
app (binary, composition root)
 ├── ui          (Dioxus)        ─┐
 ├── infra-git   (git2, notify)  ─┼──> application ──> domain
 └── infra-config (config files) ─┘
```

| Crate | Responsibility | May depend on |
|---|---|---|
| `crates/domain` | Pure types and rules: `Commit`, `CommitId`, `Branch` (local/remote), `Remote`, `Signature`, `FileChange`, `ChangeKind`, `DiffHunk`, `GraphLane`, the domain errors | `std` and small pure crates only (e.g. `thiserror`). **No I/O.** |
| `crates/application` | Use cases (`OpenRepository`, `RecentRepositories`, `LoadBranches`, `LoadHistory`, `GetCommitDetails`, `LoadWorkTreeChanges`, `ComputeFileDiff`, `Fetch`, `Pull`, `Push`, `Checkout`, `CreateBranch`, `DeleteBranch`, `MergeIntoCurrent`, `RebaseCurrentOnto`, `ContinueOperation`, `AbortOperation`, `LoadOngoing`, `UserSettings`), **port traits** (`RepositoryLocator`, `RecentRepositoriesStore`, `SettingsStore`, `RepositoryReader`, `RepositoryWriter`, `RemoteOperations`, `ProgressReporter`, `CredentialProvider`, `RepoWatcher`), graph-lane layout, diff alignment (`similar`) | `domain` |
| `crates/infra-git` | Adapters that implement the ports with `git2` (and `notify` for the watcher) | `application`, `domain`, `git2`, `notify` |
| `crates/infra-config` | Adapters for app state in the user config dir (the recent-repositories list and the user settings as JSON) | `application`, `serde`, `directories` |
| `crates/ui` | Dioxus components and view state. Calls use cases only, never git2. | `application`, `domain`, `dioxus` |
| `crates/app` | `main`. Parses the CLI argument, builds the adapters, injects them into the use cases and launches the UI. | everything |

Hard rules:
- **No `git2` types outside `infra-git`.** Map them to domain types at the adapter boundary.
- **No Dioxus types outside `ui`.**
- Port traits are **synchronous**. The UI runs use cases via `tokio::task::spawn_blocking`, and long operations (fetch, push) report progress through a channel or callback that the port defines.
- Every operation that changes the repository checks for a dirty work tree first. If the tree is dirty, it returns a domain error (`DirtyWorkTree`) listing the dirty files. Dirty means tracked files with staged or unstaged changes; untracked files don't count. Never discard changes silently.
- Merge and rebase conflicts: detect them, report the conflicting files, and offer to **abort** or to **leave the state as is** for the user to resolve externally. There is no in-app resolver yet.
- Credentials are tried in this order: SSH agent, default SSH keys, git credential helper, then a UI prompt via `CredentialProvider`. Never persist secrets.
- Pull honours `pull.rebase` and `pull.ff` from the git config.

## Commands

```sh
cargo build                                  # build all crates
cargo run -p app -- <repo-path>              # run the app (binary: mygittool)
cargo test --workspace                       # all tests
cargo fmt --all --check                      # formatting (CI enforced)
cargo clippy --workspace --all-targets -- -D warnings   # lints (CI enforced)
```

Run fmt, clippy and the tests before you consider a change done.

## Testing

**Source and tests live in separate places.** `crates/<crate>/src/` holds production code only, and every test goes in `crates/<crate>/tests/`. Source files contain no `#[cfg(test)] mod tests` blocks and no test helpers. Tests use the crate's public API. If something can't be tested that way, the API needs a better shape; don't add test-only visibility.

```
crates/application/
├── src/            # production code only
└── tests/
    ├── common/mod.rs        # shared fakes and fixtures
    ├── load_history.rs
    └── pull.rs
```

- **domain / application:** unit tests, written TDD style. Test use cases against hand-written fake port implementations in `tests/common/`. No real repositories.
- **infra-git:** integration tests against temporary repositories built with a fixture helper (`tempfile` + `git2`) in `tests/common/`, e.g. `TestRepo::new().commit("a.txt", "1").branch("feature")...`. Network operations are tested against a local bare repo used as the remote.
- **ui:** keep the logic out of components (put it in view-model or state structs) so it can be unit-tested. Component tests are optional.
- CI runs fmt, clippy and the tests on Linux, Windows and macOS.

## Conventions

- Rust edition 2024, latest stable toolchain.
- Errors: `thiserror` enums in domain, application and infra. `anyhow` only in `app`. No `unwrap()` or `expect()` outside tests, except where an invariant makes failure impossible; document that invariant in a comment.
- Keep dependencies at the crate level and declare their versions once in `[workspace.dependencies]`.
- The history must stay responsive on large repos (100k+ commits). Load commits lazily or page them, and virtualize the list in the UI.
- Code, comments, commit messages and docs are in English.
- Keyboard shortcuts: `F5` refreshes. In the diff window, `F7` jumps to the next change, `F6` to the previous one, and `Esc` closes it.
