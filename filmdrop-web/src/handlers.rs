use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use filmdrop_core::AlbumManifest;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::state::AppState;

/// Index page
pub async fn index() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Filmdrop</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 800px;
            margin: 100px auto;
            padding: 20px;
            text-align: center;
        }
        h1 {
            font-size: 3rem;
            font-weight: 300;
            margin-bottom: 1rem;
        }
        p {
            font-size: 1.2rem;
            color: #666;
        }
    </style>
</head>
<body>
    <h1>Filmdrop</h1>
    <p>Access your private gallery using the link provided.</p>
</body>
</html>
        "#,
    )
}

/// Gallery page
pub async fn gallery(State(state): State<AppState>, Path(album_id): Path<String>) -> Html<String> {
    tracing::info!("Gallery page request: album_id={}", album_id);

    // Verify album exists by checking manifest
    let manifest_key = format!("{album_id}/manifest.json");
    let manifest_data = match state.s3.download_file(&manifest_key).await {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to fetch manifest for album {}: {:?}", album_id, e);
            return Html(generate_404_html());
        }
    };

    let manifest_json = match String::from_utf8(manifest_data) {
        Ok(json) => json,
        Err(_) => return Html(generate_404_html()),
    };

    let mut manifest: AlbumManifest = match serde_json::from_str(&manifest_json) {
        Ok(m) => m,
        Err(_) => return Html(generate_404_html()),
    };

    // Generate presigned URLs for direct S3 access (valid for 7 days to match object expiration)
    let expires_in = std::time::Duration::from_secs(7 * 24 * 3600);
    for image in &mut manifest.images {
        let thumbnail_key = format!("{album_id}/{}", image.thumbnail_path);
        let preview_key = format!("{album_id}/{}", image.preview_path);
        let original_key = format!("{album_id}/{}", image.original_path);

        image.thumbnail_url = state
            .s3
            .generate_presigned_url(&thumbnail_key, expires_in)
            .await
            .ok();
        image.preview_url = state
            .s3
            .generate_presigned_url(&preview_key, expires_in)
            .await
            .ok();
        image.original_url = state
            .s3
            .generate_presigned_url(&original_key, expires_in)
            .await
            .ok();
    }

    // Generate HTML
    let html = generate_gallery_html(&album_id, &manifest);

    Html(html)
}

/// Get album manifest JSON
pub async fn get_manifest(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
) -> Result<Json<AlbumManifest>, StatusCode> {
    tracing::info!("Manifest API request: album_id={}", album_id);

    let manifest_key = format!("{album_id}/manifest.json");
    let manifest_data = state.s3.download_file(&manifest_key).await.map_err(|e| {
        tracing::error!("Failed to fetch manifest for album {}: {:?}", album_id, e);
        StatusCode::NOT_FOUND
    })?;

    let manifest_json =
        String::from_utf8(manifest_data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut manifest: AlbumManifest =
        serde_json::from_str(&manifest_json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Generate presigned URLs for all images (valid for 7 days to match object expiration)
    let expires_in = std::time::Duration::from_secs(7 * 24 * 3600);
    for image in &mut manifest.images {
        let thumbnail_key = format!("{album_id}/{}", image.thumbnail_path);
        let preview_key = format!("{album_id}/{}", image.preview_path);
        let original_key = format!("{album_id}/{}", image.original_path);

        image.thumbnail_url = state
            .s3
            .generate_presigned_url(&thumbnail_key, expires_in)
            .await
            .ok();
        image.preview_url = state
            .s3
            .generate_presigned_url(&preview_key, expires_in)
            .await
            .ok();
        image.original_url = state
            .s3
            .generate_presigned_url(&original_key, expires_in)
            .await
            .ok();
    }

    Ok(Json(manifest))
}

/// Get image from S3
pub async fn get_image(
    State(state): State<AppState>,
    Path((album_id, path)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    tracing::info!("Image request: album_id={}, path={}", album_id, path);

    let s3_key = format!("{album_id}/{path}");
    tracing::debug!("Computed S3 key: {}", s3_key);

    let image_data = state.s3.download_file(&s3_key).await.map_err(|e| {
        tracing::error!("Failed to fetch image {}: {:?}", s3_key, e);
        StatusCode::NOT_FOUND
    })?;

    // Determine content type
    let content_type = if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".png") {
        "image/png"
    } else {
        "application/octet-stream"
    };

    tracing::debug!(
        "Serving image: s3_key={}, content_type={}, size={} bytes",
        s3_key,
        content_type,
        image_data.len()
    );

    // Check if download is requested
    let is_download = params.get("download").map(|v| v == "true").unwrap_or(false);

    if is_download {
        // Extract filename from path
        let filename = path.split('/').next_back().unwrap_or("image.jpg");
        Ok((
            [
                (header::CONTENT_TYPE, content_type),
                (
                    header::CONTENT_DISPOSITION,
                    &format!("attachment; filename=\"{filename}\""),
                ),
            ],
            image_data,
        )
            .into_response())
    } else {
        Ok(([(header::CONTENT_TYPE, content_type)], image_data).into_response())
    }
}

/// Download all originals for an album as a ZIP file
pub async fn download_album(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
) -> Result<Response, StatusCode> {
    tracing::info!("Download album request: album_id={}", album_id);

    // Download and parse manifest
    let manifest_key = format!("{album_id}/manifest.json");
    let manifest_data = state.s3.download_file(&manifest_key).await.map_err(|e| {
        tracing::error!("Failed to fetch manifest for album {}: {:?}", album_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let manifest_json = String::from_utf8(manifest_data).map_err(|e| {
        tracing::error!("Manifest is not valid UTF-8: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let manifest: AlbumManifest = serde_json::from_str(&manifest_json).map_err(|e| {
        tracing::error!("Failed to parse manifest JSON: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let album_name = manifest.name.clone();

    // Download all originals in parallel
    let semaphore = Arc::new(Semaphore::new(8));
    let mut join_set = tokio::task::JoinSet::new();
    for image in manifest.images {
        let s3 = state.s3.clone();
        let key = format!("{album_id}/{}", image.original_path);
        let semaphore = semaphore.clone();
        join_set.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let data = s3.download_file(&key).await.map_err(|e| {
                tracing::error!("Failed to download image {}: {:?}", key, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            let entry_name = format!("{}_{}", &image.id[..8], image.original_filename);
            Ok::<(String, Vec<u8>), StatusCode>((entry_name, data))
        });
    }

    let mut image_files: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(pair)) => image_files.push(pair),
            Ok(Err(status)) => return Err(status),
            Err(e) => {
                tracing::error!("Task join error: {:?}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // Build ZIP in memory using spawn_blocking (zip crate is sync)
    let zip_bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (filename, data) in image_files {
                zip.start_file(&filename, options)
                    .map_err(|e| e.to_string())?;
                std::io::Write::write_all(&mut zip, &data).map_err(|e| e.to_string())?;
            }
            zip.finish().map_err(|e| e.to_string())?;
        }
        Ok(buf.into_inner())
    })
    .await
    .map_err(|e| {
        tracing::error!("spawn_blocking join error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .map_err(|e| {
        tracing::error!("ZIP building error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Build slug from album name
    let slug: String = album_name
        .to_lowercase()
        .replace(' ', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let slug = if slug.is_empty() {
        "album".to_string()
    } else {
        slug
    };

    let content_disposition = format!("attachment; filename=\"{slug}.zip\"");

    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        zip_bytes,
    )
        .into_response())
}

fn generate_gallery_html(album_id: &str, manifest: &AlbumManifest) -> String {
    let album_id_json = serde_json::to_string(album_id).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{album_name} - Filmdrop</title>
    <style>
        :root {{
            --ctrl-bg: rgba(18, 18, 18, 0.72);
            --ctrl-bg-hover: rgba(18, 18, 18, 0.90);
            --ctrl-bg-active: rgba(255, 255, 255, 0.12);
            --ctrl-border: 1px solid rgba(255, 255, 255, 0.12);
            --ctrl-blur: blur(14px);
            --ctrl-color: rgba(255, 255, 255, 0.90);
            --ctrl-color-dim: rgba(255, 255, 255, 0.80);
            --radius-rect: 10px;
            --radius-pill: 999px;
            --ctrl-font: 0.85rem;
            --ctrl-font-sm: 0.82rem;
            --ctrl-weight: 500;
            --ctrl-transition: background 0.15s ease, opacity 0.15s ease;
            --ctrl-min-h: 44px;
        }}

        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: #ffffff;
            color: #333;
            line-height: 1.6;
            /* Safe area insets for notched devices */
            padding-top: env(safe-area-inset-top);
        }}

        body.lightbox-open {{
            overflow: hidden;
            position: fixed;
            width: 100%;
        }}

        .header {{
            padding: 40px 20px;
            text-align: center;
            border-bottom: 1px solid #eee;
        }}

        .header h1 {{
            font-size: 2.5rem;
            font-weight: 300;
            margin-bottom: 10px;
        }}

        .header p {{
            color: #666;
            font-size: 0.9rem;
        }}

        .download-all-btn {{
            display: inline-flex;
            align-items: center;
            gap: 6px;
            margin-top: 12px;
            padding: 0 20px;
            height: var(--ctrl-min-h);
            background: var(--ctrl-bg);
            backdrop-filter: var(--ctrl-blur);
            -webkit-backdrop-filter: var(--ctrl-blur);
            color: var(--ctrl-color);
            text-decoration: none;
            border-radius: var(--radius-pill);
            border: var(--ctrl-border);
            font-size: var(--ctrl-font);
            font-weight: var(--ctrl-weight);
            letter-spacing: 0.02em;
            transition: var(--ctrl-transition);
            cursor: pointer;
            line-height: 1;
        }}

        .download-all-btn:hover {{
            background: var(--ctrl-bg-hover);
        }}

        .download-all-btn:active {{
            background: var(--ctrl-bg-active);
        }}

        .gallery-container {{
            max-width: 1400px;
            margin: 0 auto;
            padding: 40px 20px;
        }}

        /* Centered justified gallery layout */
        .bento-grid {{
            display: flex;
            flex-wrap: wrap;
            justify-content: center;
            gap: 15px;
            align-items: center;
        }}

        .bento-item {{
            position: relative;
            cursor: pointer;
            background: #f5f5f5;
            border-radius: 4px;
            transition: transform 0.2s ease;
            flex: 0 0 auto;
            height: 300px;
            overflow: hidden;
        }}

        .bento-item:hover {{
            transform: translateY(-4px);
            box-shadow: 0 8px 20px rgba(0,0,0,0.1);
        }}

        .bento-item img {{
            display: block;
            width: 100%;
            height: 100%;
            object-fit: cover;
            border-radius: 4px;
            transition: opacity 0.3s ease;
        }}

        .thumb-download {{
            position: absolute;
            bottom: 10px;
            right: 10px;
            width: 36px;
            height: 36px;
            padding: 4px;
            margin: -4px;
            display: flex;
            align-items: center;
            justify-content: center;
            background: var(--ctrl-bg);
            backdrop-filter: var(--ctrl-blur);
            -webkit-backdrop-filter: var(--ctrl-blur);
            border: var(--ctrl-border);
            border-radius: var(--radius-rect);
            color: var(--ctrl-color);
            text-decoration: none;
            z-index: 2;
            transition: var(--ctrl-transition);
            opacity: 0;
        }}

        .thumb-download:hover {{
            background: var(--ctrl-bg-hover);
        }}

        .thumb-download:active {{
            background: var(--ctrl-bg-active);
        }}

        .bento-item:hover .thumb-download {{
            opacity: 1;
        }}

        /* Lightbox */
        .lightbox {{
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            background: rgba(0, 0, 0, 0.97);
            z-index: 1000;
            align-items: center;
            justify-content: center;
            opacity: 0;
            transition: opacity 0.3s ease;
        }}

        .lightbox.active {{
            display: flex;
            opacity: 1;
        }}

        .lightbox-content {{
            position: relative;
            width: 90vw;
            height: 90vh;
            display: flex;
            align-items: center;
            justify-content: center;
        }}

        .lightbox-image {{
            width: 100%;
            height: 100%;
            object-fit: contain;
            user-select: none;
            transition: opacity 0.2s ease;
            touch-action: none;
            -webkit-touch-callout: none;
        }}

        /* Top controls */
        .lightbox-controls {{
            position: fixed;
            top: max(20px, env(safe-area-inset-top));
            right: max(20px, env(safe-area-inset-right));
            display: flex;
            gap: 8px;
            z-index: 1001;
        }}

        .lightbox-btn {{
            background: var(--ctrl-bg);
            border: var(--ctrl-border);
            padding: 0 18px;
            height: var(--ctrl-min-h);
            cursor: pointer;
            font-size: var(--ctrl-font);
            font-weight: var(--ctrl-weight);
            color: var(--ctrl-color);
            border-radius: var(--radius-rect);
            transition: var(--ctrl-transition);
            backdrop-filter: var(--ctrl-blur);
            -webkit-backdrop-filter: var(--ctrl-blur);
            display: flex;
            align-items: center;
            line-height: 1;
        }}

        .lightbox-btn:hover {{
            background: var(--ctrl-bg-hover);
        }}

        .lightbox-btn:active {{
            background: var(--ctrl-bg-active);
        }}

        .close-btn {{
            position: fixed;
            top: max(20px, env(safe-area-inset-top));
            left: max(20px, env(safe-area-inset-left));
            background: var(--ctrl-bg);
            border: var(--ctrl-border);
            width: var(--ctrl-min-h);
            height: var(--ctrl-min-h);
            cursor: pointer;
            font-size: 1.25rem;
            color: var(--ctrl-color);
            border-radius: var(--radius-rect);
            z-index: 1001;
            transition: var(--ctrl-transition);
            backdrop-filter: var(--ctrl-blur);
            -webkit-backdrop-filter: var(--ctrl-blur);
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 0;
            line-height: 1;
        }}

        .close-btn:hover {{
            background: var(--ctrl-bg-hover);
        }}

        .close-btn:active {{
            background: var(--ctrl-bg-active);
        }}

        /* Navigation pill */
        .nav-pill {{
            position: fixed;
            bottom: max(24px, env(safe-area-inset-bottom));
            left: 50%;
            transform: translateX(-50%);
            display: flex;
            align-items: center;
            background: var(--ctrl-bg);
            backdrop-filter: var(--ctrl-blur);
            -webkit-backdrop-filter: var(--ctrl-blur);
            border: var(--ctrl-border);
            border-radius: var(--radius-pill);
            z-index: 1001;
            overflow: hidden;
        }}

        .pill-btn {{
            display: flex;
            align-items: center;
            justify-content: center;
            width: 48px;
            height: var(--ctrl-min-h);
            background: transparent;
            border: none;
            color: var(--ctrl-color);
            font-size: 1.5rem;
            line-height: 1;
            cursor: pointer;
            transition: var(--ctrl-transition);
            padding: 0;
        }}

        .pill-btn:hover {{
            background: var(--ctrl-bg-active);
        }}

        .pill-btn:active {{
            background: var(--ctrl-bg-active);
        }}

        .pill-btn:disabled {{
            opacity: 0.28;
            cursor: not-allowed;
        }}

        .pill-btn:disabled:hover {{
            background: transparent;
        }}

        .pill-counter {{
            padding: 0 6px;
            color: var(--ctrl-color-dim);
            font-size: var(--ctrl-font-sm);
            font-weight: var(--ctrl-weight);
            min-width: 56px;
            text-align: center;
            white-space: nowrap;
            letter-spacing: 0.04em;
            user-select: none;
        }}

        @media (max-width: 768px) {{
            .header h1 {{
                font-size: 2rem;
            }}

            .bento-grid {{
                flex-direction: column;
                align-items: stretch;
            }}

            .bento-item {{
                height: auto;
                width: 100%;
            }}

            .bento-item img {{
                width: 100%;
                height: auto;
            }}

            .thumb-download {{
                opacity: 1;
            }}
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>{album_name}</h1>
        <p>{image_count} photographs</p>
        <a class="download-all-btn" href="/api/album/{album_id}/download" download>
            <svg width="14" height="14" viewBox="0 0 12 12" fill="none" xmlns="http://www.w3.org/2000/svg">
                <path d="M6 1v7M3 5.5l3 3 3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                <path d="M1.5 10.5h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            </svg>
            Download all
        </a>
    </div>

    <div class="gallery-container">
        <div class="bento-grid" id="gallery">
            {thumbnails}
        </div>
    </div>

    <div class="lightbox" id="lightbox">
        <button class="close-btn" onclick="closeLightbox()">&times;</button>
        <div class="lightbox-controls">
            <button class="lightbox-btn" onclick="downloadImage()">Download</button>
        </div>
        <div class="lightbox-content">
            <img class="lightbox-image" id="lightbox-img" src="" alt="">
        </div>
        <div class="nav-pill" id="nav-pill">
            <button class="pill-btn" id="prev-btn" onclick="navigateImage(-1)">&#x2039;</button>
            <span class="pill-counter" id="image-counter">1 / 1</span>
            <button class="pill-btn" id="next-btn" onclick="navigateImage(1)">&#x203a;</button>
        </div>
    </div>

    <script>
        const albumId = {album_id_json};
        const images = {images_json};
        let currentImageIndex = 0;

        // Track which images have which tiers loaded
        const loadedTiers = {{}};

        // Cache for preloaded Image objects to prevent garbage collection
        const imageCache = {{}};

        // Progressive enhancement: upgrade thumbnails to previews in the gallery
        document.addEventListener('DOMContentLoaded', () => {{
            images.forEach((image, index) => {{
                const previewUrl = image.preview_url || `/api/album/${{albumId}}/image/${{image.preview_path}}`;
                const thumbImg = document.querySelector(`img[data-index="${{index}}"]`);

                if (thumbImg && previewUrl) {{
                    const previewImg = new Image();
                    previewImg.onload = () => {{
                        // Direct swap - no flashing fade animation
                        thumbImg.src = previewImg.src;

                        if (!loadedTiers[index]) loadedTiers[index] = {{}};
                        loadedTiers[index].preview = true;
                    }};
                    previewImg.src = previewUrl;
                }}
            }});
        }});

        function openLightbox(index) {{
            resetZoom();
            currentImageIndex = index;
            showImage(index);
            document.getElementById('lightbox').classList.add('active');
            document.body.classList.add('lightbox-open');
            updateNavButtons();
            preloadAdjacentImages();
        }}

        function showImage(index) {{
            const image = images[index];
            const lightboxImg = document.getElementById('lightbox-img');
            const counter = document.getElementById('image-counter');

            const tiers = loadedTiers[index] || {{}};
            const originalUrl = image.original_url || `/api/album/${{albumId}}/image/${{image.original_path}}`;
            const previewUrl = image.preview_url || `/api/album/${{albumId}}/image/${{image.preview_path}}`;
            const thumbnailUrl = image.thumbnail_url || `/api/album/${{albumId}}/image/${{image.thumbnail_path}}`;

            // Update counter
            counter.textContent = `${{index + 1}} / ${{images.length}}`;

            // If original is already loaded, show it immediately - no re-download
            if (tiers.original) {{
                lightboxImg.style.opacity = '1';
                lightboxImg.src = originalUrl;
                return;
            }}

            // Determine best available tier to show while loading original
            let initialSrc = thumbnailUrl;
            if (tiers.preview || image.preview_url) {{
                initialSrc = previewUrl;
            }}

            // Show best available tier immediately
            lightboxImg.style.opacity = '1';
            lightboxImg.src = initialSrc;

            // If showing thumbnail and preview not loaded yet, load preview first
            if (initialSrc === thumbnailUrl && !tiers.preview && previewUrl) {{
                const previewImg = new Image();
                previewImg.onload = () => {{
                    lightboxImg.style.opacity = '0.3';
                    setTimeout(() => {{
                        lightboxImg.src = previewImg.src;
                        lightboxImg.style.opacity = '1';
                    }}, 50);
                    if (!loadedTiers[index]) loadedTiers[index] = {{}};
                    loadedTiers[index].preview = true;
                }};
                previewImg.src = previewUrl;
            }}

            // Load original in background and swap when ready
            const fullImg = new Image();
            fullImg.onload = () => {{
                // Smooth transition to full-res
                lightboxImg.style.opacity = '0.5';
                setTimeout(() => {{
                    lightboxImg.src = fullImg.src;
                    lightboxImg.style.opacity = '1';
                }}, 50);
                if (!loadedTiers[index]) loadedTiers[index] = {{}};
                loadedTiers[index].original = true;

                // Cache the image object to prevent garbage collection
                if (!imageCache[index]) imageCache[index] = {{}};
                imageCache[index].original = fullImg;
            }};
            fullImg.src = originalUrl;
        }}

        function navigateImage(direction) {{
            const newIndex = currentImageIndex + direction;
            if (newIndex >= 0 && newIndex < images.length) {{
                currentImageIndex = newIndex;
                showImage(newIndex);
                updateNavButtons();
                preloadAdjacentImages();
            }}
        }}

        function updateNavButtons() {{
            const prevBtn = document.getElementById('prev-btn');
            const nextBtn = document.getElementById('next-btn');
            prevBtn.disabled = currentImageIndex === 0;
            nextBtn.disabled = currentImageIndex === images.length - 1;
        }}

        function preloadAdjacentImages() {{
            // Preload next and previous originals
            [-1, 1].forEach(offset => {{
                const idx = currentImageIndex + offset;
                if (idx >= 0 && idx < images.length) {{
                    const tiers = loadedTiers[idx] || {{}};

                    // Skip if already loaded
                    if (tiers.original) return;

                    const img = images[idx];
                    const originalUrl = img.original_url || `/api/album/${{albumId}}/image/${{img.original_path}}`;
                    const preloadImg = new Image();
                    preloadImg.onload = () => {{
                        if (!loadedTiers[idx]) loadedTiers[idx] = {{}};
                        loadedTiers[idx].original = true;

                        // Store in cache to prevent garbage collection
                        if (!imageCache[idx]) imageCache[idx] = {{}};
                        imageCache[idx].original = preloadImg;
                    }};
                    preloadImg.src = originalUrl;
                }}
            }});
        }}

        function closeLightbox() {{
            document.getElementById('lightbox').classList.remove('active');
            document.body.classList.remove('lightbox-open');
        }}

        function downloadImage() {{
            const image = images[currentImageIndex];
            // Use proxy endpoint with download parameter to get proper Content-Disposition header
            const downloadUrl = `/api/album/${{albumId}}/image/${{image.original_path}}?download=true`;

            // Create temporary link and trigger download
            const link = document.createElement('a');
            link.href = downloadUrl;
            link.download = image.original_filename;
            document.body.appendChild(link);
            link.click();
            document.body.removeChild(link);
        }}

        // Keyboard shortcuts
        document.addEventListener('keydown', (e) => {{
            const lightbox = document.getElementById('lightbox');
            if (!lightbox.classList.contains('active')) return;

            if (e.key === 'Escape') {{
                closeLightbox();
            }} else if (e.key === 'ArrowLeft') {{
                navigateImage(-1);
            }} else if (e.key === 'ArrowRight') {{
                navigateImage(1);
            }}
        }});

        // Close on background click
        document.getElementById('lightbox').addEventListener('click', (e) => {{
            if (e.target.id === 'lightbox') closeLightbox();
        }});

        // Zoom/pan touch handler for mobile lightbox
        let zoomScale = 1;
        let zoomTranslateX = 0;
        let zoomTranslateY = 0;
        let zoomLastTouchDist = 0;
        let zoomPanStartX = 0;
        let zoomPanStartY = 0;
        let zoomPanActive = false;
        let zoomLastTapTime = 0;

        function applyZoom() {{
            const img = document.getElementById('lightbox-img');
            img.style.transform = `translate(${{zoomTranslateX}}px, ${{zoomTranslateY}}px) scale(${{zoomScale}})`;
            img.style.transformOrigin = 'center center';
        }}

        function resetZoom() {{
            zoomScale = 1;
            zoomTranslateX = 0;
            zoomTranslateY = 0;
            applyZoom();
        }}

        // Patch navigateImage to reset zoom on navigation
        const _origNavigateImage = navigateImage;
        navigateImage = function(direction) {{
            resetZoom();
            _origNavigateImage(direction);
        }};

        // Patch closeLightbox to reset zoom on close
        const _origCloseLightbox = closeLightbox;
        closeLightbox = function() {{
            resetZoom();
            _origCloseLightbox();
        }};

        const lightboxContent = document.querySelector('.lightbox-content');

        lightboxContent.addEventListener('touchstart', (e) => {{
            if (e.touches.length === 2) {{
                // Pinch start
                zoomLastTouchDist = Math.hypot(
                    e.touches[0].clientX - e.touches[1].clientX,
                    e.touches[0].clientY - e.touches[1].clientY
                );
                zoomPanActive = false;
                e.preventDefault();
            }} else if (e.touches.length === 1) {{
                if (zoomScale > 1) {{
                    // Pan start
                    zoomPanStartX = e.touches[0].clientX - zoomTranslateX;
                    zoomPanStartY = e.touches[0].clientY - zoomTranslateY;
                    zoomPanActive = true;
                }}
                // Double-tap detection
                const now = Date.now();
                if (now - zoomLastTapTime < 300) {{
                    // Double tap: toggle fit <-> 100%
                    const img = document.getElementById('lightbox-img');
                    if (zoomScale > 1) {{
                        resetZoom();
                    }} else {{
                        const nativeW = img.naturalWidth;
                        const containerW = img.clientWidth;
                        if (nativeW && containerW) {{
                            zoomScale = Math.min(nativeW / containerW, 4);
                        }} else {{
                            zoomScale = 2.5;
                        }}
                        applyZoom();
                    }}
                    zoomLastTapTime = 0;
                }} else {{
                    zoomLastTapTime = now;
                }}
            }}
        }}, {{ passive: false }});

        lightboxContent.addEventListener('touchmove', (e) => {{
            if (e.touches.length === 2) {{
                // Pinch zoom
                const dist = Math.hypot(
                    e.touches[0].clientX - e.touches[1].clientX,
                    e.touches[0].clientY - e.touches[1].clientY
                );
                if (zoomLastTouchDist > 0) {{
                    zoomScale = Math.max(1, Math.min(zoomScale * (dist / zoomLastTouchDist), 4));
                    if (zoomScale === 1) {{ zoomTranslateX = 0; zoomTranslateY = 0; }}
                    applyZoom();
                }}
                zoomLastTouchDist = dist;
                e.preventDefault();
            }} else if (e.touches.length === 1 && zoomPanActive) {{
                // Pan
                zoomTranslateX = e.touches[0].clientX - zoomPanStartX;
                zoomTranslateY = e.touches[0].clientY - zoomPanStartY;
                applyZoom();
                e.preventDefault();
            }}
        }}, {{ passive: false }});

        lightboxContent.addEventListener('touchend', (e) => {{
            if (e.touches.length < 2) {{
                zoomLastTouchDist = 0;
            }}
            if (e.touches.length === 0) {{
                zoomPanActive = false;
            }}
        }}, {{ passive: true }});

        // Bulk download loading state
        const downloadAllBtn = document.querySelector('.download-all-btn');
        if (downloadAllBtn) {{
            downloadAllBtn.addEventListener('click', () => {{
                const original = downloadAllBtn.innerHTML;
                downloadAllBtn.textContent = 'Preparing…';
                downloadAllBtn.style.pointerEvents = 'none';
                downloadAllBtn.style.opacity = '0.7';
                setTimeout(() => {{
                    downloadAllBtn.innerHTML = original;
                    downloadAllBtn.style.pointerEvents = '';
                    downloadAllBtn.style.opacity = '';
                }}, 8000);
            }});
        }}
    </script>
</body>
</html>"#,
        album_name = html_escape(&manifest.name),
        album_id = album_id,
        album_id_json = album_id_json,
        image_count = manifest.images.len(),
        thumbnails = generate_thumbnails_html(album_id, manifest),
        images_json = serde_json::to_string(&manifest.images).unwrap_or_else(|_| "[]".to_string()),
    )
}

fn generate_thumbnails_html(album_id: &str, manifest: &AlbumManifest) -> String {
    manifest
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let thumbnail_src = image.thumbnail_url.clone().unwrap_or_else(|| {
                // Fallback to proxy URL if presigned URL not available
                format!("/api/album/{}/image/{}", album_id, image.thumbnail_path)
            });

            let download_href = format!(
                "/api/album/{}/image/{}?download=true",
                album_id, image.original_path
            );

            format!(
                r#"<div class="bento-item" style="aspect-ratio:{iw}/{ih}" onclick="openLightbox({index})">
                <img data-index="{index}" src="{thumbnail_src}" alt="{filename}" loading="lazy" style="opacity:0" onload="this.style.opacity='1'">
                <a class="thumb-download" href="{download_href}" download="{orig_filename}" onclick="event.stopPropagation()" title="Download original">
                    <svg width="14" height="14" viewBox="0 0 12 12" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M6 1v7M3 5.5l3 3 3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                        <path d="M1.5 10.5h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                    </svg>
                </a>
            </div>"#,
                iw = image.width,
                ih = image.height,
                index = index,
                thumbnail_src = html_escape(&thumbnail_src),
                filename = html_escape(&image.original_filename),
                download_href = html_escape(&download_href),
                orig_filename = html_escape(&image.original_filename),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn generate_404_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Not Found</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
            margin: 0;
            background: #ffffff;
            color: #333;
        }
        .container {
            text-align: center;
            padding: 40px 20px;
            max-width: 500px;
        }
        h1 {
            font-size: 6rem;
            font-weight: 300;
            margin: 0;
            color: #999;
        }
        p {
            font-size: 1.2rem;
            margin: 20px 0;
            color: #666;
        }
        a {
            color: #333;
            text-decoration: none;
            border-bottom: 1px solid #333;
        }
        a:hover {
            border-bottom: 2px solid #333;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>404</h1>
        <p>This gallery doesn't exist or has expired.</p>
        <p><a href="/">Return home</a></p>
    </div>
</body>
</html>"#
        .to_string()
}
