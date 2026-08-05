use boltffi::*;

pub mod compiler;
pub mod driver;
pub mod image_helper;

pub use compiler::{compile, CompileError};
pub use image_helper::encode_image_tag;

#[data]
pub struct ReceiptBytes {
    pub bytes: Vec<u8>,
}

#[export]
pub fn compile_dsl(dsl: String) -> Result<ReceiptBytes, String> {
    compile(&dsl)
        .map(|bytes| ReceiptBytes { bytes })
        .map_err(|e| e.to_string())
}

#[export]
pub fn compile_dsl_raw(dsl: String) -> Result<Vec<u8>, String> {
    compile(&dsl).map_err(|e| e.to_string())
}
