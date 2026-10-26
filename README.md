# MyGitTool

A fast, cross-platform desktop Git client written in Rust that shows a repository, its branches and its remotes at a glance.

> **Status:** early development. Nothing works yet. See [ROADMAP.md](ROADMAP.md) for what's planned and what's done.

> [!WARNING]
> **This tool is developed by AI.** The code, tests and documentation are written by [Claude Code](https://claude.com/claude-code). Use it at your own risk, especially for operations that modify your repository (pull, push, merge, rebase, delete branch).

## Features (planned)

The window has three columns:

| Left: Branches | Middle: History | Right: Commit details |
|---|---|---|
| Tree of local and remote branches | Commit graph across all branches | Changed files (tree or flat list) |
| Context menu: Fetch, Checkout, Pull, Push, Create / Delete Branch, Merge into current, Rebase current onto this | One-line message, author, short hash, ref badges | Full message, author / committer, dates, parents, hashes |
| Double-click a branch to filter the history to it | "Uncommitted changes" row at the top | Double-click a file to open its diff |

**Diff window:** opens in its own window and shows the file side by side, highlighting the changed words within each line.

| Key | Action |
|---|---|
| `F7` | Jump to the next change |
| `F6` | Jump to the previous change |
| `Esc` | Close the diff window |

Other planned features:
- Fetch, pull and push use your SSH agent, your default SSH keys or your git credential helper. If none of those work, the app asks for credentials and does not store them.
- Pull follows your `pull.rebase` / `pull.ff` git config.
- The view refreshes automatically when the repository changes outside the app.
- Runs on Linux, Windows and macOS.

## Usage

```sh
mygittool [PATH]
```

If you give a `PATH`, that repository opens. Without one, the app opens the current directory if it is a Git repository and otherwise shows a folder picker. Recently opened repositories are remembered.

## Building from source

You need a current stable Rust toolchain (`rustup update stable`).

```sh
git clone <this repo>
cd MyGitTool
cargo run --release -p app -- /path/to/repo
```

On Linux, Dioxus desktop needs the WebKitGTK development packages. On Debian/Ubuntu install them with:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev
```

On Arch, install `webkit2gtk-4.1` and `xdotool`. Windows and macOS need no extra packages.

## Tech stack

- **UI:** [Dioxus](https://dioxuslabs.com/) 0.7 (desktop renderer)
- **Git:** [git2](https://crates.io/crates/git2) (libgit2 bindings, vendored)
- **Diff:** [similar](https://crates.io/crates/similar)
- **File watching:** [notify](https://crates.io/crates/notify)

The code is split into layers following Clean Architecture: infrastructure, then application, then domain. See [CLAUDE.md](CLAUDE.md) for details.

## License

MIT. See [LICENSE](LICENSE).
