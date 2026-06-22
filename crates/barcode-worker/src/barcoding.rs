//! Pure barcode logic — no Arrow, no VGI. Everything here works on `&[u8]` image
//! blobs / `&str` text and plain Rust types so it can be unit-tested directly.
//! The Arrow adapters in `scalar/` and `table/` call into these functions.
//!
//! ## Robustness / security
//!
//! Image bytes are UNTRUSTED. Every entry point that decodes an image first
//! reads only the header (via `image`'s reader) and rejects absurd dimensions
//! *before* materializing pixels, so a hostile "image bomb" cannot exhaust
//! memory. All decode/parse errors are returned as a plain [`BarcodeError`] —
//! never a panic — so a single bad blob never crashes the worker.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Luma};
use rxing::{BarcodeFormat, DecodeHints, Exceptions, MultiFormatWriter, RXingResult, Writer};

/// A processing error, rendered to a string for the worker to surface to DuckDB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarcodeError(pub String);

impl std::fmt::Display for BarcodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BarcodeError {}

type Result<T> = std::result::Result<T, BarcodeError>;

fn err(msg: impl Into<String>) -> BarcodeError {
    BarcodeError(msg.into())
}

/// Upper bound on either image dimension. A barcode photo realistically never
/// exceeds this; anything larger is treated as hostile and rejected at the
/// header stage before any pixels are allocated.
pub const MAX_DIMENSION: u32 = 20_000;

/// Upper bound on total pixel count (width × height). Guards against a small-ish
/// but still pathological aspect ratio that slips under [`MAX_DIMENSION`].
pub const MAX_PIXELS: u64 = 100_000_000; // 100 megapixels

/// Largest barcode image we will *generate* (per side, in pixels).
pub const MAX_GENERATE_PX: u32 = 4_096;
/// Default side length (pixels) for generated barcode images.
pub const DEFAULT_GENERATE_PX: u32 = 256;

/// One decoded barcode: its canonical ZXing format name and decoded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// Canonical ZXing format name, e.g. `QR_CODE`, `EAN_13`, `CODE_128`.
    pub format: String,
    /// The decoded payload text.
    pub text: String,
}

/// Decode an untrusted image blob to grayscale (luma8), guarding dimensions
/// before materializing pixels. Returns the luma buffer plus width/height.
fn decode_luma(blob: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    if blob.is_empty() {
        return Err(err("empty image blob"));
    }
    // Guess the container format and read the header *without* decoding pixels,
    // so we can reject an absurdly-sized image up front.
    let reader = ImageReader::new(Cursor::new(blob))
        .with_guessed_format()
        .map_err(|e| err(format!("could not read image header: {e}")))?;
    if reader.format().is_none() {
        return Err(err("unrecognized image format"));
    }
    if let Ok((w, h)) = reader.into_dimensions() {
        guard_dimensions(w, h)?;
    }
    // Re-read for the actual decode (the reader above was consumed by the
    // dimension probe). The header was valid, so this is the real decode.
    let img: DynamicImage = ImageReader::new(Cursor::new(blob))
        .with_guessed_format()
        .map_err(|e| err(format!("could not read image header: {e}")))?
        .decode()
        .map_err(|e| err(format!("could not decode image: {e}")))?;
    // Belt and suspenders: re-check the decoded dimensions.
    guard_dimensions(img.width(), img.height())?;
    let luma = img.to_luma8();
    let (w, h) = (luma.width(), luma.height());
    Ok((luma.into_raw(), w, h))
}

/// Reject images whose dimensions exceed our safety bounds.
fn guard_dimensions(w: u32, h: u32) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(err("image has a zero dimension"));
    }
    if w > MAX_DIMENSION || h > MAX_DIMENSION {
        return Err(err(format!(
            "image dimension {w}x{h} exceeds the {MAX_DIMENSION}px limit"
        )));
    }
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(err(format!(
            "image has {} pixels, exceeding the {MAX_PIXELS} pixel limit",
            (w as u64) * (h as u64)
        )));
    }
    Ok(())
}

/// Map an rxing [`BarcodeFormat`] to its canonical ZXing string name (the form
/// callers expect: `QR_CODE`, `EAN_13`, …). rxing's own `Display` yields a
/// lowercase/spaced variant, so we map explicitly.
pub fn format_name(f: &BarcodeFormat) -> &'static str {
    match f {
        BarcodeFormat::AZTEC => "AZTEC",
        BarcodeFormat::CODABAR => "CODABAR",
        BarcodeFormat::CODE_39 => "CODE_39",
        BarcodeFormat::CODE_93 => "CODE_93",
        BarcodeFormat::CODE_128 => "CODE_128",
        BarcodeFormat::DATA_MATRIX => "DATA_MATRIX",
        BarcodeFormat::EAN_8 => "EAN_8",
        BarcodeFormat::EAN_13 => "EAN_13",
        BarcodeFormat::ITF => "ITF",
        BarcodeFormat::MAXICODE => "MAXICODE",
        BarcodeFormat::PDF_417 => "PDF_417",
        BarcodeFormat::QR_CODE => "QR_CODE",
        BarcodeFormat::MICRO_QR_CODE => "MICRO_QR_CODE",
        BarcodeFormat::RECTANGULAR_MICRO_QR_CODE => "RECTANGULAR_MICRO_QR_CODE",
        BarcodeFormat::RSS_14 => "RSS_14",
        BarcodeFormat::RSS_EXPANDED => "RSS_EXPANDED",
        BarcodeFormat::TELEPEN => "TELEPEN",
        BarcodeFormat::UPC_A => "UPC_A",
        BarcodeFormat::UPC_E => "UPC_E",
        BarcodeFormat::UPC_EAN_EXTENSION => "UPC_EAN_EXTENSION",
        BarcodeFormat::DXFilmEdge => "DX_FILM_EDGE",
        _ => "UNKNOWN",
    }
}

/// The canonical format names we advertise as decodable / generatable, for the
/// `barcode_formats()` discovery table. (DX-film-edge / micro-QR / maxicode are
/// omitted as they are niche / not round-trippable through generation.)
pub fn supported_formats() -> &'static [&'static str] {
    &[
        "QR_CODE",
        "EAN_8",
        "EAN_13",
        "UPC_A",
        "UPC_E",
        "CODE_39",
        "CODE_93",
        "CODE_128",
        "CODABAR",
        "ITF",
        "DATA_MATRIX",
        "PDF_417",
        "AZTEC",
    ]
}

fn to_decoded(r: &RXingResult) -> Decoded {
    Decoded {
        format: format_name(r.getBarcodeFormat()).to_string(),
        text: r.getText().to_string(),
    }
}

/// Decode the FIRST barcode found in an image blob. Returns `Ok(None)` when the
/// image is valid but carries no detectable barcode; `Err` only for a blob that
/// is not a decodable image at all (caller may still map that to NULL).
pub fn decode_first(blob: &[u8]) -> Result<Option<Decoded>> {
    let (luma, w, h) = decode_luma(blob)?;
    let mut hints = DecodeHints::default();
    match rxing::helpers::detect_in_luma_with_hints(luma, w, h, None, &mut hints) {
        Ok(r) => Ok(Some(to_decoded(&r))),
        Err(Exceptions::NotFoundException(_)) => Ok(None),
        // Any other rxing error (e.g. checksum/format) also means "nothing
        // usable decoded" — surface as None, never a panic.
        Err(_) => Ok(None),
    }
}

/// Decode ALL barcodes found in an image blob (multi-detect). Returns an empty
/// vector when none are found. `Err` only for an undecodable image.
pub fn decode_all(blob: &[u8]) -> Result<Vec<Decoded>> {
    let (luma, w, h) = decode_luma(blob)?;
    let mut hints = DecodeHints::default();
    match rxing::helpers::detect_multiple_in_luma_with_hints(luma, w, h, &mut hints) {
        Ok(results) => Ok(results.iter().map(to_decoded).collect()),
        Err(Exceptions::NotFoundException(_)) => Ok(Vec::new()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Parse a user-supplied format string into a `BarcodeFormat`, erroring clearly
/// on an unknown name. Accepts canonical (`EAN_13`), spaced (`ean 13`) and
/// compact (`ean13`) spellings (rxing's `From<&str>` is permissive).
pub fn parse_format(s: &str) -> Result<BarcodeFormat> {
    let f = BarcodeFormat::from(s.trim());
    if f == BarcodeFormat::UNSUPORTED_FORMAT {
        return Err(err(format!(
            "unknown barcode format '{s}'; supported: {}",
            supported_formats().join(", ")
        )));
    }
    Ok(f)
}

/// Encode `text` as a QR code, rendered to a square PNG of `size_px` per side.
pub fn generate_qr(text: &str, size_px: u32) -> Result<Vec<u8>> {
    generate(text, BarcodeFormat::QR_CODE, size_px, size_px)
}

/// Encode `text` in `format`, rendered to a PNG. For 2D symbologies a square
/// image of `size_px` is produced; for 1D symbologies the width is `size_px` and
/// the height a readable fraction of it.
pub fn generate_barcode(text: &str, format: BarcodeFormat, size_px: u32) -> Result<Vec<u8>> {
    let (w, h) = if is_two_d(&format) {
        (size_px, size_px)
    } else {
        // 1D barcodes are wide and short; give a 4:1 aspect with a sane floor.
        (size_px, (size_px / 4).max(64))
    };
    generate(text, format, w, h)
}

fn is_two_d(f: &BarcodeFormat) -> bool {
    matches!(
        f,
        BarcodeFormat::QR_CODE
            | BarcodeFormat::MICRO_QR_CODE
            | BarcodeFormat::RECTANGULAR_MICRO_QR_CODE
            | BarcodeFormat::DATA_MATRIX
            | BarcodeFormat::PDF_417
            | BarcodeFormat::AZTEC
            | BarcodeFormat::MAXICODE
    )
}

/// Validate a requested generated-image side length.
pub fn check_size(size_px: i64) -> Result<u32> {
    if size_px <= 0 {
        return Err(err(format!("size_px must be positive, got {size_px}")));
    }
    if size_px > i64::from(MAX_GENERATE_PX) {
        return Err(err(format!(
            "size_px {size_px} exceeds the {MAX_GENERATE_PX}px generation limit"
        )));
    }
    Ok(size_px as u32)
}

/// Core encode: text → `format` → a `width`×`height` PNG (black on white).
fn generate(text: &str, format: BarcodeFormat, width: u32, height: u32) -> Result<Vec<u8>> {
    if text.is_empty() {
        return Err(err("cannot encode empty text"));
    }
    let writer = MultiFormatWriter;
    let matrix = writer
        .encode(text, &format, width as i32, height as i32)
        .map_err(|e| err(format!("could not encode {}: {e}", format_name(&format))))?;

    let (mw, mh) = (matrix.width(), matrix.height());
    if mw == 0 || mh == 0 {
        return Err(err("encoder produced an empty matrix"));
    }
    // Render the bit matrix: set bit → black (0), clear → white (255).
    let mut img = image::GrayImage::new(mw, mh);
    for y in 0..mh {
        for x in 0..mw {
            let v: u8 = if matrix.get(x, y) { 0 } else { 255 };
            img.put_pixel(x, y, Luma([v]));
        }
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(img)
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| err(format!("could not encode PNG: {e}")))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_roundtrips() {
        let png = generate_qr("hello world", 200).unwrap();
        let d = decode_first(&png).unwrap().expect("a barcode");
        assert_eq!(d.format, "QR_CODE");
        assert_eq!(d.text, "hello world");
    }

    #[test]
    fn qr_default_size_roundtrips() {
        let png = generate_qr("hi", DEFAULT_GENERATE_PX).unwrap();
        let d = decode_first(&png).unwrap().unwrap();
        assert_eq!((d.format.as_str(), d.text.as_str()), ("QR_CODE", "hi"));
    }

    #[test]
    fn ean13_roundtrips() {
        // A valid EAN-13 payload (12 digits + check digit computed by encoder
        // requires 13 digits including a valid checksum). 5901234123457 is the
        // canonical ZXing EAN-13 example.
        let png = generate_barcode("5901234123457", parse_format("EAN_13").unwrap(), 400).unwrap();
        let d = decode_first(&png).unwrap().expect("a barcode");
        assert_eq!(d.format, "EAN_13");
        assert_eq!(d.text, "5901234123457");
    }

    #[test]
    fn code128_roundtrips() {
        let png = generate_barcode("ABC-123", parse_format("CODE_128").unwrap(), 400).unwrap();
        let d = decode_first(&png).unwrap().expect("a barcode");
        assert_eq!(d.format, "CODE_128");
        assert_eq!(d.text, "ABC-123");
    }

    #[test]
    fn decode_all_finds_the_qr() {
        let png = generate_qr("multi", 220).unwrap();
        let all = decode_all(&png).unwrap();
        assert!(!all.is_empty());
        assert!(all
            .iter()
            .any(|d| d.text == "multi" && d.format == "QR_CODE"));
    }

    #[test]
    fn garbage_bytes_are_not_a_panic_and_error() {
        // Not a decodable image at all → Err (the scalar layer maps to NULL).
        assert!(decode_first(b"not an image at all").is_err());
        assert!(decode_all(b"not an image at all").is_err());
    }

    #[test]
    fn empty_bytes_error() {
        assert!(decode_first(b"").is_err());
        assert!(decode_all(b"").is_err());
    }

    #[test]
    fn valid_image_without_barcode_is_none_not_error() {
        // A plain white PNG decodes fine but has no barcode.
        let mut img = image::GrayImage::new(64, 64);
        for p in img.pixels_mut() {
            *p = Luma([255]);
        }
        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(img)
            .write_to(&mut buf, ImageFormat::Png)
            .unwrap();
        let blob = buf.into_inner();
        assert_eq!(decode_first(&blob).unwrap(), None);
        assert!(decode_all(&blob).unwrap().is_empty());
    }

    #[test]
    fn oversized_dimension_is_rejected() {
        // Directly exercise the guard (constructing a real 21000px image would
        // be wasteful); the guard is what protects the decode path.
        assert!(guard_dimensions(MAX_DIMENSION + 1, 10).is_err());
        assert!(guard_dimensions(10, MAX_DIMENSION + 1).is_err());
        assert!(guard_dimensions(0, 10).is_err());
        // A pathological aspect ratio under the per-side cap but over the pixel
        // budget is also rejected.
        assert!(guard_dimensions(19_000, 19_000).is_err());
        // A reasonable size passes.
        assert!(guard_dimensions(640, 480).is_ok());
    }

    #[test]
    fn unknown_format_errors() {
        assert!(parse_format("NOPE").is_err());
        assert!(parse_format("").is_err());
        assert!(parse_format("QR_CODE").is_ok());
        assert!(parse_format("qrcode").is_ok());
        assert!(parse_format("ean13").is_ok());
    }

    #[test]
    fn check_size_bounds() {
        assert!(check_size(0).is_err());
        assert!(check_size(-5).is_err());
        assert!(check_size(i64::from(MAX_GENERATE_PX) + 1).is_err());
        assert_eq!(check_size(256).unwrap(), 256);
    }

    #[test]
    fn supported_formats_nonempty_and_parseable() {
        assert!(!supported_formats().is_empty());
        for f in supported_formats() {
            assert!(parse_format(f).is_ok(), "format {f} must parse");
        }
    }
}
