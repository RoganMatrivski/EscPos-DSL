use base64::Engine;
use image::{DynamicImage, GrayImage, ImageFormat};
use std::io::Cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DitherAlgo {
    FloydSteinberg,
    Bayer2x2,
    Bayer4x4,
    Bayer8x8,
    Bayer16x16,
    ClusterDot4x4,
    ClusterDot8x8,
    Atkinson,
    Burkes,
    JarvisJudiceNinke,
    Sierra,
    TwoRowSierra,
    SierraLite,
    Stucki,
}

pub fn to_luma_white_bg(img: &DynamicImage) -> GrayImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut gray = GrayImage::new(w, h);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = pixel[3] as f32 / 255.0;
        let luma = 0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32;
        let blended = (luma * alpha + 255.0 * (1.0 - alpha)).clamp(0.0, 255.0) as u8;
        gray.put_pixel(x, y, image::Luma([blended]));
    }
    gray
}

impl DitherAlgo {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "floyd-steinberg" | "floyd_steinberg" | "fs" => Some(Self::FloydSteinberg),
            "bayer2" | "bayer2x2" | "bayer_2x2" => Some(Self::Bayer2x2),
            "bayer4" | "bayer4x4" | "bayer_4x4" => Some(Self::Bayer4x4),
            "bayer8" | "bayer8x8" | "bayer_8x8" | "bayer" => Some(Self::Bayer8x8),
            "bayer16" | "bayer16x16" | "bayer_16x16" => Some(Self::Bayer16x16),
            "cluster4" | "cluster_dot_4x4" | "cluster4x4" => Some(Self::ClusterDot4x4),
            "cluster8" | "cluster_dot_8x8" | "cluster8x8" => Some(Self::ClusterDot8x8),
            "atkinson" => Some(Self::Atkinson),
            "burkes" => Some(Self::Burkes),
            "jarvis" | "jarvis_judice_ninke" | "jjn" => Some(Self::JarvisJudiceNinke),
            "sierra" => Some(Self::Sierra),
            "two_row_sierra" | "tworowsierra" => Some(Self::TwoRowSierra),
            "sierra_lite" | "sierralite" => Some(Self::SierraLite),
            "stucki" => Some(Self::Stucki),
            _ => None,
        }
    }

    pub fn apply(&self, gray: &mut GrayImage) -> Result<(), String> {
        let (width, height) = gray.dimensions();
        let mut buf = dithr::gray_u8_packed(gray.as_mut(), width as usize, height as usize)
            .map_err(|e| e.to_string())?;
        let q = dithr::QuantizeMode::gray_bits(1).map_err(|e| e.to_string())?;

        match self {
            Self::FloydSteinberg => dithr::diffusion::floyd_steinberg_in_place(&mut buf, q),
            Self::Bayer2x2 => dithr::ordered::bayer_2x2_in_place(&mut buf, q),
            Self::Bayer4x4 => dithr::ordered::bayer_4x4_in_place(&mut buf, q),
            Self::Bayer8x8 => dithr::ordered::bayer_8x8_in_place(&mut buf, q),
            Self::Bayer16x16 => dithr::ordered::bayer_16x16_in_place(&mut buf, q),
            Self::ClusterDot4x4 => dithr::ordered::cluster_dot_4x4_in_place(&mut buf, q),
            Self::ClusterDot8x8 => dithr::ordered::cluster_dot_8x8_in_place(&mut buf, q),
            Self::Atkinson => dithr::diffusion::atkinson_in_place(&mut buf, q),
            Self::Burkes => dithr::diffusion::burkes_in_place(&mut buf, q),
            Self::JarvisJudiceNinke => dithr::diffusion::jarvis_judice_ninke_in_place(&mut buf, q),
            Self::Sierra => dithr::diffusion::sierra_in_place(&mut buf, q),
            Self::TwoRowSierra => dithr::diffusion::two_row_sierra_in_place(&mut buf, q),
            Self::SierraLite => dithr::diffusion::sierra_lite_in_place(&mut buf, q),
            Self::Stucki => dithr::diffusion::stucki_in_place(&mut buf, q),
        }
        .map_err(|e| e.to_string())
    }
}

/// Serializes a DynamicImage into a `<img w="..." h="...">BASE64</img>` DSL tag.
/// The image is dithered to monochrome (1-bit), saved as PNG, and base64-encoded.
pub fn encode_image_tag_with_dither(
    img: &DynamicImage,
    dither: Option<DitherAlgo>,
) -> Result<String, String> {
    let width = img.width();
    let height = img.height();

    let mut gray: GrayImage = to_luma_white_bg(img);
    if let Some(algo) = dither {
        algo.apply(&mut gray)?;
    } else {
        DitherAlgo::FloydSteinberg.apply(&mut gray)?;
    }

    let mut png_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut png_bytes);
    gray.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode image to PNG: {}", e))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    let dither_attr = dither
        .map(|d| match d {
            DitherAlgo::FloydSteinberg => " dither=\"floyd-steinberg\"",
            DitherAlgo::Bayer2x2 => " dither=\"bayer2x2\"",
            DitherAlgo::Bayer4x4 => " dither=\"bayer4x4\"",
            DitherAlgo::Bayer8x8 => " dither=\"bayer8x8\"",
            DitherAlgo::Bayer16x16 => " dither=\"bayer16x16\"",
            DitherAlgo::ClusterDot4x4 => " dither=\"cluster4x4\"",
            DitherAlgo::ClusterDot8x8 => " dither=\"cluster8x8\"",
            DitherAlgo::Atkinson => " dither=\"atkinson\"",
            DitherAlgo::Burkes => " dither=\"burkes\"",
            DitherAlgo::JarvisJudiceNinke => " dither=\"jarvis\"",
            DitherAlgo::Sierra => " dither=\"sierra\"",
            DitherAlgo::TwoRowSierra => " dither=\"two-row-sierra\"",
            DitherAlgo::SierraLite => " dither=\"sierra-lite\"",
            DitherAlgo::Stucki => " dither=\"stucki\"",
        })
        .unwrap_or("");

    Ok(format!(
        r#"<img w="{}" h="{}"{}>{}</img>"#,
        width, height, dither_attr, b64
    ))
}

pub fn encode_image_tag(img: &DynamicImage) -> Result<String, String> {
    encode_image_tag_with_dither(img, Some(DitherAlgo::FloydSteinberg))
}
