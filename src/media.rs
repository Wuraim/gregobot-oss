use anyhow::{Result, anyhow};
use image::ImageEncoder;
use image::imageops::FilterType;

/// Longest side after normalize. Larger photos are downscaled; smaller ones are left as-is.
pub const JPEG_MAX_EDGE: u32 = 1280;
/// Lossy JPEG quality (0–100). 75 is a reasonable size/quality trade-off.
pub const JPEG_QUALITY: u8 = 75;

pub const JPEG_MIME: &str = "image/jpeg";

/// Decode any common raster input, downscale if needed, re-encode as JPEG.
/// EXIF (including GPS) is dropped by the decode → RGB → encode round-trip.
pub fn normalize_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() {
        return Err(anyhow!("empty image bytes"));
    }

    let img = image::load_from_memory(bytes)?;
    let img = if img.width() > JPEG_MAX_EDGE || img.height() > JPEG_MAX_EDGE {
        img.resize(JPEG_MAX_EDGE, JPEG_MAX_EDGE, FilterType::Triangle)
    } else {
        img
    };
    let rgb = img.into_rgb8();

    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    encoder.write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    if out.len() < 2 || out[0] != 0xff || out[1] != 0xd8 {
        return Err(anyhow!("JPEG encoder produced invalid output"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, ImageEncoder, Rgb};

    fn solid_jpeg(width: u32, height: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_pixel(width, height, Rgb([40u8, 80, 120]));
        let dyn_img = DynamicImage::ImageRgb8(img);
        let mut out = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
        encoder
            .write_image(
                dyn_img.as_bytes(),
                dyn_img.width(),
                dyn_img.height(),
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        out
    }

    #[test]
    fn normalize_keeps_jpeg_magic() {
        let src = solid_jpeg(64, 48);
        let out = normalize_to_jpeg(&src).unwrap();
        assert_eq!(&out[..2], &[0xff, 0xd8]);
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 64);
        assert_eq!(decoded.height(), 48);
    }

    #[test]
    fn normalize_downscales_long_edge() {
        let src = solid_jpeg(2000, 1000);
        let out = normalize_to_jpeg(&src).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert!(decoded.width() <= JPEG_MAX_EDGE);
        assert!(decoded.height() <= JPEG_MAX_EDGE);
        assert_eq!(decoded.width(), JPEG_MAX_EDGE);
        assert_eq!(decoded.height(), 640);
    }

    #[test]
    fn normalize_rejects_empty() {
        assert!(normalize_to_jpeg(&[]).is_err());
    }
}
