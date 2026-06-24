//! `decode_barcodes(blob) -> (seq BIGINT, format VARCHAR, text VARCHAR)` — every
//! barcode detected in one image, one row each.
//!
//! The image BLOB is a bind-time constant (DuckDB table functions take constant
//! arguments, not row columns). An undecodable / hostile blob — or a valid image
//! with no barcodes — yields zero rows, never an error and never a crash.

use std::sync::Arc;

use arrow_array::builder::{Int64Builder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

use crate::barcoding::{self, Decoded};

/// Guaranteed-runnable, catalog-qualified examples (VGI509). Each `sql` is
/// self-contained and re-runnable against an attached `barcode` worker. The
/// image BLOB is constant-folded (a freshly generated QR), since table functions
/// take constant arguments rather than row columns. We omit `expected_result`
/// deliberately — the linter only needs each query to execute cleanly.
const EXECUTABLE_EXAMPLES: &str = r#"[
  {
    "description": "Decode every symbol in a freshly generated QR image.",
    "sql": "SELECT seq, format, text FROM barcode.main.decode_barcodes(barcode.main.generate_qr('hello world')) ORDER BY seq"
  },
  {
    "description": "Decode every symbol in a generated EAN-13 barcode image.",
    "sql": "SELECT format, text FROM barcode.main.decode_barcodes(barcode.main.generate_barcode('5901234123457', 'EAN_13'))"
  }
]"#;

pub struct DecodeBarcodes;

fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("seq", DataType::Int64, false),
        Field::new("format", DataType::Utf8, true),
        Field::new("text", DataType::Utf8, true),
    ]))
}

impl TableFunction for DecodeBarcodes {
    fn name(&self) -> &str {
        "decode_barcodes"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Decode All Barcodes In Image",
            "Decode every barcode and QR code found in a single image BLOB, returning one row per \
             symbol with its zero-based sequence index, detected format, and decoded text. An \
             undecodable or hostile blob, or a valid image with no symbols, yields zero rows. The \
             image is a bind-time constant argument.",
            "Decode **all** barcodes/QR codes in an image BLOB into `(seq, format, text)` rows.",
            "decode all barcodes, multiple barcodes, fan out, every barcode, decode_barcodes, \
             scan image, all symbols, table function",
            "table/decode_barcodes.rs",
        );
        tags.push((
            "vgi.result_columns_md".into(),
            "| column | type | description |\n\
             |---|---|---|\n\
             | `seq` | BIGINT | Zero-based index of the symbol within the image. |\n\
             | `format` | VARCHAR | Detected symbology, e.g. `QR_CODE`, `EAN_13`, `CODE_128`. |\n\
             | `text` | VARCHAR | Decoded payload text of the symbol. |"
                .into(),
        ));
        tags.push(("vgi.executable_examples".into(), EXECUTABLE_EXAMPLES.into()));
        FunctionMetadata {
            description:
                "Decode ALL barcodes/QR codes in an image BLOB into (seq, format, text) rows".into(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::const_arg(
            "blob",
            0,
            "binary",
            "Image bytes (BLOB)",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        // NULL blob → no rows. Undecodable / hostile blob → no rows (decode_all
        // only errors for an undecodable image; we still map that to empty).
        let rows = match params.arguments.const_bytes(0) {
            None => Vec::new(),
            Some(bytes) => barcoding::decode_all(&bytes).unwrap_or_default(),
        };
        Ok(Box::new(DecodeProducer {
            schema: params.output_schema.clone(),
            rows,
            done: false,
        }))
    }
}

struct DecodeProducer {
    schema: SchemaRef,
    rows: Vec<Decoded>,
    done: bool,
}

impl TableProducer for DecodeProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;

        let mut seq = Int64Builder::new();
        let mut format = StringBuilder::new();
        let mut text = StringBuilder::new();
        for (i, d) in self.rows.iter().enumerate() {
            seq.append_value(i as i64);
            format.append_value(&d.format);
            text.append_value(&d.text);
        }
        let cols: Vec<ArrayRef> = vec![
            Arc::new(seq.finish()),
            Arc::new(format.finish()),
            Arc::new(text.finish()),
        ];
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), cols)
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        ))
    }
}
