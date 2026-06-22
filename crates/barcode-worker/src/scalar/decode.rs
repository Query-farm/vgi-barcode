//! `decode_barcode(blob) -> VARCHAR` and `barcode_format(blob) -> VARCHAR`.
//!
//! Both decode the FIRST barcode found in an image BLOB. They differ only in
//! which field they project (the decoded text vs. the format name). An image
//! with no barcode — or an undecodable / hostile blob — yields NULL, never an
//! error and never a crash.

use std::sync::Arc;

use arrow_array::builder::StringBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::blob_bytes;
use crate::barcoding;

/// Which field of the first decoded barcode a given function projects.
#[derive(Clone, Copy)]
enum Field {
    Text,
    Format,
}

pub struct DecodeBarcode {
    field: Field,
    name: &'static str,
    desc: &'static str,
}

impl DecodeBarcode {
    pub fn text() -> Self {
        DecodeBarcode {
            field: Field::Text,
            name: "decode_barcode",
            desc: "Decode the first barcode/QR in an image BLOB to its text (NULL if none)",
        }
    }

    pub fn format() -> Self {
        DecodeBarcode {
            field: Field::Format,
            name: "barcode_format",
            desc:
                "Format name of the first barcode/QR in an image BLOB, e.g. QR_CODE (NULL if none)",
        }
    }
}

impl ScalarFunction for DecodeBarcode {
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: self.desc.into(),
            return_type: Some(DataType::Utf8),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::any_column("blob", 0, "Image bytes (BLOB)")]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let rows = batch.num_rows();
        let mut out = StringBuilder::new();
        for i in 0..rows {
            match blob_bytes(col, i)? {
                // NULL in → NULL out.
                None => out.append_null(),
                Some(bytes) => {
                    // Untrusted bytes: any decode failure → NULL (never error /
                    // panic). A "valid image, no barcode" also → NULL.
                    match barcoding::decode_first(bytes) {
                        Ok(Some(d)) => match self.field {
                            Field::Text => out.append_value(&d.text),
                            Field::Format => out.append_value(&d.format),
                        },
                        Ok(None) | Err(_) => out.append_null(),
                    }
                }
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{bound_type, run_scalar_blob};
    use crate::barcoding;
    use arrow_array::cast::AsArray;
    use arrow_array::Array;
    use vgi::arguments::Arguments;

    fn qr(text: &str) -> Vec<u8> {
        barcoding::generate_qr(text, 220).unwrap()
    }

    #[test]
    fn binds_varchar() {
        assert_eq!(bound_type(&DecodeBarcode::text()), DataType::Utf8);
        assert_eq!(bound_type(&DecodeBarcode::format()), DataType::Utf8);
    }

    #[test]
    fn decode_text_and_format() {
        let png = qr("scan me");
        let text =
            run_scalar_blob(&DecodeBarcode::text(), &[Some(&png)], Arguments::default()).unwrap();
        assert_eq!(text.as_string::<i32>().value(0), "scan me");
        let fmt = run_scalar_blob(
            &DecodeBarcode::format(),
            &[Some(&png)],
            Arguments::default(),
        )
        .unwrap();
        assert_eq!(fmt.as_string::<i32>().value(0), "QR_CODE");
    }

    #[test]
    fn null_in_null_out() {
        let out = run_scalar_blob(&DecodeBarcode::text(), &[None], Arguments::default()).unwrap();
        assert!(out.is_null(0));
    }

    #[test]
    fn garbage_blob_is_null_not_error() {
        let out = run_scalar_blob(
            &DecodeBarcode::text(),
            &[Some(b"not an image")],
            Arguments::default(),
        )
        .unwrap();
        assert!(out.is_null(0), "garbage must yield NULL, not an error");
    }

    #[test]
    fn bad_blob_beside_good_still_produces_results() {
        // A hostile blob next to a valid QR: the good one must still decode and
        // the worker must survive (no panic, no whole-batch failure).
        let png = qr("survivor");
        let out = run_scalar_blob(
            &DecodeBarcode::text(),
            &[Some(b"garbage"), None, Some(&png), Some(b"")],
            Arguments::default(),
        )
        .unwrap();
        let s = out.as_string::<i32>();
        assert!(out.is_null(0));
        assert!(out.is_null(1));
        assert_eq!(s.value(2), "survivor");
        assert!(out.is_null(3));
    }
}
