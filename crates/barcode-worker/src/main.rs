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
                "# Barcode & QR-Code Decoding and Generation for DuckDB\n\n\
                 ![ZXing logo](https://raw.githubusercontent.com/wiki/zxing/zxing/zxing-logo.png)\n\n\
                 Read and write barcodes and QR codes directly in SQL: decode QR, EAN, UPC, \
                 Code 128, Code 39, Data Matrix, PDF417, Aztec and more out of image BLOBs, and \
                 generate scannable barcode PNGs from text — all over Apache Arrow, with no \
                 external service.\n\n\
                 ## What it does and who it's for\n\n\
                 The `barcode` extension turns DuckDB into a complete barcode and QR-code toolkit. \
                 Data engineers, retail and logistics teams, and anyone holding a column of \
                 product photos or scanned documents can pull the encoded text and symbology out \
                 of images, or mint fresh barcode/QR images for labels, tickets, and links — \
                 entirely in SQL. Decoding is hardened against hostile or oversized input: a \
                 corrupt, empty, or giant image yields `NULL` (scalars) or zero rows (tables) \
                 instead of crashing the worker, so it is safe to run across untrusted data at \
                 scale.\n\n\
                 ## How it works\n\n\
                 Decoding and encoding are powered by \
                 [`rxing`](https://github.com/rxing-core/rxing), the maintained Rust port of the \
                 venerable [ZXing](https://github.com/zxing/zxing) (\"zebra crossing\") library. \
                 Input rasters (PNG, JPEG, GIF, BMP, WebP) are converted to grayscale and scanned \
                 for one or many symbols; text is encoded into a bit matrix and rendered to a PNG \
                 BLOB. Full API documentation for the underlying library lives at \
                 [docs.rs/rxing](https://docs.rs/rxing). The worker streams everything over Apache \
                 Arrow IPC, so results flow back into DuckDB as native columns without \
                 intermediate files.\n\n\
                 ## When to reach for it\n\n\
                 Reach for this worker to read product or QR codes out of a column of images, to \
                 fan a single image that holds several codes into one row per symbol, or to \
                 produce scannable barcode and QR PNGs for labels, tickets, and links — without \
                 leaving SQL or standing up an external service. Text encoding and image decoding \
                 round-trip cleanly, so you can both mint and re-read codes in one query. List the \
                 schema to discover the exact functions and their signatures; a quick round-trip \
                 looks like:\n\n\
                 ```sql\n\
                 SELECT decode_barcode(generate_qr('https://query.farm'));\n\
                 SELECT generate_barcode('5901234123457', 'EAN_13');\n\
                 ```"
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
            // VGI152: an analyst-task suite so `vgi-lint simulate` can measure how
            // well an agent actually drives this worker. Each task is a natural
            // prompt plus the canonical `reference_sql` that answers it. Every task
            // is deterministic and self-contained (it generates its own image), so
            // it needs no external fixtures.
            (
                "vgi.agent_test_tasks".to_string(),
                r#"[
  {"name": "worker_version", "prompt": "Which version of the barcode worker is currently attached? Return the single version string in a column named worker_version.", "reference_sql": "SELECT barcode.main.barcode_version() AS worker_version"},
  {"name": "count_supported_formats", "prompt": "How many distinct barcode and QR symbologies can this worker generate or decode? Return the total in a column named supported_formats.", "reference_sql": "SELECT COUNT(*) AS supported_formats FROM barcode.main.barcode_formats"},
  {"name": "list_supported_formats", "prompt": "List every barcode and QR symbology name this worker supports, ordered alphabetically, in a column named format.", "reference_sql": "SELECT format FROM barcode.main.barcode_formats ORDER BY format"},
  {"name": "decode_all_symbols", "prompt": "Encode the text 'hello world' as a QR code image, then decode every symbol found in that image. Return one row per symbol with its sequence index, format, and decoded text.", "reference_sql": "SELECT seq, format, text FROM barcode.main.decode_barcodes(barcode.main.generate_qr('hello world')) ORDER BY seq"},
  {"name": "decode_roundtrip", "prompt": "Encode the text 'hello world' as a QR code image, then decode that image back to a string. Return the decoded text in a column named decoded_text.", "reference_sql": "SELECT barcode.main.decode_barcode(barcode.main.generate_qr('hello world')) AS decoded_text"},
  {"name": "detect_symbology", "prompt": "Encode the text 'ping' as a QR code image, then report the barcode symbology detected in that image. Return it in a column named format.", "reference_sql": "SELECT barcode.main.barcode_format(barcode.main.generate_qr('ping')) AS format"},
  {"name": "generate_ean13_roundtrip", "prompt": "Generate an EAN-13 barcode image for the product code '5901234123457', then decode that image and report the symbology it was detected as. Return the symbology in a column named format.", "reference_sql": "SELECT barcode.main.barcode_format(barcode.main.generate_barcode('5901234123457', 'EAN_13')) AS format"}
]"#
                .to_string(),
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
                // VGI413: the schema's category registry — an ordered list of the
                // navigation sections its objects are grouped into. Each object
                // carries a `vgi.category` naming one of these.
                (
                    "vgi.categories".to_string(),
                    r#"[
  {"name": "decode", "description": "Read and recognize barcodes and QR codes from image data."},
  {"name": "generate", "description": "Create scannable barcode and QR-code images from text."},
  {"name": "reference", "description": "Discover the supported symbologies and inspect the worker build."}
]"#
                    .to_string(),
                ),
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
                    "# barcode.main\n\nThe primary schema of the barcode worker. It groups the \
                     barcode and QR-code capabilities into three areas: **decoding** symbols out \
                     of image data, **generating** scannable barcode/QR images from text, and \
                     **reference** lookups for the supported symbologies and the worker build.\n\n\
                     Decoding accepts raster images (PNG, JPEG, GIF, BMP, WebP) and is hardened \
                     against hostile or oversized input — unreadable data yields `NULL` or zero \
                     rows rather than an error, so it is safe to run over untrusted data at scale. \
                     Generation returns a PNG BLOB and raises a clear error only for an unknown \
                     symbology or an unencodable payload. List the schema to discover the exact \
                     functions and their signatures."
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
        // VGI413: place this table in one of the schema's declared categories.
        ("vgi.category".to_string(), "reference".to_string()),
        // VGI123 classifying tag so the table is findable by facet.
        ("domain".to_string(), "imaging".to_string()),
        (
            "vgi.example_queries".to_string(),
            r#"[
  {"description": "Count how many barcode/QR symbologies this worker supports.", "sql": "SELECT count(*) AS supported_formats FROM barcode.main.barcode_formats"},
  {"description": "List the Code-family symbologies (CODE_39/93/128) alphabetically.", "sql": "SELECT format FROM barcode.main.barcode_formats WHERE format LIKE 'CODE%' ORDER BY format"}
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
