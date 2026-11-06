#!/usr/bin/env bash
# Archives the release build for $TARGET into dist/ as syntaxeon-<version>-<target>.<ext>
# (tar.gz on Linux and macOS, zip on Windows). The archive holds the binaries (the CLI and
# the language server), LICENSE, README.md, the man page and the shell completions, all at
# the paths below.
set -euo pipefail

: "${TARGET:?set TARGET to a Rust target triple}"
VERSION="${VERSION:-$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')}"

case "$TARGET" in
  x86_64-unknown-linux-gnu|aarch64-apple-darwin) ext="" ;;
  x86_64-pc-windows-msvc) ext=".exe" ;;
  *) echo "unsupported TARGET: $TARGET" >&2; exit 1 ;;
esac

cargo build --release --locked --target "$TARGET" -p syntaxeon-cli -p syntaxeon-lsp
bin="target/$TARGET/release/syntaxeon$ext"
lsp="target/$TARGET/release/syntaxeon-lsp$ext"

name="syntaxeon-$VERSION-$TARGET"
stage="dist/$name"
rm -rf "$stage"
mkdir -p "$stage/man" "$stage/completions"
cp "$bin" "$lsp" LICENSE README.md "$stage/"
"$bin" man > "$stage/man/syntaxeon.1"
"$bin" completions bash > "$stage/completions/syntaxeon.bash"
"$bin" completions zsh > "$stage/completions/_syntaxeon"
"$bin" completions fish > "$stage/completions/syntaxeon.fish"
"$bin" completions powershell > "$stage/completions/syntaxeon.ps1"

# The archive's single top-level folder is $name/.
case "$TARGET" in
  *-windows-*) (cd dist && 7z a -tzip -bso0 "$name.zip" "$name") ;;
  *) tar -C dist -czf "dist/$name.tar.gz" "$name" ;;
esac
rm -rf "$stage"
ls -l dist
