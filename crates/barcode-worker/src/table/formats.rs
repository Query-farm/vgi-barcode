//! `barcode_formats() -> (format VARCHAR)` — the list of supported barcode
//! format strings, for discovery.

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::builder::StringBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

use crate::barcoding;

pub struct BarcodeFormats;

/// The single-column `(format VARCHAR)` schema produced by `barcode_formats`.
/// Exposed so `main.rs` can also surface it as a zero-argument catalog *table*
/// (VGI311) — `SELECT * FROM barcode.main.barcode_formats` — backed by this same
/// table function.
/// Per-column comment carried in the Arrow field metadata (DuckDB surfaces it as
/// the column COMMENT, which the metadata linter reads).
const FORMAT_COLUMN_COMMENT: &str =
    "A supported barcode/QR symbology name in canonical ZXing form, e.g. QR_CODE, EAN_13, \
     CODE_128 — valid as the `format` argument to generate_barcode.";

/// Number of rows `barcode_formats` produces — the count of supported
/// symbologies. Used as the catalog table's fixed cardinality estimate.
pub fn supported_format_count() -> usize {
    barcoding::supported_formats().len()
}

pub fn output_schema() -> SchemaRef {
    let comment = HashMap::from([("comment".to_string(), FORMAT_COLUMN_COMMENT.to_string())]);
    Arc::new(Schema::new(vec![Field::new(
        "format",
        DataType::Utf8,
        false,
    )
    .with_metadata(comment)]))
}

impl TableFunction for BarcodeFormats {
    fn name(&self) -> &str {
        "barcode_formats"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Supported Barcode Formats",
            "List every barcode/QR symbology name the worker can generate or decode, one row per \
             format. Use it to discover which format strings are valid inputs to generate_barcode \
             and which symbologies decode_barcode/decode_barcodes can recognize.",
            "List the supported barcode/QR **format names**, one per row. Column: `format`.",
            &[
                "supported formats",
                "list formats",
                "barcode formats",
                "symbologies",
                "available formats",
                "barcode_formats",
                "discovery",
                "which barcodes",
            ],
            "reference",
        );
        tags.push((
            "vgi.result_columns_md".into(),
            "| column | type | description |\n\
             |---|---|---|\n\
             | `format` | VARCHAR | A supported barcode/QR symbology name, e.g. `QR_CODE`, \
             `EAN_13`, `CODE_128`, `DATA_MATRIX`, `PDF_417`, `AZTEC`. |"
                .into(),
        ));
        FunctionMetadata {
            description: "List the supported barcode/QR format names".into(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        Ok(Box::new(FormatsProducer {
            schema: params.output_schema.clone(),
            done: false,
        }))
    }
}

struct FormatsProducer {
    schema: SchemaRef,
    done: bool,
}

impl TableProducer for FormatsProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        let mut b = StringBuilder::new();
        for f in barcoding::supported_formats() {
            b.append_value(f);
        }
        let col: ArrayRef = Arc::new(b.finish());
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), vec![col])
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        ))
    }
}
