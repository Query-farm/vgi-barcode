# CLAUDE.md — vgi-barcode

Contributor/agent notes. User-facing docs live in `README.md`; this is the
"how it's built and where the sharp edges are" companion.

## What this is

A [VGI](https://query.farm) worker (Rust, compiled binary) exposing barcode / QR
decode and generation to DuckDB/SQL over Arrow IPC. Built on the `vgi` crate
(crates.io), modeled on `vgi-image` / `vgi-fixedformat`. Catalog name `barcode`
(single `main` schema). Decode + encode via [`rxing`](https://crates.io/crates/rxing)
(the maintained Rust ZXing port).

## Layout

```
Cargo.toml                          workspace; pins vgi = "0.5.0", rxing, image
crates/barcode-worker/
  src/main.rs                       Worker::new(); registers scalars + tables
  src/barcoding.rs                  PURE logic (no Arrow): decode/encode + dimension guards + unit tests
  src/arrow_io.rs                   BLOB/VARCHAR cell reads + in-process scalar test harness
  src/scalar/{decode,generate,version,mod}.rs   thin Arrow scalar adapters
  src/table/{decode_barcodes,formats,mod}.rs    thin Arrow table-producer adapters
  examples/gen_fixtures.rs          deterministically generates the test images (make fixtures)
  tests/roundtrip.rs                integration tests (generate↔decode, hostile input)
test/sql/*.test                     haybarn-unittest sqllogictest — authoritative E2E
test/sql/data/                      committed tiny fixture images (qr.png, code128.png)
Makefile                            test / test-unit / test-sql / lint / fmt / fixtures / build / clean
```

Pattern: keep computation in `barcoding.rs` (pure, unit-tested), keep Arrow
marshalling in `arrow_io.rs` + `scalar/*.rs` + `table/*.rs` (thin, harness-tested).

## Library: rxing (decode + encode)

`rxing` 0.9.1 handles both directions. Decode goes through the luma helpers
(`detect_in_luma_with_hints` / `detect_multiple_in_luma_with_hints`) so we
control the grayscale conversion and the dimension guard. Encode is
`MultiFormatWriter::encode` → a `BitMatrix` rendered to a PNG.

rxing's `BarcodeFormat::Display` yields lowercase/spaced names (`"qrcode"`,
`"ean 13"`); we map enum variants to the canonical ZXing names (`QR_CODE`,
`EAN_13`) ourselves in `barcoding::format_name`. `BarcodeFormat::from(&str)` is
permissive on input (accepts `EAN_13` / `ean 13` / `ean13`).

## Sharp edges (learned the hard way)

1. **`haybarn-unittest` skips `require vgi`** — `.test` files use explicit
   `statement ok` + `LOAD vgi;`. Functions live under the `barcode` catalog, so
   each file does `SET search_path = 'barcode.main'`, then `USE memory` before
   `DETACH barcode`.

2. **`image` is pinned to `=0.25.8`** — rxing 0.9.1 pins `image = "=0.25.8"`
   exactly, so we match it so a single `image` resolves (0.25.9 would conflict
   with rxing; 0.25.10 also bumps MSRV past the workspace `rust-version = 1.86`).
   `rxing` is pinned `=0.9.1`. Default `image` features are disabled (png/jpeg/
   gif/bmp/webp only).

3. **Arity overloads + overload scoring (THE big one).** DuckDB scalar functions
   are positional-only, so `generate_qr(text[, size])` and
   `generate_barcode(text, format[, size])` are registered as *separate* arity
   overloads. The SDK's overload resolver scores **const args against the
   *compacted* positional arrays** (DuckDB sends only the const values, in send
   order) *before* `remap_positional` puts them back at their declared indices.
   A concrete const type (e.g. `int64` `size_px` at declared position 2) then
   gets scored at compacted index 1 against the *format* spec and spuriously
   rejected → `No matching overload`. **Fix:** declare the disambiguating const
   args as `arrow_type = "any"` (see `scalar/generate.rs::const_any`). ANY const
   args score neutrally and never reject, so overloads resolve by argument COUNT
   — which is exactly what distinguishes them. The actual value is still
   validated in `on_bind`/`process` via `const_str` / `const_i64`. (Concrete
   `const_arg` only works when there is a single const arg, like the 2-arg
   `generate_barcode`'s `format`, which lands at compacted index 0.)

4. **Table functions take CONSTANTS, not subqueries.** `decode_barcodes(blob)`
   reads its BLOB via `const_bytes(0)`; `SELECT … FROM decode_barcodes((SELECT
   content FROM read_blob(…)))` fails the DuckDB binder ("Table function cannot
   contain subqueries"). The SQL E2E feeds a constant-foldable BLOB instead
   (`generate_qr('…')` / a `'…'::BLOB` literal). Per-row decode of a column is
   the `decode_barcodes` *scalar*… no — for per-row use call the *scalars*
   (`decode_barcode` / `barcode_format`) over the column; the table function is
   for fanning one constant image into its multiple codes.

5. **Untrusted images.** Every decode path runs `guard_dimensions` (≤ 20000 px/
   side, ≤ 100 MP) at the header stage before allocating pixels, and maps any
   rxing/image error to `None` (→ NULL / no rows), never a panic. The
   `bad_blob_beside_good_still_produces_results` tests (Rust + SQL) prove a
   hostile blob next to a valid one still yields results and keeps the worker
   alive.

6. **Fixtures are generated, not hand-authored.** `make fixtures` runs
   `examples/gen_fixtures.rs` (which `#[path]`-includes `barcoding.rs`) to
   (re)produce `test/sql/data/qr.png` and `code128.png` deterministically using
   the worker's own encoder, so the fixtures are exactly what the worker decodes.

## Testing

```sh
cargo test --workspace --all-features    # pure unit + arrow-boundary harness + integration
cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all -- --check
make test-sql                            # builds release, sets VGI_BARCODE_WORKER, haybarn over test/sql/*
make test                                # cargo test + sql
```

CI (`.github/workflows/ci.yml`) runs fmt/clippy/build/test plus a gated
`e2e-sql` job (installs `uv` + `haybarn-unittest`, runs `make test-sql`).

## Function surface

Scalars: `decode_barcode` (VARCHAR), `barcode_format` (VARCHAR), `generate_qr`
(BLOB, 1- and 2-arg), `generate_barcode` (BLOB, 2- and 3-arg). Tables:
`decode_barcodes` (seq/format/text), `barcode_formats` (format).
Garbage/empty/oversized/hostile input → graceful NULL / no rows; an invalid
generate-format name is a clear error. The worker build version is published as
the catalog's `implementation_version` (read it from `vgi_catalogs()`), not as a
scalar function (VGI328).
