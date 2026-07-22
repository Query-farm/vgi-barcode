//! The `barcode` VGI worker (native binary).
//!
//! A thin entrypoint: initialize logging, then build and run the shared
//! [`barcode_worker::build_worker`] over the native stdio/HTTP transport. All
//! function registration and catalog metadata live in the library crate so the
//! browser (`barcode-wasm`) build can serve the identical worker over a
//! SharedArrayBuffer byte channel.

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    barcode_worker::build_worker().run();
}
