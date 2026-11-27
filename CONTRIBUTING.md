# Contributing to `lume`

## Local development

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins
```

## Building a static Linux `musl` binary

```bash
./build.sh
```

## Release process

1. Bump `version` in the workspace `Cargo.toml` and update `CHANGELOG.md`.
2. Commit and push to `main`.
3. Tag `vX.Y.Z` and push the tag — GitHub Actions automatically builds and publishes static binaries for Linux (`x86_64` / `aarch64` musl), macOS (`arm64` / `x86_64`), and Windows (`x86_64` static CRT) with `SHA256SUMS.txt`.
