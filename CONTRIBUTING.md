# Contributing to visr

Thanks for helping out. This page is the short version for humans. [AGENTS.md](AGENTS.md) has the
full rules and is the source of truth.

## Getting started

You need:

- The Rust version pinned in `rust-toolchain.toml`. `rustup` installs it on the first `cargo` call.
- herdr-core, the program that owns the terminals. Visr offers to install it
  on first run if it is missing.

Build and run, then run the same checks as CI before you open a PR:

```sh
cargo run -p visr
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Branches

- `main` holds released code only. Every merge into `main` is a release.
- `dev` is the integration branch and the default one.
- Start your branch from `dev` and name it `feat/…`, `fix/…`, `chore/…`, `docs/…` or `ci/…`.
- Nothing is pushed directly to `main` or `dev`. Everything goes through a PR.

## Commits

We use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`,
imperative, lowercase, no trailing period, header up to 72 characters.

- Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`,
  `revert`.
- Scopes: the crate (`app`, `ui`, `theme`, `app-state`, `term`, `config`, `hc-client`, `hc-proto`)
  or `release`. `ci:`, `build:` and `docs:` take no scope.
- Add a body only when the header doesn't make the reason clear. Breaking changes get `!` and a
  `BREAKING CHANGE:` footer.
- Each commit builds and passes the checks on its own, and does one thing.

## Pull requests

- Target `dev`. The title follows the commit rules above, because the squash merge turns it into
  the commit on `dev`.
- The description has one `## Summary` heading and a list of lowercase, imperative, one-line
  bullets that describe behavior rather than files.
- For visual changes, show before and after screenshots, in a dark and a light theme.
- State which platforms you ran it on (macOS, Windows).

## Changelog

A change someone using visr can notice adds one short line to `## [Unreleased]` in
`CHANGELOG.md`, in the same commit, under `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed` or
`Security`. Say what a person can now do, in their words. Refactors and CI changes get no entry.

## Dependencies and licenses

Please discuss a new dependency in an issue before adding it. Prefer what is already in the tree.
Every dependency, font and icon pack must be under a license compatible with GPL-3.0-or-later: MIT,
Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0 or OFL-1.1 (fonts only). No GPL, LGPL, AGPL or
MPL code.

## Releases

Only the maintainer cuts releases. From `dev`, a `release/vX.Y.Z` branch bumps the version and
moves `Unreleased` in `CHANGELOG.md` to the new version. It is merged into `main` with a merge
commit, the maintainer tags it, and the tag push builds and publishes the release. Then `main` is
merged back into `dev`.

## License

Licensed under the GNU General Public License, version 3 or (at your option) any later version
([LICENSE](LICENSE) or <https://www.gnu.org/licenses/gpl-3.0.html>).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you shall be licensed as above, without any additional terms or conditions.
