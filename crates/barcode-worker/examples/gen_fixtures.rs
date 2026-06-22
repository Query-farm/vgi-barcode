//! Generate the committed SQL-test fixture images under `test/sql/data/`.
//!
//! Run with: `cargo run -p barcode-worker --example gen_fixtures`
//!
//! Produces:
//!   * `qr.png`      — a QR code encoding the text `vgi-barcode`.
//!   * `code128.png` — a Code 128 barcode encoding `ABC-12345`.
//!
//! These are checked in so the SQL E2E suite needs no build step to obtain
//! image bytes; the generator exists only to (re)produce them deterministically.
//!
//! This reuses the worker's own pure encode path (`barcoding`), so the fixtures
//! are exactly what the worker would decode.

// Pull in the worker's encode logic directly from source (examples can't import
// the binary crate's private modules otherwise). Only a couple of its functions
// are used here, so silence the unused-item warnings for the rest.
#[allow(dead_code)]
#[path = "../src/barcoding.rs"]
mod barcoding;

use std::path::Path;

fn write(path: &str, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    println!("wrote {path} ({} bytes)", bytes.len());
}

fn main() {
    let dir = Path::new("test/sql/data");
    std::fs::create_dir_all(dir).unwrap();

    let qr = barcoding::generate_qr("vgi-barcode", 256).unwrap();
    write("test/sql/data/qr.png", &qr);

    let fmt = barcoding::parse_format("CODE_128").unwrap();
    let code128 = barcoding::generate_barcode("ABC-12345", fmt, 400).unwrap();
    write("test/sql/data/code128.png", &code128);
}
