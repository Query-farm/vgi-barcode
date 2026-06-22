//! Integration tests: black-box exercise of the worker's pure barcode logic
//! (generate → decode round-trips, multi-decode, and the hostile-input contract)
//! the same way the SQL E2E suite drives it, but without the Arrow/RPC layer.
//!
//! The pure logic lives in a private module of the binary crate, so we include
//! it by path — the same trick the fixture generator uses.

#[path = "../src/barcoding.rs"]
#[allow(dead_code)]
mod barcoding;

use barcoding::{decode_all, decode_first, generate_barcode, generate_qr, parse_format};

#[test]
fn qr_round_trips_text_and_format() {
    let png = generate_qr("integration-qr", 256).unwrap();
    let d = decode_first(&png).unwrap().expect("a decoded barcode");
    assert_eq!(d.format, "QR_CODE");
    assert_eq!(d.text, "integration-qr");
}

#[test]
fn ean13_round_trips() {
    let png = generate_barcode("5901234123457", parse_format("EAN_13").unwrap(), 400).unwrap();
    let d = decode_first(&png).unwrap().expect("a decoded barcode");
    assert_eq!(d.format, "EAN_13");
    assert_eq!(d.text, "5901234123457");
}

#[test]
fn code128_round_trips() {
    let png = generate_barcode("ABC-12345", parse_format("CODE_128").unwrap(), 400).unwrap();
    let d = decode_first(&png).unwrap().expect("a decoded barcode");
    assert_eq!(d.format, "CODE_128");
    assert_eq!(d.text, "ABC-12345");
}

#[test]
fn multi_decode_finds_the_code() {
    let png = generate_qr("multi", 240).unwrap();
    let all = decode_all(&png).unwrap();
    assert!(all
        .iter()
        .any(|d| d.text == "multi" && d.format == "QR_CODE"));
}

#[test]
fn garbage_and_empty_are_not_panics() {
    // Undecodable image → Err (the scalar/table layers map this to NULL / no
    // rows). The point is that it returns, never panicking.
    assert!(decode_first(b"not an image at all").is_err());
    assert!(decode_first(b"").is_err());
    assert!(decode_all(b"\x00\x01\x02\x03").is_err());
    assert!(decode_all(b"").is_err());
}

#[test]
fn invalid_generate_format_errors() {
    assert!(parse_format("DEFINITELY_NOT_A_FORMAT").is_err());
}
