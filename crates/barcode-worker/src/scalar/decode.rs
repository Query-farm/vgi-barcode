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
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
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
    example_sql: &'static str,
    example_desc: &'static str,
    title: &'static str,
    keywords: &'static str,
    description_llm: &'static str,
    description_md: &'static str,
}

impl DecodeBarcode {
    pub fn text() -> Self {
        DecodeBarcode {
            field: Field::Text,
            name: "decode_barcode",
            desc: "Decode the first barcode/QR in an image BLOB to its text (NULL if none)",
            example_sql:
                "SELECT barcode.main.decode_barcode(barcode.main.generate_qr('hello world'));",
            example_desc:
                "Decode the text of the first barcode/QR found in an image BLOB (here a freshly \
                 generated QR).",
            title: "Decode Barcode Text",
            keywords:
                "decode, read barcode, scan barcode, qr decode, barcode text, decode_barcode, \
                 read qr, extract payload",
            description_llm:
                "Decode the first barcode or QR code found in an image BLOB (PNG/JPEG/GIF/BMP/WebP) \
                 and return its decoded text payload. Returns NULL when the image contains no \
                 symbol or cannot be decoded; never errors on hostile input.",
            description_md:
                "Decode the first barcode/QR in an image BLOB to its **text**. Returns NULL when \
                 no symbol is found.",
        }
    }

    pub fn format() -> Self {
        DecodeBarcode {
            field: Field::Format,
            name: "barcode_format",
            desc:
                "Format name of the first barcode/QR in an image BLOB, e.g. QR_CODE (NULL if none)",
            example_sql:
                "SELECT barcode.main.barcode_format(barcode.main.generate_qr('hello world'));",
            example_desc:
                "Identify the symbology of the first barcode/QR in an image BLOB (e.g. 'QR_CODE').",
            title: "Identify Barcode Format",
            keywords: "barcode format, symbology, detect format, qr or barcode, barcode_format, \
                 format name, identify barcode, ean upc code128",
            description_llm:
                "Identify the symbology (format) of the first barcode or QR code found in an image \
                 BLOB and return its canonical ZXing name, e.g. QR_CODE, EAN_13, CODE_128. Returns \
                 NULL when the image contains no symbol or cannot be decoded.",
            description_md:
                "Return the **format name** of the first barcode/QR in an image BLOB, e.g. \
                 `QR_CODE`. Returns NULL when no symbol is found.",
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
            examples: vec![FunctionExample {
                sql: self.example_sql.into(),
                description: self.example_desc.into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                self.title,
                self.description_llm,
                self.description_md,
                self.keywords,
                "scalar/decode.rs",
            ),
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
