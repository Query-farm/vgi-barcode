//! `barcode_version()` — return the worker's version string.

use std::sync::Arc;

use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

pub struct BarcodeVersion;

impl ScalarFunction for BarcodeVersion {
    fn name(&self) -> &str {
        "barcode_version"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Returns the barcode worker version string".into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT barcode.main.barcode_version();".into(),
                description: "Return the running barcode worker's version string.".into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Barcode Worker Version",
                "Return the semantic version string of the running barcode worker binary. Useful \
                 for diagnostics and confirming which build is attached.",
                "Return the barcode worker version string, e.g. `barcode_version()` → '0.1.0'.",
                &[
                    "version",
                    "build version",
                    "barcode_version",
                    "diagnostics",
                    "worker version",
                    "semver",
                ],
                "reference",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let rows = batch.num_rows();
        let out: ArrayRef = Arc::new(StringArray::from(vec![crate::version(); rows]));
        RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}
