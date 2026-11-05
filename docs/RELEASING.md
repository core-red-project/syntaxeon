# Releasing

1. Move the `[Unreleased]` entries of [CHANGELOG.md](../CHANGELOG.md) under the new version and date.
2. Set the same version in the root `Cargo.toml`: `[workspace.package] version` and the `version` of every internal entry in `[workspace.dependencies]`. `release.yml` fails unless every crate's version equals the tag.
3. Merge to `main`, then tag the merge commit: `git tag vX.Y.Z && git push origin vX.Y.Z`.

`release.yml` verifies the tag, re-runs `just check` on Linux, macOS and Windows, packages each target, writes `SHA256SUMS`, generates the release notes from Conventional Commits with git-cliff, and creates the release once.

## Assets

`syntaxeon-<version>-<target>.<ext>`, `tar.gz` for Linux and macOS, `zip` for Windows.

| Target | Runner |
|---|---|
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` |
| `aarch64-apple-darwin` | `macos-15` |
| `x86_64-pc-windows-msvc` | `windows-2025` |

Each archive has a single top-level folder `syntaxeon-<version>-<target>/` holding the binary, `LICENSE`, `README.md`, `man/syntaxeon.1` and `completions/` for bash, zsh, fish and PowerShell.

Build one locally with `TARGET=aarch64-apple-darwin VERSION=0.0.1 just package` (needs `jq`).
