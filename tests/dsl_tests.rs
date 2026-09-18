use base64::Engine;
use escpos_dsl::{DitherAlgo, compile, encode_image_tag, encode_image_tag_with_dither};
use image::{DynamicImage, ImageBuffer, Luma};

#[test]
fn test_acceptance_sample_receipt() {
    let dsl = r#"[C]<b>ORDER #045</b>
[C]--------------------------------
[L]2<pos x="48"/>SHIRT<r>9.99</r>
[L]<pos x="48"/>+ Size: S
[L]1<pos x="48"/>HAT<r>24.99</r>
[C]--------------------------------
[R]<b>TOTAL: 34.98</b>
[C]<qrcode size='6'>https://example.com/order/045</qrcode>"#;

    let bytes = compile(dsl, 32).expect("Sample receipt compilation failed");
    assert!(!bytes.is_empty());

    // Check broken/forbidden commands NOT present
    // CR (0x0D)
    assert!(
        !bytes.contains(&0x0D),
        "Compiled bytes must not contain CR (0x0D)"
    );

    // ESC 3 (0x1B 0x33)
    assert!(
        !contains_subslice(&bytes, &[0x1B, 0x33]),
        "Compiled bytes must not contain ESC 3"
    );

    // ESC D (0x1B 0x44)
    assert!(
        !contains_subslice(&bytes, &[0x1B, 0x44]),
        "Compiled bytes must not contain ESC D"
    );

    // ESC \ (0x1B 0x5C)
    assert!(
        !contains_subslice(&bytes, &[0x1B, 0x5C]),
        "Compiled bytes must not contain ESC \\"
    );

    // GS L (0x1D 0x4C)
    assert!(
        !contains_subslice(&bytes, &[0x1D, 0x4C]),
        "Compiled bytes must not contain GS L"
    );

    // Must contain ESC $ 48 (0x1B 0x24 0x30 0x00)
    assert!(
        contains_subslice(&bytes, &[0x1B, 0x24, 0x30, 0x00]),
        "Must contain ESC $ 48"
    );

    // Must contain ESC @ (0x1B 0x40 init)
    assert!(
        contains_subslice(&bytes, &[0x1B, 0x40]),
        "Must contain ESC @ init"
    );
}

#[test]
fn test_autospace_tag() {
    // 32 chars max. "2 SHIRT" = 7 chars, "9.99" = 4 chars.
    // Needed spaces = 32 - 7 - 4 = 21 spaces.
    let dsl = "[L]2 SHIRT<autospace/>9.99";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected_padding = " ".repeat(21);
    assert!(
        text.contains(&format!("2 SHIRT{}9.99", expected_padding)),
        "Must contain 21 whitespace padding chars"
    );
}

#[test]
fn test_autospace_explicit_max() {
    // "Key" = 3 chars, ": Value" = 7 chars. autospace max="15" -> 15 - 3 - 7 = 5 spaces.
    let dsl = "[L]Key<autospace max='15'/>: Value";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = format!("Key{}: Value", " ".repeat(5));
    assert!(text.contains(&expected), "Must pad to 15 chars");
}

#[test]
fn test_text_sizing_tags() {
    let dsl = "[C]<big>BIG</big><dh>DH</dh><dw>DW</dw><size w='3' h='3'>S3</size>";
    let bytes = compile(dsl, 32).expect("Compilation failed");

    assert!(
        contains_subslice(&bytes, &[0x1D, 0x21, 0x11]),
        "GS ! 17 for <big>"
    );
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x21, 0x01]),
        "GS ! 1 for <dh>"
    );
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x21, 0x10]),
        "GS ! 16 for <dw>"
    );
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x21, 0x22]),
        "GS ! 0x22 for <size w=3 h=3>"
    );
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x21, 0x00]),
        "GS ! 0 for reset size"
    );
}

#[test]
fn test_comments() {
    let dsl = "[L]Hello\n# This is a comment\n  # Indented comment\nWorld";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
    assert!(!text.contains("This is a comment"));
    assert!(!text.contains("Indented comment"));
}

#[test]
fn test_unclosed_tags() {
    let dsl = "[L]<b>Unclosed bold\nNormal text";
    let bytes = compile(dsl, 32).expect("Compilation failed");

    // Bold should turn on for line 1 and reset at line 2
    assert!(contains_subslice(&bytes, &[0x1B, 0x45, 0x01]), "Bold on");
    assert!(
        contains_subslice(&bytes, &[0x1B, 0x45, 0x00]),
        "Bold off at line end"
    );
}

#[test]
fn test_unknown_tags() {
    let dsl = "[L]Hello <unknown_tag_foo>World";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    assert!(text.contains("Hello World"));
    assert!(!text.contains("unknown_tag_foo"));
}

#[test]
fn test_empty_input() {
    let bytes = compile("", 32).expect("Compilation failed");
    assert!(!bytes.is_empty()); // Contains printer init sequence
}

#[test]
fn test_long_text_wrapping() {
    let long_line = "A".repeat(100);
    let dsl = format!("[L]{}", long_line);
    let bytes = compile(&dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    assert!(text.contains(&long_line));
}

#[test]
fn test_pos_two_byte_encoding() {
    // pos x="300" => 300 = 0x012C -> low byte 0x2C (44), high byte 0x01 (1)
    let dsl = r#"<pos x="300"/>"#;
    let bytes = compile(dsl, 32).expect("Compilation failed");

    assert!(
        contains_subslice(&bytes, &[0x1B, 0x24, 0x2C, 0x01]),
        "ESC $ 300 byte sequence"
    );
}

#[test]
fn test_qrcode_large_data() {
    let long_data = "A".repeat(300);
    let dsl = format!("<qrcode size='6'>{}</qrcode>", long_data);
    let bytes = compile(&dsl, 32).expect("Compilation failed");

    // QR code command GS ( k (0x1D 0x28 0x6B)
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x28, 0x6B]),
        "Must contain GS ( k"
    );
}

#[test]
fn test_r_span_restore_non_default_alignment() {
    // [C] line base alignment is CENTER. <r> goes RIGHT, </r> reverts to CENTER (ESC a 1), NOT LEFT.
    let dsl = "[C]<r>Right</r>";
    let bytes = compile(dsl, 32).expect("Compilation failed");

    // CENTER = ESC a 1 (0x1B 0x61 0x01)
    // RIGHT = ESC a 2 (0x1B 0x61 0x02)
    assert!(
        contains_subslice(&bytes, &[0x1B, 0x61, 0x01]),
        "Center alignment"
    );
    assert!(
        contains_subslice(&bytes, &[0x1B, 0x61, 0x02]),
        "Right alignment"
    );
}

#[test]
fn test_r_align_alias_tag() {
    let dsl = "[L]<r-align>RightText</r-align>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    assert!(
        contains_subslice(&bytes, &[0x1B, 0x61, 0x02]),
        "Right alignment via <r-align>"
    );
}

#[test]
fn test_lenna_file_loading() {
    if std::path::Path::new("tmp/lenna.tiff").exists() {
        let bytes = std::fs::read("tmp/lenna.tiff").unwrap();
        let dyn_img = image::load_from_memory(&bytes).expect("Failed to load lenna.tiff");
        let (w, h) = (dyn_img.width(), dyn_img.height());
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let tag = format!(r#"<img w="{}" h="{}">{}</img>"#, w, h, b64);

        let compiled = compile(&tag, 32).expect("Failed to compile lenna.tiff tag");
        assert!(
            contains_subslice(&compiled, &[0x1D, 0x76, 0x30]),
            "Must emit GS v 0"
        );
    }
}

#[test]
fn test_img_auto_resize() {
    let mut img_buf = ImageBuffer::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            let val = if (x + y) % 2 == 0 { 255 } else { 0 };
            img_buf.put_pixel(x, y, Luma([val]));
        }
    }

    let dyn_img = DynamicImage::ImageLuma8(img_buf);
    let tag_encoded = encode_image_tag(&dyn_img).unwrap();
    let tag_mismatched = tag_encoded.replace(r#"w="4" h="4""#, r#"w="8" h="8""#);

    let bytes = compile(&tag_mismatched, 32).expect("Auto-resize compilation failed");
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x76, 0x30]),
        "Must emit GS v 0"
    );
}

#[test]
fn test_img_roundtrip_fixture() {
    // Create 4x4 checkerboard image fixture
    let mut img_buf = ImageBuffer::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            let val = if (x + y) % 2 == 0 { 255 } else { 0 };
            img_buf.put_pixel(x, y, Luma([val]));
        }
    }
    let dyn_img = DynamicImage::ImageLuma8(img_buf);

    let tag = encode_image_tag(&dyn_img).expect("Failed to encode image tag");
    assert!(tag.starts_with(r#"<img w="4" h="4""#));

    let bytes = compile(&tag, 32).expect("Failed to compile image tag");
    // GS v 0 raster image command (0x1D 0x76 0x30)
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x76, 0x30]),
        "Must emit GS v 0"
    );
}

#[test]
fn test_img_dither_attribute() {
    let mut img_buf = ImageBuffer::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            let val = if (x + y) % 2 == 0 { 255 } else { 0 };
            img_buf.put_pixel(x, y, Luma([val]));
        }
    }
    let dyn_img = DynamicImage::ImageLuma8(img_buf);
    let tag = encode_image_tag_with_dither(&dyn_img, Some(DitherAlgo::Bayer8x8)).unwrap();

    let bytes = compile(&tag, 32).expect("Dither attribute compilation failed");
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x76, 0x30]),
        "Must emit GS v 0"
    );
}

#[test]
fn test_img_transparency_blending() {
    // 2x2 image, one pixel fully transparent, others opaque
    let mut img_buf = image::RgbaImage::new(2, 2);
    // top-left opaque red
    img_buf.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    // top-right fully transparent
    img_buf.put_pixel(1, 0, image::Rgba([0, 0, 0, 0]));
    // bottom-left opaque green
    img_buf.put_pixel(0, 1, image::Rgba([0, 255, 0, 255]));
    // bottom-right opaque black
    img_buf.put_pixel(1, 1, image::Rgba([0, 0, 0, 255]));

    let dyn_img = DynamicImage::ImageRgba8(img_buf);
    let tag = encode_image_tag(&dyn_img).unwrap();

    let bytes = compile(&tag, 32).expect("Compilation of transparent image failed");
    assert!(
        contains_subslice(&bytes, &[0x1D, 0x76, 0x30]),
        "Must emit GS v 0"
    );
}

#[test]
fn test_lpad_container() {
    let dsl = "[L]<l-pad len='10'>ABC</l-pad>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = format!("{}ABC", " ".repeat(7));
    assert!(
        text.contains(&expected),
        "Expected 7 spaces left padding before ABC"
    );
}

#[test]
fn test_lpad_custom_char() {
    let dsl = "[L]<l-pad len='8' ch='0'>123</l-pad>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    assert!(
        text.contains("00000123"),
        "Expected 5 leading zeros before 123"
    );
}

#[test]
fn test_rpad_container() {
    let dsl = "[L]<r-pad len='10'>ABC</r-pad>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = format!("ABC{}", " ".repeat(7));
    assert!(
        text.contains(&expected),
        "Expected ABC followed by 7 spaces right padding"
    );
}

#[test]
fn test_rpad_custom_char() {
    let dsl = "[L]<r-pad len='12' ch='.'>TOTAL</r-pad>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    assert!(
        text.contains("TOTAL......."),
        "Expected TOTAL followed by 7 dots"
    );
}

#[test]
fn test_pad_self_closing() {
    let dsl = "[L]<l-pad len='5'/>ABC";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    assert!(text.contains("     ABC"), "Expected 5 spaces before ABC");
}

#[test]
fn test_autospace_with_padding() {
    // Line total width = 32.
    // "<r-pad len='5'>2</r-pad>" = 5 chars ("2    ")
    // "Number 9" = 8 chars
    // "$5.98" = 5 chars
    // Total occupied = 5 + 8 + 5 = 18 chars.
    // autospace should output 32 - 18 = 14 spaces.
    let dsl = "[L]<r-pad len='5'>2</r-pad>Number 9<autospace/>$5.98";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = format!("2    Number 9{}$5.98", " ".repeat(14));
    assert!(
        text.contains(&expected),
        "Expected autospace to account for r-pad padding"
    );
}

#[test]
fn test_autospace_with_lpad() {
    // Line total width = 32.
    // "<l-pad len='10'>ABC</l-pad>" = 10 chars ("       ABC")
    // "$10.00" = 6 chars
    // Total occupied = 10 + 6 = 16 chars.
    // autospace should output 32 - 16 = 16 spaces.
    let dsl = "[L]<l-pad len='10'>ABC</l-pad><autospace/>$10.00";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = format!("       ABC{}$10.00", " ".repeat(16));
    assert!(
        text.contains(&expected),
        "Expected autospace to account for l-pad padding"
    );
}

#[test]
fn test_hr_default_length() {
    let dsl = "[C]<hr/>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = "-".repeat(32);
    assert!(
        text.contains(&expected),
        "Expected 32 default dashes for <hr/>"
    );
}

#[test]
fn test_hr_custom_length() {
    let dsl = "[C]<hr len='16'/>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = "-".repeat(16);
    assert!(
        text.contains(&expected),
        "Expected 16 dashes for <hr len='16'/>"
    );
}

#[test]
fn test_hr_custom_char() {
    let dsl = "[C]<hr ch='='/>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = "=".repeat(32);
    assert!(
        text.contains(&expected),
        "Expected 32 equals signs for <hr ch='='/>"
    );
}

#[test]
fn test_hr_custom_len_and_char() {
    let dsl = "[C]<hr len='20' ch='*'/>";
    let bytes = compile(dsl, 32).expect("Compilation failed");
    let text = String::from_utf8_lossy(&bytes);

    let expected = "*".repeat(20);
    assert!(
        text.contains(&expected),
        "Expected 20 asterisks for <hr len='20' ch='*'/>"
    );
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    find_subslice(haystack, needle).is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
