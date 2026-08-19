# gitr

A Git client for macOS, written in Rust on [gpui](https://github.com/zed-industries/zed)
and [gpui-component](https://github.com/longbridge/gpui-component).

It replaces [GitX](https://github.com/rowanj/gitx), whose network operations crash on a
modal-sheet assertion and whose subprocess `git` inherits a truncated GUI `PATH`.

## Status

Milestone 1 — read and visualise. Not yet usable.

## Requirements

- macOS 15 or later
- Rust 1.97.1 (pinned by `rust-toolchain.toml`)

## Installing

This repository is private, so the fetch has to go through your own credentials:

```sh
cargo install --git ssh://git@github.com/LeadcodeDev/gitr \
  --config net.git-fetch-with-cli=true
```

`net.git-fetch-with-cli` is not optional here. Cargo fetches through its bundled libgit2,
which reads neither `~/.ssh/config` nor your SSH agent, and fails with `no authentication
methods succeeded`; the flag delegates the fetch to the `git` binary, which has both. The
HTTPS form fails for the same class of reason, with no credential helper to draw on. Were
this repository public, plain `cargo install --git https://github.com/LeadcodeDev/gitr`
would work with no configuration at all.

Either way it installs one command, `gitr`:

```sh
gitr              # open the repository containing the working directory
gitr ~/code/foo   # open a repository by path
```

Run from a directory that is not inside a repository, `gitr` reopens the projects it
already knows. Naming a path that is not a repository is an error, because you asked for
that one by name.

gitr is not on crates.io, and cannot be for now. `gpui` is reachable only from git, and
zed's February 2026 split left the version on crates.io frozen and unable to receive the
rest of the framework. `CLAUDE.md` records what changing that would cost.

## Running from a checkout

```sh
cargo run -p gitr_gui
```

The first build compiles roughly 850 crates and takes several minutes. Subsequent
incremental builds are a couple of seconds.

## Layout

| Directory | Package | Role |
|---|---|---|
| `crates/domain` | `gitr-domain` | Entities, value objects, ports. No infrastructure, no gpui. |
| `crates/graph` | `gitr-graph` | Commit graph lane layout. Pure, no I/O. |
| `crates/vcs` | `gitr-vcs` | Git adapters: `gix` for reads, subprocess `git` for patches and mutations. |
| `crates/ui` | `gitr-ui` | Views built on gpui-component. |
| `crates/gitr` | `gitr_gui` | Binary `gitr`, and composition root. |

The packages carry a `gitr-` prefix because crates.io has a single global namespace in
which the short names were taken. The code does not: each library pins its `[lib] name`
back to the short form, so imports read `use domain::…`. Cargo's `-p` flag names packages,
not libraries, so it takes the prefixed form.

`gitr-domain` and `gitr-graph` do not depend on gpui, so `cargo test -p gitr-domain -p
gitr-graph` runs in seconds rather than minutes.

## License

MIT
