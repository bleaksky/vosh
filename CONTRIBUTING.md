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

Bumping Rust is a deliberate commit of its own. Change `channel` in `rust-toolchain.toml`, then run `cargo fmt --all -- --check` and the `cargo clippy` command under Checks on the new version. A newer clippy often brings new lints. Fix what it reports in the same commit so CI stays green.

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

## Checks

CI runs all of these. Run them in this order before you push. The Rust steps need the built page, so `npm run build` comes before them. `npm run knip` finds unused files, exports and dependencies, and `npm run css:usage` finds classes in `src/styles` that no file uses.

```
npm run format:check
npm run lint
npm run typecheck
npm run knip
npm run css:usage
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

CI runs the page checks on Linux and builds the page once. The Rust checks run on Linux, macOS, and Windows with that page, so none of them builds it again. A cargo test holds package.json, package-lock.json, Cargo.toml and tauri.conf.json to one version. The release workflow runs the same checks before it builds. The pre-commit hook runs prettier on the web files you stage and eslint on the script files alone, and cargo fmt and clippy when you stage Rust.

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
docs(readme): list the telnet options Vosh answers
test(gmcp): cover Char.Vitals payload
chore(ci): cache cargo registry between runs
refactor(parser): split state machine
```

## Plans and Scope

`docs/requirements.md` says what 1.0 must do. `docs/refactor-plan.md` holds the milestone plan, the decisions taken, and the status of each phase. Raise a feature that neither names before you build it.

## Tests

A page test sits next to the file it tests as `name.test.ts` or `name.test.tsx`, and `npm test` runs them all with vitest. A Rust unit test lives in a `tests` module or a `tests.rs` beside its code. Tests that span the app crate live in `src-tauri/src/tests/`, and the integration tests there play against the fake MUD in `src-tauri/src/tests/fake_mud/` rather than a live game. A crate keeps its integration tests in its own `tests/` folder.

Protocol parsers need unit tests against byte stream fixtures. Fixtures live in `fixtures/`, and `fixtures/README.md` says what each folder holds and which test reads it. When the page and Rust share a rule, one fixture holds both halves to the same cases.

Run a single test by name.

```
cargo test --workspace <test_name>
npx vitest run <file>
```

## Regenerating Fixtures

Some fixtures are written by the code they check. A test fails when the code no longer matches its fixture. When you meant the change, run the test with its switch set, read the diff, and commit the new fixture with the change that caused it. Leave every switch unset otherwise.

| Switch                         | Fixture                             | Test                                      |
| ------------------------------ | ----------------------------------- | ----------------------------------------- |
| `VOSH_WRITE_CONFIG=1`          | `fixtures/config/`                  | `src-tauri/src/tests/config_golden.rs`    |
| `VOSH_WRITE_IPC_NAMES=1`       | `fixtures/ipc/names.txt`            | `src-tauri/src/tests/ipc_contract.rs`     |
| `VOSH_WRITE_WIRE=1`            | `fixtures/prompt/aabahran/wire/`    | `crates/prompt/tests/wire.rs`             |
| `VOSH_WRITE_PINNED_SPLITS=1`   | `fixtures/prompt/aabahran/pinned/`  | `src-tauri/src/session/tests/show.rs`     |
| `VOSH_WRITE_PREVIEW_SPLITS=1`  | `fixtures/prompt/aabahran/preview/` | `src-tauri/src/session/tests/preview.rs`  |
| `VOSH_WRITE_POINTER_CASES=1`   | `fixtures/prompt/aabahran/pointer/` | `src-tauri/src/session/tests/pointer.rs`  |
| `VOSH_WRITE_COLLAPSE_SPLITS=1` | `fixtures/collapse/`                | `src-tauri/src/session/tests/collapse.rs` |

For example, `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire`.

A config golden changes only in a commit tied to a numbered bug or a decision, and the files in `fixtures/config/old/` never change.

## Writing Style

User-visible prose follows a strict style. README, CONTRIBUTING, settings labels, error messages, and commit messages all qualify.

- Active voice. Address the user as "you".
- No dashes of any kind in prose, no semicolons, no colons in body sentences, no asterisks for emphasis, no emojis.
- Direct and concise. Vary sentence length for rhythm.
- Concrete and specific over abstract. Definitive statements over conditionals.
- No filler phrases.

Code comments are for developers and follow the same style where it makes sense, but stay rare. Names should carry the meaning. Comment only when the why is non obvious.

## Help and Release Notes

`HELP.md` is the one copy of the in-app help, and the Help window reads it when Vosh is built. A change that alters what you see or do updates the help topic that describes it in the same commit. A new topic needs an `<!-- id: section.topic -->` line under its heading, and the build stops if one is missing or used twice. `fixtures/links/help-topics.json` lists every topic id in rail order, so a new or moved topic updates that file too.

`CHANGES.md` holds the release notes, newest first. Each release adds its entry in the same commits as the version bump, and nothing else touches the file. An entry opens with one summary line, then gives one change per bullet in words a player knows, with no commit hashes, file names or internal names.

## Reporting Bugs

Open an issue with the smallest reproducer you can manage. Attach the captured byte stream when the bug touches a parser. Note the OS, the Tauri version, and the MUD you connected to.
