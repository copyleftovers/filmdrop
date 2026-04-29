# Codebase State — filmdrop

Last updated: 2026-04-29

## Project identity

- **Name**: filmdrop
- **Repository**: https://github.com/copyleftovers/filmdrop
  (transferred from `ryzhakar/gallery-rs` in 2026)
- **Crates.io**: all three crates are published under the `copyleftovers` org token
- **License**: MIT

## Workspace structure

Three-crate Cargo workspace (`Cargo.toml` at repo root). No database; all persistent state lives in S3.

| Crate | Binary name | Role |
|---|---|---|
| `filmdrop-core` | (library) | S3 client wrapper (`S3Client`), `AlbumManifest` types, shared error types |
| `filmdrop-cli` | `filmdrop` | Collects JPEG paths, hashes files (rayon), deduplicates, processes three image tiers, uploads to S3, writes manifest |
| `filmdrop-web` | `filmdrop-web` | Axum web server; reads S3 manifests, generates presigned URLs, serves server-rendered HTML gallery pages and ZIP downloads |

Source paths:
- `/Users/ryzhakar/pp/gallery-rs/filmdrop-core/`
- `/Users/ryzhakar/pp/gallery-rs/filmdrop-cli/`
- `/Users/ryzhakar/pp/gallery-rs/filmdrop-web/`

## Current versions

- **Workspace version** (all three crates share it via `version.workspace = true`): **0.2.2**
- Latest tag on `main`: `v0.2.1` (release-plz PR `81053c4` merged 2026-04-28); `0.2.2` is on `main` but not yet tagged
- `filmdrop-core` dependency is pinned explicitly in the other two crates:
  - `filmdrop-cli/Cargo.toml`: `filmdrop-core = { path = "../filmdrop-core", version = "0.2.2" }`
  - `filmdrop-web/Cargo.toml`: `filmdrop-core = { path = "../filmdrop-core", version = "0.2.2" }`
- Rust toolchain pinned in `rust-toolchain.toml`: channel `1.91.1`, components `clippy` + `rustfmt`

## Build and test commands

### justfile recipes (preferred for development)

```
just check     # cargo check --workspace
just fmt       # cargo fmt --all
just clippy    # cargo clippy --workspace
just test      # cargo test --workspace
just build     # cargo build --release
just run       # sources .env then cargo run --bin filmdrop-web
just install   # cargo install --path filmdrop-cli
just upload "Album Name" /path/to/photos/
```

### Raw cargo equivalents (used in CI)

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo build --workspace
cargo test --workspace
cargo build --release
```

### CI pipeline (`.github/workflows/ci.yml`)

Runs on every push and PR to `main`. Steps in the `validate` job:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo build --workspace`
4. `cargo test --workspace`

The `create-release-pr` job runs only on `push` to `main` when `github.repository_owner == 'copyleftovers'` and calls `release-plz release-pr`.

### Publish pipeline (`.github/workflows/publish.yml`)

Triggers on merged PRs to `main` when `github.repository_owner == 'copyleftovers'`. Calls `release-plz release` (publishes to crates.io and creates git tags). cargo-dist then builds platform binaries from those tags.

## Status snapshots

Taken 2026-04-29 on commit `d58b8ee`:

**`cargo build --workspace 2>&1 | tail -2`**
```
   Compiling filmdrop-cli v0.2.2 (/Users/ryzhakar/pp/gallery-rs/filmdrop-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.48s
```

**`cargo clippy --workspace 2>&1 | tail -3`**
```
    Checking filmdrop-web v0.2.2 (/Users/ryzhakar/pp/gallery-rs/filmdrop-web)
    Checking filmdrop-cli v0.2.2 (/Users/ryzhakar/pp/gallery-rs/filmdrop-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.19s
```

Both pass cleanly. No warnings.

## Active features

### Image pipeline (CLI)
- Three-tier processing per photo: thumbnail (400 px), preview (2048 px), original (untouched)
- Parallel hashing via rayon; deduplication by SHA-256 content hash
- Deterministic album ID: SHA-256 of sorted input file paths, truncated to 16 hex chars
- Resumable uploads: re-uploading the same file set skips already-uploaded images
- S3 `Expires` metadata on all objects; images expire 1 hour after the manifest

### Web server
- Axum server on `0.0.0.0:3000` (override with `PORT`)
- Server-rendered HTML — no JS build step, no templating engine; HTML assembled in `handlers.rs` via `format!` strings
- 7-day presigned URLs embedded in each gallery page
- Progressive image loading: thumbnails in grid, previews in lightbox, originals as upgrade
- Mobile zoom viewer: pinch-to-zoom and double-tap to 100% native resolution
- Skeleton loading with aspect-ratio-correct placeholders (CSS design tokens via `:root` custom properties)
- Per-photo download overlays
- Bulk ZIP download (`GET /api/album/:album_id/download`): streams ZIP of all originals using concurrency-limited parallel S3 fetches (semaphore)
- Image proxy (`GET /api/album/:album_id/image/*path`): proxies S3 objects through the server; used as fallback and for forced-download `Content-Disposition: attachment`
- Returns HTTP 404 (not 500) for corrupt or missing manifests

### Distribution
- Pre-built binaries via cargo-dist (v0.31.0) for:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-unknown-linux-gnu`
  - `x86_64-pc-windows-msvc`
- Shell and PowerShell installers generated by cargo-dist
- `[package.metadata.binstall]` in `filmdrop-cli/Cargo.toml` bridges the tag/asset naming mismatch so `cargo-binstall filmdrop-cli` resolves correctly
- Docker image: multi-stage build with cargo-chef for dependency caching; final image is `gcr.io/distroless/cc-debian12:nonroot`

### S3 compatibility
- AWS S3 (default)
- Custom S3 endpoints (Backblaze B2, MinIO, DigitalOcean Spaces): set `AWS_ENDPOINT_URL`; client enables path-style addressing automatically

## Known limitations

See `deferred_items.md` for full detail. Summary:

1. **Memory pipeline**: all processed images are held in memory before upload (no streaming to S3)
2. **In-memory ZIP**: bulk download buffers the ZIP in memory rather than true streaming
3. **Orphan S3 objects on partial upload**: if the CLI crashes mid-run, already-uploaded image objects are not cleaned up
4. **Serial S3 deletes**: `delete_prefix` uses sequential `DeleteObject` calls instead of the batch `DeleteObjects` API
5. **Schema evolution**: `AlbumManifest` fields lack `#[serde(default)]`, so adding new fields will break deserialization of existing manifests
6. **Album ID hash collision**: 64-bit truncation of SHA-256 is theoretically collidable (no practical concern at current scale)
7. **JPEG only**: `image_processor.rs` rejects all non-JPEG inputs at upload time
8. **JPEG quality constant unused**: `JPEG_QUALITY` is defined but the `image` crate encodes at default quality (marked TODO in source)

## Next actions for a fresh session

1. Check `docs/` for any spec files dropped since this snapshot (mobile UI spec was referenced in `CLAUDE.md` as `docs/spec-mobile-ui.md` but the file did not exist at snapshot time)
2. Verify the version on `main` (`0.2.2`) has been tagged and published — compare `git tag -l` against crates.io
3. Pick up deferred items from `deferred_items.md` if now scheduled
4. Run `just test` before any code change to confirm baseline
5. If working on the image pipeline, read `filmdrop-cli/src/image_processor.rs` and the in-memory buffer pattern before touching upload logic
