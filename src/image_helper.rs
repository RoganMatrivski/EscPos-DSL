use base64::Engine;
use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use std::io::Cursor;

/// Serializes a DynamicImage into a `<img w="..." h="...">BASE64</img>` DSL tag.
/// The image is dithered to monochrome (1-bit Floyd-Steinberg), saved as PNG, and base64-encoded.
pub fn encode_image_tag(img: &DynamicImage) -> Result<String, String> {
    let width = img.width();
    let height = img.height();

    // Convert to grayscale
    let mut gray: GrayImage = img.to_luma8();

    // Perform Floyd-Steinberg dithering to monochrome (0 or 255)
    dither_floyd_steinberg(&mut gray);

    // Encode to PNG bytes
    let mut png_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut png_bytes);
    gray.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode image to PNG: {}", e))?;

    // Base64 encode
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    Ok(format!(r#"<img w="{}" h="{}">{}</img>"#, width, height, b64))
}

fn dither_floyd_steinberg(img: &mut GrayImage) {
    let (width, height) = img.dimensions();
    let mut buffer: Vec<f32> = img.pixels().map(|p| p.0[0] as f32).collect();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let old_val = buffer[idx];
            let new_val = if old_val < 128.0 { 0.0 } else { 255.0 };
            buffer[idx] = new_val;
            let err = old_val - new_val;

            if x + 1 < width {
                buffer[(y * width + (x + 1)) as usize] += err * 7.0 / 16.0;
            }
            if y + 1 < height {
                if x > 0 {
                    buffer[((y + 1) * width + (x - 1)) as usize] += err * 3.0 / 16.0;
                }
                buffer[((y + 1) * width + x) as usize] += err * 5.0 / 16.0;
                if x + 1 < width {
                    buffer[((y + 1) * width + (x + 1)) as usize] += err * 1.0 / 16.0;
                }
            }
        }
    }

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let val = buffer[idx].clamp(0.0, 255.0) as u8;
            img.put_pixel(x, y, Luma([val]));
        }
    }
}
