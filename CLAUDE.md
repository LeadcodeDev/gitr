# CLAUDE.md

Guidance for Claude Code working in this repository.

gitr is a macOS Git client built on gpui and gpui-component. It replaces GitX.

## Where things live

| Path | Owns |
|---|---|
| `crates/domain/` | Entities, value objects, ports. Depends on nothing but `thiserror`. |
| `crates/graph/` | Commit graph lane layout. Pure function over `domain` types, no I/O. |
| `crates/vcs/` | Git adapters. `gix` for structured reads, subprocess `git` for patches. |
| `crates/ui/` | Views on gpui-component. |
| `crates/gitr/` | Binary. Composition root — the only place adapters are wired to ports. |

Dependencies point inward. `domain` and `graph` must never depend on `gix`, on a
subprocess, or on gpui. That is not style: it is what keeps `cargo test -p domain -p graph`
at a few seconds instead of a few minutes.

## Commands that actually run

```sh
cargo test -p domain -p graph     # fast loop, no gpui in the tree
cargo test --workspace            # exit check
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p gitr                 # launch the app
```

Cargo holds one lock per target directory, so `clippy` and `test` do not run
concurrently in the same checkout. Batch them only across distinct `CARGO_TARGET_DIR`s.

## Non-obvious constraints

**Do not pin `gpui` to a `rev`.** gpui-component declares `gpui` against the default
branch of `zed-industries/zed`, and Cargo unifies two git sources only when the
reference matches exactly. A `rev` here produces two copies of the crate in the graph,
and traits from one do not apply to the other. The failure looks unrelated — *"no method
named `bg` found for struct `Root`"*. `Cargo.lock` is what pins the revisions.

**The gpui-component website documentation is out of date in places.** Verified wrong at
the time of writing: `h_resizable(id, state)` is really `h_resizable(id).with_state(&state)`;
`Root::render_dialog_layer(cx)` really takes `(window, cx)`; `Root::new(...).bg(...)` does not
exist; the `popup_menu` module is not public — it is `gpui_component::menu`. Read
`crates/ui/src/` and `crates/story/src/stories/` in the gpui-component checkout instead.

**`Root` must be the first child of the window**, or dialogs, sheets and notifications break.

**`theme::init` forces light mode** at startup. Follow the OS appearance explicitly if wanted.

**Never call `TableState::dump`** — it materialises the whole table (gpui-component
issue #2754). On a large history that is an out-of-memory crash.

**`TableDelegate::visible_rows_changed` runs every scroll frame.** Keep it allocation-free.

**libgit2 cannot produce `--topo-order`.** Its `GIT_SORT_TOPOLOGICAL` yields `--date-order`.
Commit order dominates graph readability — measured on `rust-lang/cargo`, 23 789 commits:
date order gives 258 lanes, topological order with date priority gives 20. Read history
through `gix_traverse::commit::topo::Builder`, which is not exposed on gix's high-level
`rev_walk` builder.

**Network and mutating Git operations go through subprocess `git`, never a library.**
libssh2 does not read `~/.ssh/config`, so `Host` aliases and `ProxyCommand` silently break.
The subprocess must inherit a `PATH` resolved from the user's login shell — a macOS GUI
app gets a truncated one, which is why GitX fails with `git: 'credential-osxkeychain' is
not a git command`.

**No blocking modal for long operations.** GitX crashes on
`assert(currentModalSheet == nil)` when two network operations overlap. Long work runs on
`cx.background_executor()` and reports through the status bar and notifications.

## House rules

- No `async_trait`, ever. gpui has its own executor; ports are synchronous and blocking,
  called from `cx.background_executor().spawn()`. tokio is not in the tree.
- Prefer enum dispatch to `Box<dyn Trait>` wherever the set of implementations is closed.
- Workspace crate names carry no `gitr-` prefix.
- Commit convention: `<type>(<subject>): <imperative message>`.
- Never add an AI co-author trailer to a commit, issue or pull request.
