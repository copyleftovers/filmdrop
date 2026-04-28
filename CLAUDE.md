# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build all crates
cargo build --release

# Run tests
cargo test --workspace

# Run a single test
cargo test --package <filmdrop-core|filmdrop-cli|filmdrop-web> <test_name>

# Format and lint
cargo fmt --all
cargo clippy --workspace

# Run the web server (requires env vars or .env file)
GALLERY_BUCKET=my-bucket cargo run --bin filmdrop-web

# Upload images via CLI
GALLERY_BUCKET=my-bucket cargo run --bin filmdrop upload --name "Album Name" /path/to/photos/

# Delete an album
GALLERY_BUCKET=my-bucket cargo run --bin filmdrop delete ALBUM-UUID

# Just recipes (preferred for development)
just check     # cargo check --workspace
just fmt       # cargo fmt --all
just clippy    # cargo clippy --workspace
just test      # cargo test --workspace
just build     # cargo build --release
just run       # run web server (sources .env automatically)
just install   # cargo install --path filmdrop-cli
just upload "Album Name" /path/to/photos/
```

Required env vars for all S3 operations: `GALLERY_BUCKET`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION`. For non-AWS S3 (MinIO etc.), also set `AWS_ENDPOINT_URL` -- the S3 client automatically enables path-style addressing when this is set.

## Active Specs

- `docs/spec-mobile-ui.md` -- Mobile UI improvements: zoom viewer, skeleton loading, download discoverability, bulk ZIP download

## Architecture

Three-crate Cargo workspace with no database: all persistent state lives in S3.

```
filmdrop-core/   # Shared library: S3Client wrapper + AlbumManifest types
filmdrop-cli/    # Binary (filmdrop): processes and uploads images; outputs album ID
filmdrop-web/    # Binary (filmdrop-web): Axum web server; reads S3 and serves HTML
```

### Data flow

1. **CLI upload**: Collects JPEG paths -> hashes files in parallel (rayon) -> checks S3 for existing album by deterministic album ID (SHA256 of sorted paths, truncated to 16 chars) -> processes new images (thumbnail 400px, preview 2048px, original unchanged) -> uploads to S3 with TTL expiration -> writes `{album_id}/manifest.json`

2. **Web server**: On `GET /gallery/:album_id` -> downloads `{album_id}/manifest.json` from S3 -> generates 7-day presigned URLs for all three image tiers per photo -> inlines everything into a single server-rendered HTML page with embedded JS/CSS. No templating engine; HTML is assembled via Rust `format!` strings in `handlers.rs`.

3. **Image proxy**: `GET /api/album/:album_id/image/*path` proxies S3 objects through the server (used as fallback when presigned URLs are unavailable, and for the download endpoint which sets `Content-Disposition: attachment`).

4. **Bulk download**: `GET /api/album/:album_id/download` streams a ZIP archive of all original images, fetched from S3 with concurrency-limited parallel downloads (semaphore).

### Key design decisions

- **Album identity**: Album ID is deterministic from the set of input file paths (not UUIDs). Re-uploading the same file set resumes the existing album and skips already-uploaded images (deduplication by SHA256 content hash).
- **Three image tiers**: thumbnail (grid), preview (lightbox initial load), original (full-res download and lightbox upgrade). The frontend progressively upgrades displayed images: thumbnails -> previews in the grid, then previews -> originals in the lightbox.
- **Expiration**: CLI sets S3 object `Expires` metadata. Images expire 1 hour after the manifest to ensure the manifest always outlives the images it references.
- **Only JPEG supported**: `image_processor.rs` rejects non-JPEG files. The `JPEG_QUALITY` constant in `encode_jpeg` is currently unused (the `image` crate encodes at default quality); marked with a TODO.
- **Frontend is entirely server-rendered**: The gallery HTML including all presigned URLs and image metadata JSON is generated on each request in `generate_gallery_html()`. There is no static asset pipeline or frontend build step.
- **CSS design tokens**: `:root` custom properties provide unified glass-morphism controls (blur, opacity, border-radius) across all UI components (lightbox, download overlays, skeleton loaders).
- **Pre-commit hooks**: `pre-commit` runs `cargo fmt`, `cargo clippy --fix`, and `cargo check` on every commit via the `doublify/pre-commit-rust` hooks. Also enforces trailing-whitespace, end-of-file-fixer, check-yaml, and check-added-large-files.
- **Release automation**: `release-plz` creates release PRs on merge to `main`. `cargo-dist` (v0.31.0) builds binaries for macOS (aarch64, x86_64), Linux (aarch64, x86_64), and Windows (x86_64) with shell and PowerShell installers.
- **Rust toolchain pinned**: `rust-toolchain.toml` pins the channel to `1.91.1` with `clippy` and `rustfmt` components.
