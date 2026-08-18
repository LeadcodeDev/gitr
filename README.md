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

## Running

```sh
cargo run -p gitr
```

The first build compiles roughly 850 crates and takes several minutes. Subsequent
incremental builds are a couple of seconds.

## Layout

| Crate | Role |
|---|---|
| `domain` | Entities, value objects, ports. No infrastructure, no gpui. |
| `graph` | Commit graph lane layout. Pure, no I/O. |
| `vcs` | Git adapters: `gix` for reads, subprocess `git` for patches and mutations. |
| `ui` | Views built on gpui-component. |
| `gitr` | Binary and composition root. |

`domain` and `graph` do not depend on gpui, so `cargo test -p domain -p graph` runs in
seconds rather than minutes.

## License

MIT
