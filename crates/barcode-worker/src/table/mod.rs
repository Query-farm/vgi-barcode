//! Table functions exposed by the barcode worker, registered under `barcode.main`.

mod decode_barcodes;
pub mod formats;

use vgi::Worker;

/// Register the table functions that take arguments (so they are not surfaced as
/// zero-argument catalog tables). `barcode_formats` is parameterless and is
/// instead exposed as a catalog *table* via [`crate::catalog_metadata`] (using
/// `CatTable::with_function`, which auto-registers its scan function), so
/// `SELECT * FROM barcode.main.barcode_formats` works (VGI311).
pub fn register(worker: &mut Worker) {
    worker.register_table(decode_barcodes::DecodeBarcodes);
}
