# Contributing

gitr is a macOS Git client built on gpui. The build is heavy and a few of the
rules are not guessable from the source, so this page covers what would
otherwise cost you an evening.

## What you need

- macOS 15 or later. gitr is macOS-only, and there is no cross-platform path today.
- Rust 1.97.1. `rust-toolchain.toml` pins it and rustup honours that on its own.

## Building and running

```sh
cargo run -p gitr_gui
```

The first build compiles roughly 850 crates and takes several minutes.
Incremental builds after that are a couple of seconds.

## The test loop

Work against the two crates that never pull in gpui:

```sh
cargo test -p gitr-domain -p gitr-graph
```

Seconds, not minutes. Keep the full set for when you think you are done:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Run those one after another rather than in parallel. Cargo takes a single lock
per target directory, so clippy and test in one checkout only wait on each other.

### `-p` wants the published name

The packages are `gitr-domain`, `gitr-graph`, `gitr-vcs`, `gitr-ui` and
`gitr_gui`, while the code imports them unprefixed — `use domain::…`. So
`cargo test -p domain` fails with *"did not match any packages"* and
`cargo test -p gitr-domain` works. The prefix buys nothing except a name that
was still free on crates.io.

## Which crate may depend on what

| Crate | Package | Depends on |
|---|---|---|
| `crates/domain` | `gitr-domain` | `thiserror`, and nothing else |
| `crates/graph` | `gitr-graph` | `domain` |
| `crates/vcs` | `gitr-vcs` | `domain`, `gix`, `notify` |
| `crates/ui` | `gitr-ui` | `domain`, `graph`, `vcs`, gpui |
| `crates/gitr` | `gitr_gui` | everything — the composition root, and the only place adapters are wired to ports |

`domain` and `graph` must never reach for gix, a subprocess, or gpui. That is not
a matter of taste: it is what keeps their tests at a few seconds instead of a few
minutes, and that number is what makes the loop above usable.

`CLAUDE.md` holds the constraints found the hard way — why gpui must not be
pinned to a revision, why network operations go through subprocess `git` instead
of a library, why the commit graph needs topological order. Read it before
changing anything structural. It is written for an AI assistant and is just as
useful to a person.

## Commits

```
<type>(<subject>): <imperative message>
```

Type from `feat`, `fix`, `refactor`, `chore`, `docs`, `test`, `perf`, `ci`.
Subject is the module or bounded context. The message opens on a lowercase
imperative verb — `fix(graph): keep a branch one colour to the junction`.

Branch names say intent: `feat/…`, `fix/…`, `docs/…`, `chore/…`.

One commit is one logical change, and the message explains *why* rather than
what. `git log` is the most-read documentation in this project.

Do not add an AI co-author trailer to a commit, an issue or a pull request.

## Pull requests

Open against `main`.

Put in the description what the diff cannot carry: the approach you tried and
abandoned, the alternative you rejected and the evidence that settled it. A
reviewer six months out has only that text.

Small and reviewable in one sitting beats complete. A pull request that cannot
be read in one sitting usually wants to be two.

## Licensing of contributions

gitr is [Apache 2.0](LICENSE). Under section 5 of that licence, what you submit
is contributed under those same terms unless you state otherwise. There is no
CLA to sign.

Who maintains this, and what to expect from a single-maintainer project:
[`MAINTAINERS.md`](MAINTAINERS.md).
