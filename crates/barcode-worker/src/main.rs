//! The `barcode` VGI worker.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'barcode' (TYPE vgi, LOCATION '…')`). It brings barcode / QR-code
//! decoding and generation to SQL under the catalog `barcode`, schema `main`:
//!
//! ```sql
//! ATTACH 'barcode' (TYPE vgi, LOCATION './target/release/barcode-worker');
//! SET search_path = 'barcode.main';
//!
//! SELECT decode_barcode(img)  FROM photos;          -- first barcode's text
//! SELECT barcode_format(img)  FROM photos;          -- e.g. 'QR_CODE'
//! SELECT generate_qr('hi');                          -- PNG BLOB
//! SELECT * FROM decode_barcodes(read_blob('x.png')); -- all codes in one image
//! SELECT * FROM barcode_formats();                   -- supported formats
//! ```
//!
//! Pure barcode logic (decode/encode, dimension guards) lives in `barcoding.rs`;
//! the `scalar/` and `table/` modules are thin Arrow adapters over it.

mod arrow_io;
mod barcoding;
mod scalar;
mod table;

use vgi::catalog::{CatSchema, CatalogModel};
use vgi::Worker;

/// Worker version string, surfaced by `barcode_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Catalog + schema metadata (description, provenance) surfaced to DuckDB and the
/// `vgi-lint` metadata-quality linter. The function objects themselves are served
/// from the registered scalars/tables; this only adds catalog/schema-level
/// comments and tags.
fn catalog_metadata(name: &str) -> CatalogModel {
    CatalogModel {
        name: name.to_string(),
        comment: Some("Barcode and QR-code decoding and generation over Apache Arrow.".to_string()),
        tags: vec![
            (
                "vgi.description_llm".to_string(),
                "Decode barcodes and QR codes out of image BLOBs (PNG/JPEG/GIF/BMP/WebP) and \
                 generate barcode/QR PNGs from text. Read the text and format name of the first \
                 symbol in an image, fan one image out into every symbol it contains, encode text \
                 as a QR code or a named symbology (EAN_13, UPC_A, CODE_128, CODE_39, ITF, \
                 CODABAR, DATA_MATRIX, PDF_417, AZTEC, …), and list the supported formats. Use \
                 for reading product/QR codes from photos and for producing scannable barcode \
                 images in SQL."
                    .to_string(),
            ),
            (
                "vgi.description_md".to_string(),
                "# barcode\n\nBarcode / QR-code decode and generation over Apache Arrow, powered \
                 by the Rust ZXing port (`rxing`).\n\nScalars: `decode_barcode`, `barcode_format`, \
                 `generate_qr`, `generate_barcode`, `barcode_version`. Tables: `decode_barcodes`, \
                 `barcode_formats`."
                    .to_string(),
            ),
            ("vgi.author".to_string(), "Query.Farm".to_string()),
            (
                "vgi.copyright".to_string(),
                "Copyright 2026 Query Farm LLC - https://query.farm".to_string(),
            ),
            ("vgi.license".to_string(), "MIT".to_string()),
            (
                "vgi.support_contact".to_string(),
                "https://github.com/Query-farm/vgi-barcode/issues".to_string(),
            ),
            (
                "vgi.support_policy_url".to_string(),
                "https://github.com/Query-farm/vgi-barcode/blob/main/README.md".to_string(),
            ),
        ],
        source_url: Some("https://github.com/Query-farm/vgi-barcode".to_string()),
        schemas: vec![CatSchema {
            name: "main".to_string(),
            comment: Some("Barcode / QR-code decode and generation functions.".to_string()),
            tags: vec![
                (
                    "vgi.description_llm".to_string(),
                    "Barcode / QR-code decode and generation functions: read the text and format \
                     of barcodes in an image, fan an image out into all its symbols, encode text \
                     as a QR code or a named barcode symbology, and list supported formats."
                        .to_string(),
                ),
                (
                    "vgi.description_md".to_string(),
                    "Barcode / QR-code decode and generation functions over Apache Arrow."
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            tables: Vec::new(),
        }],
        ..Default::default()
    }
}

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    // The catalog name DuckDB sees in `ATTACH 'barcode' (TYPE vgi, …)`. Default
    // to `barcode`, but honor an explicit override so a test harness can rename.
    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "barcode");
    }
    let catalog_name =
        std::env::var("VGI_WORKER_CATALOG_NAME").unwrap_or_else(|_| "barcode".to_string());

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    worker.set_catalog(catalog_metadata(&catalog_name));
    worker.run();
}
