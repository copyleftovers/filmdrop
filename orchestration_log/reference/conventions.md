# Conventions — filmdrop

Last updated: 2026-06-14

Patterns derived from this project's session history. Many entries document traps that have already been triggered; read them before starting a new session.

---

## Release workflow

The release pipeline is fully automated but has a specific shape:

1. **Push to `main`** → CI `validate` job runs (fmt, clippy, build, test)
2. **CI passes** → `create-release-pr` job calls `release-plz release-pr`; a PR titled "chore: release vX.Y.Z" is created or updated
3. **Merge the release PR** → `publish.yml` triggers on `pull_request.types: [closed]` with `merged == true`
4. **`publish.yml`** → calls `release-plz release`, which:
   - Publishes updated crates to crates.io
   - Creates git tags (`filmdrop-cli-vX.Y.Z`, `filmdrop-web-vX.Y.Z`, `filmdrop-core-vX.Y.Z`)
5. **Tags created** → cargo-dist (v0.31.0) builds platform binaries and attaches them to the GitHub release

**Secrets required** (set in the `copyleftovers` org or repo):
- `RELEASE_PLZ_TOKEN` — GitHub PAT with `contents: write` and `pull-requests: write`
- `CARGO_REGISTRY_TOKEN` — crates.io API token

**Key guard**: both `ci.yml` and `publish.yml` gate the release steps on `github.repository_owner == 'copyleftovers'`. This means:
- Forks will pass CI but will never accidentally publish or create release PRs.
- If the repo is transferred again, these conditions must be updated.

---

## Versioning

- All three crates share a single version via `version.workspace = true` in each crate's `Cargo.toml`.
- The canonical version lives in `[workspace.package]` in the root `Cargo.toml`.
- release-plz reads the conventional commits since the last tag to determine the bump (patch/minor/major).
- **`cargo-semver-checks` may auto-bump to major**: if a public API in `filmdrop-core` changes in a breaking way, the tooling will detect it and force a major version bump even if the commit message says `fix:`.
- To release, just merge to `main` and let the automation run. Do not manually bump versions in `Cargo.toml` — release-plz owns that.
- The `version` alongside `path` in inter-crate dependencies (`filmdrop-core = { path = "../filmdrop-core", version = "0.2.2" }`) must match the workspace version. Update both together when release-plz bumps the version.

---

## Pre-commit hooks

Configured in `.pre-commit-config.yaml` using `doublify/pre-commit-rust` (pinned to `v1.0`).

Hooks that run on every `git commit`:

| Hook | Command |
|---|---|
| `fmt` | `cargo fmt` |
| `cargo-check` | `cargo check` |
| `clippy` | `cargo clippy --fix --allow-staged --allow-dirty --workspace` |
| `trailing-whitespace` | strips trailing whitespace |
| `end-of-file-fixer` | ensures files end with a newline |
| `check-yaml` | validates YAML syntax |
| `check-added-large-files` | blocks accidental binary commits |

**Important**: `clippy` runs with `--fix` and `--allow-dirty`. This means it can modify files you haven't staged. Review the diff after a hook runs before re-committing.

Install hooks on a fresh clone:
```bash
pip install pre-commit
pre-commit install
```

---

## CI guard — `repository_owner` check

Both `.github/workflows/ci.yml` and `.github/workflows/publish.yml` contain:

```yaml
if: github.repository_owner == 'copyleftovers'
```

This guards the `create-release-pr` and `publish-and-tag` jobs. Consequences:
- PRs from forks do not trigger publishing.
- If the repo is ever transferred out of `copyleftovers`, update this string in both files before merging anything.
- Locally, `act` or similar tools that simulate CI will skip these jobs unless you mock the org name.

---

## Rust toolchain

Pinned in `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.91.1"
components = ["clippy", "rustfmt"]
```

- `rustup` will automatically download and activate `1.91.1` when you run any `cargo` command inside the repo.
- CI uses `dtolnay/rust-toolchain@stable` — this resolves to the stable channel at CI run time, which may diverge from `1.91.1` over time. If CI behavior differs from local, suspect a toolchain mismatch and pin CI explicitly.
- The `Dockerfile` uses `rust:1.91.1-slim-bookworm` — keep this in sync with `rust-toolchain.toml` when updating the toolchain.

---

## Worktree CWD trap

Implementer agents that use git worktrees commonly leave the parent shell's working directory pointing at the worktree path rather than the repo root. This causes subsequent `git` operations to operate on the worktree's branch silently.

**Rule**: always prefix git commands with the absolute repo root:

```bash
cd /Users/ryzhakar/pp/gallery-rs && git status
cd /Users/ryzhakar/pp/gallery-rs && git log --oneline -5
```

Or, for single-shot commands from any directory:

```bash
git -C /Users/ryzhakar/pp/gallery-rs log --oneline -5
```

---

## Pre-merge requirement — dual review

Before merging any implementation branch to `main`:

1. **Spec review**: confirm the implementation matches the relevant spec in `docs/` (e.g., `docs/spec-mobile-ui.md`). Deviations must be noted and either corrected or recorded in `deferred_items.md`.
2. **Quality review**: confirm `just clippy` and `just test` pass with zero warnings. CI enforces `-- -D warnings` (warnings-as-errors).

Both reviews are required. Skipping either has historically led to regressions that blocked the release-plz pipeline.

---

## cargo-dist + cargo-binstall naming mismatch

cargo-dist produces release assets named `{binary}-{target}.tar.xz`, e.g. `filmdrop-aarch64-apple-darwin.tar.xz`. `cargo-binstall` by default looks for assets named after the **package** (`filmdrop-cli-aarch64-apple-darwin.tar.xz`).

The fix is in `filmdrop-cli/Cargo.toml`:

```toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/{ name }-v{ version }/{ name }-{ target }.tar.xz"
bin-dir = "{ name }-{ target }/{ bin }{ binary-ext }"
pkg-fmt = "txz"
```

Without this block, `cargo binstall filmdrop-cli` fails with a 404. Do not remove or rename these keys. If the cargo-dist asset naming changes (e.g., upgrading cargo-dist), update `pkg-url` and `bin-dir` to match.

---

## Docker image — cargo-binstall workflow

The Dockerfile uses cargo-binstall to download prebuilt filmdrop-web binaries from GitHub Releases. It does NOT compile from source.

**Build:**
```bash
docker build -t filmdrop-web .                                    # latest version
docker build --build-arg FILMDROP_WEB_VERSION=0.2.3 -t filmdrop-web .  # pinned version
```

**Key constraints:**
- Runtime base is `debian:bookworm-slim` (not distroless) — prebuilt binaries have dynamic deps (liblzma etc.) that distroless lacks
- `ca-certificates` must be installed in the runtime stage — the binary makes TLS calls to S3
- cargo-binstall installs to `/root/.cargo/bin`, which is not on PATH — use the full path in RUN commands
- `FILMDROP_WEB_VERSION` defaults to `latest`; cargo-binstall resolves it from crates.io metadata
- The binstall metadata in Cargo.toml must match cargo-dist's asset naming (`{name}-{target}.tar.xz`, `.zip` for Windows)

**Failure history:** distroless/cc-debian12 caused liblzma.so.5 missing; nim65s/cargo-binstall image was 1.8 GB (too heavy for remote); PATH not set after bootstrap install.

---

## Secrets handling — .env file

The `.env` file at repo root contains S3 credentials. When passing env vars to Docker or subprocesses, source opaquely and forward by name:

```bash
set -a && source .env && set +a && docker run -e GALLERY_BUCKET -e AWS_ACCESS_KEY_ID ...
```

Never read, cat, or log the .env file contents. Treat as secret from all agents including the orchestrator.

---

## Crates.io publishing — path deps need `version`

Crates.io does not allow publishing a crate with a `path`-only dependency. Both `filmdrop-cli` and `filmdrop-web` declare their dependency on `filmdrop-core` as:

```toml
filmdrop-core = { path = "../filmdrop-core", version = "0.2.2" }
```

The `version` field is mandatory for publishing. When the workspace version is bumped (by release-plz), update this version string in both `filmdrop-cli/Cargo.toml` and `filmdrop-web/Cargo.toml` to match. release-plz does this automatically, but if you manually edit `Cargo.toml`, you must keep them in sync.

---

## Worktree base SHA discipline

When an implementer agent creates a git worktree, it may branch from an older commit rather than the current `HEAD` of `main`. This can produce a PR whose diff is much larger than expected (it includes other people's merged work as if it were new).

Before merging any branch:

```bash
cd /Users/ryzhakar/pp/gallery-rs && git log --oneline main..HEAD
```

Confirm the commit list matches only the intended changes. If the branch has diverged significantly, rebase onto current `main` before review:

```bash
git rebase origin/main
```

---

## GitHub repo transfer — PAT regeneration

Personal Access Tokens (PATs) are issued to a specific GitHub user or org. When the repository was transferred from `ryzhakar/gallery-rs` to `copyleftovers/filmdrop`:

- The `RELEASE_PLZ_TOKEN` PAT was invalidated because it was scoped to `ryzhakar`.
- A new PAT was generated under the `copyleftovers` org and stored as a repository secret.

**Rule**: after any repo transfer or org rename, regenerate all PATs used in GitHub Actions secrets and update the secret values in the new org/repo settings before expecting CI to pass end-to-end.
