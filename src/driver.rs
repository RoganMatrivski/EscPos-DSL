use escpos::driver::Driver;
use escpos::errors::PrinterError;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct MemoryDriver {
    pub bytes: Arc<Mutex<Vec<u8>>>,
}

impl MemoryDriver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_bytes(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }
}

impl Driver for MemoryDriver {
    fn write(&self, data: &[u8]) -> Result<(), PrinterError> {
        self.bytes.lock().unwrap().extend_from_slice(data);
        Ok(())
    }

    fn flush(&self) -> Result<(), PrinterError> {
        Ok(())
    }

    fn name(&self) -> String {
        "MemoryDriver".to_string()
    }

    fn read(&self, _data: &mut [u8]) -> Result<usize, PrinterError> {
        Ok(0)
    }
}
