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

use vgi::Worker;

/// Worker version string, surfaced by `barcode_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
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

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    worker.run();
}
