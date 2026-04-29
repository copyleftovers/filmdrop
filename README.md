# filmdrop

Ephemeral photo sharing for film photographers -- drop your scans, share a link, let it expire.

## Features

- CLI upload with automatic three-tier processing (thumbnail 400px, preview 2048px, original)
- Self-destructing galleries (configurable expiration, default 7 days)
- Zero-dependency web viewer (server-rendered, no JS build step)
- Grain-preserving pipeline (originals never re-encoded)
- Mobile zoom viewer (pinch-to-zoom, double-tap to 100% native resolution)
- Bulk ZIP download
- Per-photo download overlays
- Skeleton loading with aspect-ratio placeholders
- Deterministic album IDs (re-upload resumes, deduplication by content hash)
- BYO S3 (AWS, Backblaze B2, MinIO, DigitalOcean Spaces)

## Quick start

```bash
cargo install --git https://github.com/copyleftovers/filmdrop filmdrop-cli
cargo install --git https://github.com/copyleftovers/filmdrop filmdrop-web

export GALLERY_BUCKET="my-bucket"
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."
export AWS_REGION="us-east-1"

# Upload a gallery
filmdrop upload --name "Portra 400 / June" /path/to/scans/

# Start the web server
filmdrop-web
# -> http://localhost:3000/gallery/<album-id>
```

## CLI usage

```bash
# Upload a directory of JPEGs
filmdrop upload --name "Album Name" /path/to/photos/

# Upload specific files
filmdrop upload --name "Album Name" scan01.jpg scan02.jpg

# Delete an album
filmdrop delete ALBUM-ID
```

The CLI processes each image into three tiers (thumbnail, preview, original), uploads everything to S3, and prints the album ID.

## Web server

```bash
filmdrop-web
```

Listens on `0.0.0.0:3000` by default (override with `PORT`).

### Routes

| Route | Description |
|---|---|
| `GET /` | Landing page |
| `GET /gallery/:album_id` | Gallery viewer (server-rendered HTML) |
| `GET /api/album/:album_id/manifest` | Album manifest JSON |
| `GET /api/album/:album_id/image/*path` | Image proxy (with `Content-Disposition: attachment`) |
| `GET /api/album/:album_id/download` | Bulk ZIP download of all originals |

## Configuration

| Variable | Required | Default | Description |
|---|---|---|---|
| `GALLERY_BUCKET` | yes | -- | S3 bucket name |
| `AWS_ACCESS_KEY_ID` | yes | -- | AWS access key |
| `AWS_SECRET_ACCESS_KEY` | yes | -- | AWS secret key |
| `AWS_REGION` | no | `us-east-1` | AWS region |
| `AWS_ENDPOINT_URL` | no | -- | Custom S3 endpoint (enables path-style addressing) |
| `PORT` | no | `3000` | Web server port |

## Development

This project uses [just](https://github.com/casey/just) for development tasks. For standard Rust development, use `cargo` commands directly.

```bash
# Install just (if not already installed)
cargo install just

# Standard workflow
cargo fmt --all
cargo clippy --workspace
cargo test --workspace

# Just recipes
just check    # cargo check --workspace
just fmt      # cargo fmt --all
just clippy   # cargo clippy --workspace
just test     # cargo test --workspace
just build    # cargo build --release
just run      # run web server (sources .env)
just install  # cargo install --path filmdrop-cli
```

Pre-commit hooks enforce formatting, clippy, and cargo-check via [pre-commit](https://pre-commit.com/).

See `just --list` for all available commands.

## Deployment

### Docker

The included `Dockerfile` uses a multi-stage build with cargo-chef for dependency caching:

1. Dependencies are cached in a separate layer via `cargo-chef`
2. Release build with LTO, single codegen unit, and symbol stripping
3. Final image is `gcr.io/distroless/cc-debian12:nonroot` -- minimal and runs as non-root

```bash
docker build -t filmdrop-web .
docker run -p 3000:3000 \
  -e GALLERY_BUCKET=my-bucket \
  -e AWS_ACCESS_KEY_ID=... \
  -e AWS_SECRET_ACCESS_KEY=... \
  -e AWS_REGION=us-east-1 \
  filmdrop-web
```

### CI/CD

GitHub Actions runs fmt, clippy, build, and tests on every push and PR. On merge to `main`, [release-plz](https://release-plz.ieni.dev/) creates release PRs, and [cargo-dist](https://opensource.axo.dev/cargo-dist/) builds binaries for macOS (aarch64, x86_64), Linux (aarch64, x86_64), and Windows (x86_64).

## Architecture

Three-crate Cargo workspace. No database -- all state lives in S3.

```
filmdrop-core/   # Shared library: S3 client wrapper + AlbumManifest types
filmdrop-cli/    # Binary (filmdrop): image processing + upload
filmdrop-web/    # Binary (filmdrop-web): Axum web server
```

The CLI hashes input file paths to produce a deterministic album ID. Re-uploading the same set of files resumes the existing album and skips already-uploaded images. Albums and images have S3 `Expires` metadata set so they self-destruct without any cleanup job.

## License

MIT
