//! Message photo upload processing.
//!
//! The HTTP handlers keep multipart parsing and database/storage orchestration
//! in `api::messages`; this module owns the CPU-heavy image work so it can be
//! exercised with in-memory bytes and run off Actix async workers.

use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::error::AppError;

pub(crate) const MAX_MESSAGE_PHOTOS: usize = 4;
pub(crate) const PHOTO_MAX_BYTES: usize = 10 * 1024 * 1024;
const PHOTO_MAX_PIXELS: u64 = 25_000_000;
const FULL_IMAGE_MAX_EDGE: u32 = 2048;
const THUMBNAIL_MAX_EDGE: u32 = 512;
pub(crate) const STORED_PHOTO_CONTENT_TYPE: &str = "image/webp";

#[derive(Debug)]
pub(crate) struct UploadedPhoto {
    pub(crate) content_type: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct ProcessedPhoto {
    pub(crate) full_bytes: Vec<u8>,
    pub(crate) full_width: i32,
    pub(crate) full_height: i32,
    pub(crate) thumbnail_bytes: Vec<u8>,
    pub(crate) thumbnail_width: i32,
    pub(crate) thumbnail_height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportedPhotoFormat {
    Jpeg,
    Png,
    WebP,
}

impl SupportedPhotoFormat {
    fn image_format(self) -> ImageFormat {
        match self {
            SupportedPhotoFormat::Jpeg => ImageFormat::Jpeg,
            SupportedPhotoFormat::Png => ImageFormat::Png,
            SupportedPhotoFormat::WebP => ImageFormat::WebP,
        }
    }
}

pub(crate) async fn process_uploaded_photos(
    photos: Vec<UploadedPhoto>,
) -> Result<Vec<ProcessedPhoto>, AppError> {
    let mut processed = Vec::with_capacity(photos.len());
    for photo in photos {
        let prepared = tokio::task::spawn_blocking(move || process_uploaded_photo(photo))
            .await
            .map_err(|e| AppError::Internal(format!("photo processing task failed: {e}")))??;
        processed.push(prepared);
    }
    Ok(processed)
}

fn process_uploaded_photo(photo: UploadedPhoto) -> Result<ProcessedPhoto, AppError> {
    let format =
        photo_content_type_format(&photo.content_type).ok_or(AppError::UnsupportedPhoto)?;
    if photo.bytes.is_empty() || !photo_magic_matches(format, &photo.bytes) {
        return Err(AppError::UnsupportedPhoto);
    }
    if format == SupportedPhotoFormat::WebP && is_animated_webp(&photo.bytes) {
        return Err(AppError::UnsupportedPhoto);
    }

    let image_format = format.image_format();
    let (original_width, original_height) =
        ImageReader::with_format(Cursor::new(photo.bytes.as_slice()), image_format)
            .into_dimensions()
            .map_err(|_| AppError::UnsupportedPhoto)?;
    validate_photo_dimensions(original_width, original_height)?;

    let mut decoder = ImageReader::with_format(Cursor::new(photo.bytes), image_format)
        .into_decoder()
        .map_err(|_| AppError::UnsupportedPhoto)?;
    let (decoded_width, decoded_height) = decoder.dimensions();
    validate_photo_dimensions(decoded_width, decoded_height)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut decoded =
        DynamicImage::from_decoder(decoder).map_err(|_| AppError::UnsupportedPhoto)?;
    decoded.apply_orientation(orientation);
    validate_photo_dimensions(decoded.width(), decoded.height())?;

    let full = resize_to_max_edge(&decoded, FULL_IMAGE_MAX_EDGE);
    let thumbnail = resize_to_max_edge(&decoded, THUMBNAIL_MAX_EDGE);
    let full_width = full.width() as i32;
    let full_height = full.height() as i32;
    let thumbnail_width = thumbnail.width() as i32;
    let thumbnail_height = thumbnail.height() as i32;
    let full_bytes = encode_webp(full)?;
    let thumbnail_bytes = encode_webp(thumbnail)?;

    Ok(ProcessedPhoto {
        full_bytes,
        full_width,
        full_height,
        thumbnail_bytes,
        thumbnail_width,
        thumbnail_height,
    })
}

fn photo_content_type_format(content_type: &str) -> Option<SupportedPhotoFormat> {
    match content_type {
        "image/jpeg" | "image/jpg" => Some(SupportedPhotoFormat::Jpeg),
        "image/png" => Some(SupportedPhotoFormat::Png),
        "image/webp" => Some(SupportedPhotoFormat::WebP),
        _ => None,
    }
}

fn photo_magic_matches(format: SupportedPhotoFormat, bytes: &[u8]) -> bool {
    match format {
        SupportedPhotoFormat::Jpeg => {
            bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff
        }
        SupportedPhotoFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        SupportedPhotoFormat::WebP => {
            bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
        }
    }
}

fn is_animated_webp(bytes: &[u8]) -> bool {
    if !photo_magic_matches(SupportedPhotoFormat::WebP, bytes) {
        return false;
    }

    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        offset += 8;

        if chunk == b"VP8X" && size > 0 && offset < bytes.len() {
            return bytes[offset] & 0b0000_0010 != 0;
        }
        if chunk == b"ANIM" || chunk == b"ANMF" {
            return true;
        }

        let padded = size.saturating_add(size % 2);
        offset = offset.saturating_add(padded);
    }

    false
}

fn validate_photo_dimensions(width: u32, height: u32) -> Result<(), AppError> {
    if width == 0 || height == 0 {
        return Err(AppError::UnsupportedPhoto);
    }

    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels > PHOTO_MAX_PIXELS {
        return Err(AppError::PhotoDimensionsTooLarge);
    }
    Ok(())
}

fn resize_to_max_edge(image: &DynamicImage, max_edge: u32) -> DynamicImage {
    if image.width() <= max_edge && image.height() <= max_edge {
        return image.clone();
    }
    image.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3)
}

fn encode_webp(image: DynamicImage) -> Result<Vec<u8>, AppError> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::WebP)
        .map_err(|e| AppError::Internal(format!("encode photo webp: {e}")))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    use super::*;

    fn image_bytes(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        match format {
            ImageFormat::Jpeg => {
                DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 0, 0])))
            }
            _ => DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                width,
                height,
                Rgba([255, 0, 0, 255]),
            )),
        }
        .write_to(&mut out, format)
        .unwrap();
        out.into_inner()
    }

    fn decoded_dimensions(bytes: &[u8]) -> (u32, u32) {
        ImageReader::with_format(Cursor::new(bytes), ImageFormat::WebP)
            .into_dimensions()
            .unwrap()
    }

    fn app1_exif_orientation(orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&0u16.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());

        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let segment_len = (payload.len() + 2) as u16;
        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&segment_len.to_be_bytes());
        segment.extend_from_slice(&payload);
        segment
    }

    fn jpeg_with_exif_orientation(width: u32, height: u32, orientation: u16) -> Vec<u8> {
        let jpeg = image_bytes(width, height, ImageFormat::Jpeg);
        assert!(jpeg.starts_with(&[0xff, 0xd8]));
        let mut with_exif = Vec::new();
        with_exif.extend_from_slice(&jpeg[..2]);
        with_exif.extend_from_slice(&app1_exif_orientation(orientation));
        with_exif.extend_from_slice(&jpeg[2..]);
        with_exif
    }

    fn animated_webp_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&22u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.push(0b0000_0010);
        bytes.extend_from_slice(&[0; 9]);
        bytes
    }

    #[test]
    fn accepts_and_normalizes_jpeg_png_and_static_webp() {
        for (content_type, format) in [
            ("image/jpeg", ImageFormat::Jpeg),
            ("image/png", ImageFormat::Png),
            ("image/webp", ImageFormat::WebP),
        ] {
            let processed = process_uploaded_photo(UploadedPhoto {
                content_type: content_type.to_owned(),
                bytes: image_bytes(7, 3, format),
            })
            .unwrap();

            assert_eq!(processed.full_width, 7);
            assert_eq!(processed.full_height, 3);
            assert_eq!(processed.thumbnail_width, 7);
            assert_eq!(processed.thumbnail_height, 3);
            assert!(processed.full_bytes.starts_with(b"RIFF"));
            assert!(processed.thumbnail_bytes.starts_with(b"RIFF"));
            assert_eq!(decoded_dimensions(&processed.full_bytes), (7, 3));
        }
    }

    #[test]
    fn resizes_derivatives_without_upscaling_or_changing_aspect_ratio() {
        let processed = process_uploaded_photo(UploadedPhoto {
            content_type: "image/png".to_owned(),
            bytes: image_bytes(3000, 1000, ImageFormat::Png),
        })
        .unwrap();

        assert_eq!(processed.full_width, FULL_IMAGE_MAX_EDGE as i32);
        assert!(processed.full_height <= FULL_IMAGE_MAX_EDGE as i32);
        assert_eq!(processed.thumbnail_width, THUMBNAIL_MAX_EDGE as i32);
        assert!(processed.thumbnail_height <= THUMBNAIL_MAX_EDGE as i32);

        let full_ratio = f64::from(processed.full_width) / f64::from(processed.full_height);
        let thumb_ratio =
            f64::from(processed.thumbnail_width) / f64::from(processed.thumbnail_height);
        assert!((full_ratio - 3.0).abs() < 0.01);
        assert!((thumb_ratio - 3.0).abs() < 0.02);
    }

    #[test]
    fn applies_exif_orientation_and_strips_metadata_on_reencode() {
        let processed = process_uploaded_photo(UploadedPhoto {
            content_type: "image/jpeg".to_owned(),
            bytes: jpeg_with_exif_orientation(2, 4, 6),
        })
        .unwrap();

        assert_eq!((processed.full_width, processed.full_height), (4, 2));
        assert_eq!(decoded_dimensions(&processed.full_bytes), (4, 2));
        assert!(
            !processed
                .full_bytes
                .windows(4)
                .any(|window| window == b"EXIF")
        );
        assert!(
            !processed
                .full_bytes
                .windows(4)
                .any(|window| window == b"Exif")
        );
    }

    #[test]
    fn rejects_unsupported_or_unsafe_inputs() {
        let png = image_bytes(1, 1, ImageFormat::Png);
        for (content_type, bytes) in [
            ("image/png", b"not a png".to_vec()),
            ("image/jpeg", png),
            ("image/heic", b"\0\0\0\x18ftypheic".to_vec()),
            ("image/heif", b"\0\0\0\x18ftypheif".to_vec()),
            ("image/gif", b"GIF89a animated-ish".to_vec()),
            ("image/webp", animated_webp_bytes()),
        ] {
            let err = process_uploaded_photo(UploadedPhoto {
                content_type: content_type.to_owned(),
                bytes,
            })
            .unwrap_err();
            assert!(matches!(err, AppError::UnsupportedPhoto));
        }
    }

    #[test]
    fn validates_zero_and_excessive_dimensions_with_stable_error_kinds() {
        assert!(matches!(
            validate_photo_dimensions(0, 1).unwrap_err(),
            AppError::UnsupportedPhoto
        ));
        assert!(matches!(
            validate_photo_dimensions(6_000, 5_000).unwrap_err(),
            AppError::PhotoDimensionsTooLarge
        ));
    }

    #[tokio::test]
    async fn async_batch_processing_uses_the_blocking_processor_path() {
        let processed = process_uploaded_photos(vec![UploadedPhoto {
            content_type: "image/png".to_owned(),
            bytes: image_bytes(2, 2, ImageFormat::Png),
        }])
        .await
        .unwrap();

        assert_eq!(processed.len(), 1);
        assert_eq!(processed[0].full_width, 2);
    }
}
