# Deferred Items — filmdrop

Last updated: 2026-04-29

These are known issues, architectural weaknesses, and ideas surfaced during deep code probes that were deliberately not fixed in the sessions that found them. Each entry records severity, why it was deferred, and when to revisit.

---

## 1. Memory pipeline — all images buffered before upload

**Location**: `filmdrop-cli/src/` — image processing and upload flow

**What it is**: The CLI reads each source JPEG, processes it into three tiers (thumbnail, preview, original), and holds all three encoded buffers in memory before beginning any S3 `PutObject` calls. For large batches or high-megapixel originals, peak memory can be substantial.

**Severity**: Medium. Typical film scans (24–36 exposures, 20–80 MB originals) fit comfortably in available RAM on any modern workstation. The issue only surfaces for extremely large batches or very high-resolution scans.

**Why deferred**: Fixing it requires either streaming encoded output directly into an async S3 upload (complex with the `image` crate's synchronous encoder) or switching to a producer-consumer pipeline with bounded channels. Neither is a one-line change, and the current behavior is safe for the target use case.

**When to revisit**: If users report OOM panics or if the project adds support for medium-format raw scans (files routinely >100 MB each).

---

## 2. In-memory ZIP for bulk download

**Location**: `filmdrop-web/src/handlers.rs` — the `download_album` handler

**What it is**: `GET /api/album/:album_id/download` fetches all original images from S3 in parallel (behind a semaphore), writes them into a `zip::ZipWriter` backed by an in-memory `Vec<u8>`, and sends the completed buffer as the HTTP response body. A large album (e.g., 36 originals at 50 MB each = 1.8 GB) will fully reside in server RAM before the first byte reaches the client.

**Severity**: Medium-High for large albums; Low for the typical sub-100 MB album. Also affects time-to-first-byte: the client sees nothing until the entire ZIP is assembled.

**Why deferred**: True streaming ZIP (writing compressed data directly into the HTTP body stream) requires either `async-zip` or careful use of `zip`'s `start_file`/write loop across a `tokio::io::DuplexStream`. The ergonomic path is non-trivial and was out of scope for the feature-delivery sessions that built bulk download.

**When to revisit**: Before any public launch with albums > ~200 MB total. Consider `async-zip` crate or chunked transfer encoding with `axum::body::Body::from_stream`.

---

## 3. Orphan S3 objects on partial upload

**Location**: `filmdrop-cli/src/` — upload orchestration

**What it is**: If the CLI crashes or is interrupted after uploading some images but before writing `{album_id}/manifest.json`, the already-uploaded S3 objects (thumbnails, previews, originals) are left permanently in the bucket. They will never be referenced by any manifest and will not be cleaned up by the manifest-expiration mechanism (because there is no manifest to expire).

**Severity**: Low operationally (objects do eventually generate storage costs, but typically a small amount). Medium for bucket hygiene.

**Why deferred**: Fixing this cleanly requires either:
  - Writing a partial/draft manifest before uploading images and updating it atomically at completion, or
  - Tagging uploaded objects with the album ID and running a separate reconciliation pass.
  Both approaches add significant complexity to the upload flow.

**When to revisit**: When adding a `filmdrop gc` (garbage-collect) subcommand, or if bucket costs from abandoned uploads become noticeable.

---

## 4. Serial S3 deletes in `delete_prefix`

**Location**: `filmdrop-core/src/` — S3 client wrapper, `delete_prefix` function

**What it is**: Album deletion (`filmdrop delete ALBUM-ID`) lists all objects under the album prefix and then deletes them one by one with individual `DeleteObject` API calls. The S3 `DeleteObjects` (batch) API accepts up to 1000 object keys per request, which would reduce API calls by ~1000x for large albums.

**Severity**: Low. A 36-image album has ~108 objects (3 tiers × 36 photos + manifest). Serial deletes complete in a few seconds. Only becomes a problem for albums with thousands of images.

**Why deferred**: The current implementation is correct and simple. Batching is a pure performance improvement with no behavioral change. It was identified but not prioritized relative to user-facing features.

**When to revisit**: If `delete` commands time out in practice, or as a low-risk cleanup task for a contributor looking for a well-scoped issue.

---

## 5. Schema evolution — no `serde(default)` on manifest fields

**Location**: `filmdrop-core/src/` — `AlbumManifest` and related types

**What it is**: The `AlbumManifest` struct (and any nested types) does not use `#[serde(default)]` on fields. This means that if a new optional field is added to the struct, all existing manifests in S3 (which were serialized without that field) will fail deserialization with a missing-field error when the new server version tries to read them.

**Severity**: High for forward compatibility. In practice, S3 manifests expire within days, so the window for breaking existing data is narrow — but it is non-zero, and a rolling deployment would expose both versions simultaneously.

**Why deferred**: The risk was assessed as acceptable given the short manifest lifetime and the current single-operator deployment model. No version mismatch has occurred yet.

**When to revisit**: Before adding any new field to `AlbumManifest`. The fix is one attribute (`#[serde(default)]` or `Option<T>`) per new field. Also consider adding a `schema_version` field now while there is no live data to migrate.

---

## 6. Album ID hash collision — 64-bit truncation

**Location**: `filmdrop-cli/src/` — album ID derivation

**What it is**: The album ID is derived by computing SHA-256 over the sorted canonical paths of input files, then taking the first 16 hex characters (64 bits) of the digest. A birthday collision requires on the order of 2^32 (~4 billion) albums before the probability of any collision reaches 50%. 

**Severity**: Negligible at any realistic scale. Included for completeness.

**Why deferred**: No action required. The 64-bit truncation was a deliberate readability trade-off (shorter URLs). If the project ever scales to millions of users creating albums, consider extending to 96 or 128 bits.

**When to revisit**: Never, unless the operator is running a multi-tenant service with millions of concurrent album namespaces.
