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
mod meta;
mod scalar;
mod table;

use std::sync::Arc;

use vgi::catalog::{CatSchema, CatTable, CatalogModel};
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
                "vgi.title".to_string(),
                "Barcode & QR-Code Decode and Generation".to_string(),
            ),
            (
                "vgi.keywords".to_string(),
                meta::keywords_json(&[
                    "barcode",
                    "qr code",
                    "qr",
                    "decode",
                    "scan",
                    "generate",
                    "encode",
                    "ean",
                    "upc",
                    "code 128",
                    "code 39",
                    "data matrix",
                    "pdf417",
                    "aztec",
                    "symbology",
                    "zxing",
                    "rxing",
                    "png",
                    "image",
                ]),
            ),
            (
                "vgi.doc_llm".to_string(),
                "Decode barcodes and QR codes out of image BLOBs (PNG/JPEG/GIF/BMP/WebP) and \
                 generate barcode/QR PNGs from text. Read the text and format name of the first \
                 symbol in an image, fan one image out into every symbol it contains, encode text \
                 as a QR code or a named symbology (EAN_13, UPC_A, CODE_128, CODE_39, ITF, \
                 CODABAR, DATA_MATRIX, PDF_417, AZTEC, …), and list the supported formats. Decode \
                 paths are hardened against hostile/oversized images and return NULL or zero rows \
                 instead of erroring, so they are safe to run over untrusted data; encode paths \
                 raise a clear error for an unknown format or an unencodable payload. Use for \
                 reading product/QR codes from photos and for producing scannable barcode images \
                 in SQL."
                    .to_string(),
            ),
            (
                "vgi.doc_md".to_string(),
                "# barcode\n\nBarcode and QR-code **decode and generation** for DuckDB, over \
                 Apache Arrow IPC. The engine is the maintained Rust port of ZXing \
                 ([`rxing`](https://crates.io/crates/rxing)).\n\n## What you can do\n\n- **Read** \
                 the text and format of the first symbol in an image BLOB \
                 (`decode_barcode`, `barcode_format`).\n- **Fan out** one image into every symbol \
                 it contains, one row each (`decode_barcodes`).\n- **Generate** a QR code or a \
                 named symbology as a PNG BLOB (`generate_qr`, `generate_barcode`).\n- **Discover** \
                 the supported symbologies (`barcode_formats`) and the worker version \
                 (`barcode_version`).\n\n## Notes\n\nSupported input rasters: PNG, JPEG, GIF, BMP, \
                 WebP. Decoding never crashes on untrusted bytes — a bad, empty, or oversized \
                 image simply yields NULL (scalars) or no rows (tables)."
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
                ("vgi.title".to_string(), "Barcode — main".to_string()),
                (
                    "vgi.keywords".to_string(),
                    meta::keywords_json(&[
                        "barcode",
                        "qr code",
                        "decode_barcode",
                        "barcode_format",
                        "generate_qr",
                        "generate_barcode",
                        "decode_barcodes",
                        "barcode_formats",
                        "scan",
                        "encode",
                        "symbology",
                    ]),
                ),
                // VGI123 classifying tags (BARE keys: domain/category/topic) for faceting.
                ("domain".to_string(), "imaging".to_string()),
                ("category".to_string(), "barcode".to_string()),
                ("topic".to_string(), "decode-and-generate".to_string()),
                (
                    "vgi.doc_llm".to_string(),
                    "Barcode / QR-code decode and generation functions: read the text and format \
                     of barcodes in an image, fan an image out into all its symbols, encode text \
                     as a QR code or a named barcode symbology, and list supported formats. Decode \
                     functions return NULL or zero rows on unreadable/hostile input; generate \
                     functions error on an unknown format or unencodable payload."
                        .to_string(),
                ),
                (
                    "vgi.doc_md".to_string(),
                    "# barcode.main\n\nThe `main` schema of the barcode worker. It holds the \
                     scalar functions (`decode_barcode`, `barcode_format`, `generate_qr`, \
                     `generate_barcode`, `barcode_version`) and table functions \
                     (`decode_barcodes`, `barcode_formats`) for decoding and generating \
                     barcodes and QR codes over Apache Arrow.\n\nUse the scalars per-row over an \
                     image column, the `decode_barcodes` table to fan one image into all of its \
                     symbols, and `barcode_formats` to discover the supported symbology names."
                        .to_string(),
                ),
                // VGI506 representative example queries for the schema.
                (
                    "vgi.example_queries".to_string(),
                    "SELECT barcode.main.barcode_version();\n\
                     SELECT barcode.main.decode_barcode(barcode.main.generate_qr('hello world'));\n\
                     SELECT barcode.main.barcode_format(barcode.main.generate_qr('hello world'));\n\
                     SELECT barcode.main.generate_qr('https://query.farm');\n\
                     SELECT barcode.main.generate_barcode('5901234123457', 'EAN_13');\n\
                     SELECT * FROM barcode.main.barcode_formats();\n\
                     SELECT * FROM barcode.main.decode_barcodes(barcode.main.generate_qr('multi'));"
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            // Expose the parameterless `barcode_formats` table function as a
            // catalog TABLE so `SELECT * FROM barcode.main.barcode_formats`
            // works (VGI311). `with_function` stores the backing function and
            // `Worker::set_catalog` auto-registers its scan function, so no
            // separate `register_table` call is needed for it.
            tables: vec![barcode_formats_table()],
        }],
        ..Default::default()
    }
}

/// The catalog TABLE wrapping the parameterless `barcode_formats` table function
/// so `SELECT * FROM barcode.main.barcode_formats` works (VGI311). It carries the
/// full per-object metadata the strict linter expects (title/doc_llm/doc_md/
/// keywords/classifying tag/example queries), a documented column, and a single-
/// column primary key (`format`), since each symbology name appears once.
fn barcode_formats_table() -> CatTable {
    let mut t = CatTable::with_function(
        "barcode_formats",
        table::formats::output_schema(),
        Arc::new(table::formats::BarcodeFormats),
        Some("Supported barcode/QR symbology names, one per row.".to_string()),
        // ~20 ZXing symbologies — a small, fixed list.
        Some(table::formats::supported_format_count() as i64),
    );
    t.tags = vec![
        (
            "vgi.title".to_string(),
            "Supported Barcode Formats".to_string(),
        ),
        (
            "vgi.doc_llm".to_string(),
            "The catalog of barcode/QR symbology names this worker can generate or decode, one row \
             per format. Query it to discover which strings are valid for the `format` argument of \
             generate_barcode and which symbologies decode_barcode / decode_barcodes can recognize."
                .to_string(),
        ),
        (
            "vgi.doc_md".to_string(),
            "# barcode_formats\n\nThe supported barcode/QR **symbology names**, one per row in the \
             `format` column (canonical ZXing form, e.g. `QR_CODE`, `EAN_13`, `CODE_128`, \
             `DATA_MATRIX`, `PDF_417`, `AZTEC`). Use these strings as the `format` argument to \
             `generate_barcode`."
                .to_string(),
        ),
        (
            "vgi.keywords".to_string(),
            meta::keywords_json(&[
                "supported formats",
                "list formats",
                "barcode formats",
                "symbologies",
                "available formats",
                "barcode_formats",
                "discovery",
                "which barcodes",
            ]),
        ),
        // VGI123 classifying tag.
        ("category".to_string(), "barcode".to_string()),
        (
            "vgi.example_queries".to_string(),
            r#"[
  {"description": "List every supported barcode/QR symbology name.", "sql": "SELECT * FROM barcode.main.barcode_formats"},
  {"description": "List the supported symbology names alphabetically.", "sql": "SELECT format FROM barcode.main.barcode_formats ORDER BY format"}
]"#
            .to_string(),
        ),
    ];
    // `format` (column 0) uniquely identifies a row (each symbology name appears
    // once). `primary_key` is a list of key column-index groups.
    t.primary_key = vec![vec![0]];
    // A primary key column is implicitly NOT NULL; declare it so DuckDB and the
    // linter agree the key column is non-nullable.
    t.not_null = vec![0];
    t
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
