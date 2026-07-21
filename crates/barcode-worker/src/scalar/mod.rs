//! Scalar functions exposed by the barcode worker, registered under `barcode.main`.

mod decode;
mod generate;

use vgi::Worker;

/// Register every scalar function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_scalar(decode::DecodeBarcode::text());
    worker.register_scalar(decode::DecodeBarcode::format());
    // Arity overloads: DuckDB scalar functions are positional-only, so the
    // optional size_px is registered as a separate overload rather than a named
    // argument.
    worker.register_scalar(generate::GenerateQr::plain());
    worker.register_scalar(generate::GenerateQr::with_size());
    worker.register_scalar(generate::GenerateBarcode::plain());
    worker.register_scalar(generate::GenerateBarcode::with_size());
}
