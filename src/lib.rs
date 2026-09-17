use boltffi::*;

pub mod compiler;
pub mod driver;
pub mod image_helper;

pub use compiler::{CompileError, DEFAULT_MAX_CHARS_PER_LINE, compile};
pub use image_helper::{DitherAlgo, encode_image_tag, encode_image_tag_with_dither};

#[data]
pub struct ReceiptBytes {
    pub bytes: Vec<u8>,
}

#[export]
pub fn compile_dsl(dsl: String, max_chars_per_line: u8) -> Result<ReceiptBytes, String> {
    compile(&dsl, max_chars_per_line)
        .map(|bytes| ReceiptBytes { bytes })
        .map_err(|e| e.to_string())
}

#[export]
pub fn compile_dsl_raw(dsl: String, max_chars_per_line: u8) -> Result<Vec<u8>, String> {
    compile(&dsl, max_chars_per_line).map_err(|e| e.to_string())
}
