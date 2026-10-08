# Contributing

Thanks for helping. Read this whole file before your first commit.

## Prerequisites

- rustup. You do not pick a Rust version yourself. See Rust Version below.
- Node 20 or newer.
- Tauri 2 system prerequisites for your OS. See the Tauri 2 prerequisites page.

On Linux you also need the WebKitGTK and related dev packages. On Debian and Ubuntu these are `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, and `patchelf`.

## Rust Version

`rust-toolchain.toml` at the repo root pins one Rust version along with `clippy` and `rustfmt`. Your machine, CI, and the release builds all read that file, so you lint with the same clippy and rustfmt that CI runs. CI also runs clippy on Linux and Windows, where platform code compiles that a Mac never builds.

rustup installs the pinned version on your first build. Run any `cargo` command in the repo and rustup downloads that version once, then reuses it. Older versions you installed stay on disk until you run `rustup toolchain uninstall` on them.

Bumping Rust is a deliberate commit of its own. Change `channel` in `rust-toolchain.toml`, then run `cargo fmt --all -- --check` and the `cargo clippy` command under Lint and Format on the new version. A newer clippy often brings new lints. Fix what it reports in the same commit so CI stays green.

## First-Time Setup

```
npm install
git config core.hooksPath .githooks
```

The second command points git at the project pre-commit hook so `cargo fmt`, `cargo clippy`, `eslint`, and `prettier` run before every commit.

## Development

Full app, Rust backend plus web view.

```
npm run tauri dev
```

Frontend only.

```
npm run dev
```

## Lint and Format

Run all of these before pushing.

```
npm run format:check
npm run lint
npm run typecheck
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Auto fix what you can.

```
npm run format
npm run lint:fix
cargo fmt --all
```

## Commits

Use Conventional Commits. Single concern per commit. Subject under 72 characters.

```
feat(telnet): negotiate IAC DO TTYPE
fix(ansi): handle truncated CSI sequence
docs(readme): clarify Phase 0 scope
test(gmcp): cover Char.Vitals payload
chore(ci): cache cargo registry between runs
refactor(parser): split state machine
```

## Phased Delivery

The project ships in phases. Each phase ends with a working build, a short demo, and an approval checkpoint. Do not start the next phase before approval lands in the chat. See `prompt.md` for the full phase plan.

If you find yourself wanting to add a feature that is not in the agreed phase scope for the current session, stop and raise it for review. Scope creep across phase boundaries is the easiest way to lose the plot.

## Tests

Protocol parsers (telnet, ANSI, GMCP) need unit tests against captured byte stream fixtures. Drop fixtures into `fixtures/`. Integration tests should run against a fake MUD server fixture rather than a live MUD.

Run a single test by name.

```
cargo test --workspace <test_name>
```

## Writing Style

User-visible prose follows a strict style. README, CONTRIBUTING, settings labels, error messages, and commit messages all qualify.

- Active voice. Address the user as "you".
- No dashes of any kind in prose, no semicolons, no colons in body sentences, no asterisks for emphasis, no emojis.
- Direct and concise. Vary sentence length for rhythm.
- Concrete and specific over abstract. Definitive statements over conditionals.
- No filler phrases.

Code comments are for developers and follow the same style where it makes sense, but stay rare. Names should carry the meaning. Comment only when the why is non obvious.

You edit the in-app help in `HELP.md`, which the Help window reads when Vosh is built. A new topic needs an `<!-- id: section.topic -->` line under its heading, and the build stops if one is missing or used twice.

## Reporting Bugs

Open an issue with the smallest reproducer you can manage. Attach the captured byte stream when the bug touches a parser. Note the OS, the Tauri version, and the MUD you connected to.
