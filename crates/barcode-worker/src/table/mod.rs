//! Table functions exposed by the barcode worker, registered under `barcode.main`.

mod decode_barcodes;
mod formats;

use vgi::Worker;

/// Register every table function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_table(decode_barcodes::DecodeBarcodes);
    worker.register_table(formats::BarcodeFormats);
}
