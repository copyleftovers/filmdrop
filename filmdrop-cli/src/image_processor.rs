use anyhow::{Context, Result};
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use std::io::Cursor;
use std::path::Path;

pub struct ProcessedImage {
    pub original: Vec<u8>,
    pub preview: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

const THUMBNAIL_SIZE: u32 = 400;
const PREVIEW_SIZE: u32 = 2048;

pub fn process_image_from_bytes(original_bytes: Vec<u8>, path: &Path) -> Result<ProcessedImage> {
    tracing::info!("Processing image: {}", path.display());

    if !is_jpeg_file(path) {
        anyhow::bail!(
            "Only JPEG files (.jpg, .jpeg) are supported. Got: {}",
            path.display()
        );
    }

    let img = image::load_from_memory(&original_bytes)
        .context(format!("Failed to decode image: {}", path.display()))?;

    let (width, height) = img.dimensions();

    let preview = create_resized_jpeg(&img, PREVIEW_SIZE, 90)?;
    let thumbnail = create_resized_jpeg(&img, THUMBNAIL_SIZE, 85)?;

    Ok(ProcessedImage {
        original: original_bytes,
        preview,
        thumbnail,
        width,
        height,
    })
}

fn create_resized_jpeg(img: &DynamicImage, max_size: u32, quality: u8) -> Result<Vec<u8>> {
    let (width, height) = img.dimensions();

    // Only resize if larger than target
    let resized = if width > max_size || height > max_size {
        img.resize(max_size, max_size, FilterType::Lanczos3)
    } else {
        img.clone()
    };

    encode_jpeg(&resized, quality)
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality)
        .encode_image(img)
        .context("Failed to encode JPEG")?;
    Ok(buffer.into_inner())
}

pub fn is_image_file(path: &Path) -> bool {
    is_jpeg_file(path)
}

pub fn is_jpeg_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        matches!(
            ext.to_str().unwrap_or("").to_lowercase().as_str(),
            "jpg" | "jpeg"
        )
    } else {
        false
    }
}
