use escpos_dsl::compile_dsl_raw;

pub fn main() {
    let data = std::fs::read_to_string("./testfile/murniresto.epdsl").expect("File fail");
    let bytes = compile_dsl_raw(data).expect("compile fail");

    send_print_job("100.74.128.84:9100", &bytes).expect("Fail send print");

    // println!("{bytes}")

    // dbg!(bytes.len());
}

use std::{io::Write, net::TcpStream};

pub fn send_print_job(host: &str, data: &[u8]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(host)?;
    stream.write_all(data)?;
    stream.flush()?;
    Ok(())
}
