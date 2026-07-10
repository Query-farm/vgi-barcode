//! `generate_qr(text [, size_px]) -> BLOB` and
//! `generate_barcode(text, format [, size_px]) -> BLOB`.
//!
//! Each function emits a PNG of the encoded symbol. `text` is a VARCHAR column;
//! `format` and `size_px` are bind-time constants. An invalid format name or a
//! payload that cannot be encoded in the requested symbology surfaces a clear
//! DuckDB error (encoding is a user request, not untrusted input). NULL text →
//! NULL output.

use std::sync::Arc;

use arrow_array::builder::BinaryBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::arguments::Arguments;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::text_str;
use crate::barcoding;

fn ve(e: impl std::fmt::Display) -> RpcError {
    RpcError::value_error(e.to_string())
}

/// A bind-time constant argument typed as ANY. We use ANY (rather than a
/// concrete `varchar`/`int64`) for the *disambiguating* const args of our arity
/// overloads so that overload resolution distinguishes the overloads by argument
/// COUNT alone. (The SDK scores concrete const args against the *compacted*
/// positional arrays before remapping them to their declared positions, so a
/// concrete type at a shifted index can spuriously reject the right overload.
/// An ANY const scores neutrally and never rejects; the count check then picks
/// the correct overload. We still validate the actual value at bind/process.)
fn const_any(name: &str, position: i32, doc: &str) -> ArgSpec {
    ArgSpec {
        name: name.to_string(),
        position,
        arrow_type: "any".to_string(),
        doc: doc.to_string(),
        is_const: true,
        is_varargs: false,
        arrow_data_type: None,
        type_bound: None,
        choices: None,
        ge: None,
        le: None,
        gt: None,
        lt: None,
        pattern: None,
        default: None,
    }
}

/// Resolve the optional `size_px` constant at positional index `pos`, defaulting
/// to [`barcoding::DEFAULT_GENERATE_PX`] when absent.
fn size_px(args: &Arguments, pos: usize) -> Result<u32> {
    match args.const_i64(pos) {
        None => Ok(barcoding::DEFAULT_GENERATE_PX),
        Some(n) => barcoding::check_size(n).map_err(ve),
    }
}

// ---------------------------------------------------------------------------
// generate_qr(text) / generate_qr(text, size_px)
// ---------------------------------------------------------------------------

pub struct GenerateQr {
    /// Whether this overload accepts the 2nd positional `size_px` argument.
    with_size: bool,
}

impl GenerateQr {
    pub fn plain() -> Self {
        GenerateQr { with_size: false }
    }
    pub fn with_size() -> Self {
        GenerateQr { with_size: true }
    }
}

impl ScalarFunction for GenerateQr {
    fn name(&self) -> &str {
        "generate_qr"
    }

    fn metadata(&self) -> FunctionMetadata {
        let (description, example) = if self.with_size {
            (
                "Generate a QR-code PNG (BLOB) for the given text at a chosen pixel size",
                FunctionExample {
                    sql: "SELECT barcode.main.generate_qr('https://query.farm', 512);".into(),
                    description: "Encode a URL as a 512x512-pixel QR-code PNG.".into(),
                    expected_output: None,
                },
            )
        } else {
            (
                "Generate a QR-code PNG (BLOB) for the given text at the default size",
                FunctionExample {
                    sql: "SELECT barcode.main.generate_qr('https://query.farm');".into(),
                    description: "Encode a URL as a default-size QR-code PNG.".into(),
                    expected_output: None,
                },
            )
        };
        FunctionMetadata {
            description: description.into(),
            return_type: Some(DataType::Binary),
            examples: vec![example],
            tags: crate::meta::object_tags(
                "Generate QR Code Image",
                "Encode the given text as a QR code and return it as a PNG image BLOB. Optionally \
                 takes a square side length in pixels (default 256). Use for producing scannable \
                 QR codes from URLs, payloads, or arbitrary text in SQL.",
                "Generate a **QR-code PNG** (BLOB) from text, optionally at a chosen pixel size.",
                &[
                    "generate qr",
                    "qr code",
                    "make qr",
                    "encode qr",
                    "qr png",
                    "create barcode image",
                    "generate_qr",
                    "scannable code",
                ],
                "generate",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        let mut specs = vec![ArgSpec::column(
            "text",
            0,
            "varchar",
            // Free-form input: any string encodes. Phrased so it does not read as
            // a closed enumeration of allowed values (VGI317).
            "Arbitrary text to encode into the QR code — for example a website link.",
        )];
        if self.with_size {
            specs.push(const_any(
                "size_px",
                1,
                "Square side length in pixels of the output image (default 256)",
            ));
        }
        specs
    }

    fn on_bind(&self, params: &BindParams) -> Result<BindResponse> {
        // Validate the size constant eagerly when present (fail fast at bind).
        if self.with_size {
            size_px(&params.arguments, 1)?;
        }
        Ok(BindResponse::result(DataType::Binary))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let px = if self.with_size {
            size_px(&params.arguments, 1)?
        } else {
            barcoding::DEFAULT_GENERATE_PX
        };

        let col = batch.column(0);
        let rows = batch.num_rows();
        let mut out = BinaryBuilder::new();
        for i in 0..rows {
            match text_str(col, i)? {
                None => out.append_null(),
                Some(text) => {
                    let png = barcoding::generate_qr(text, px).map_err(ve)?;
                    out.append_value(&png);
                }
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// generate_barcode(text, format) / generate_barcode(text, format, size_px)
// ---------------------------------------------------------------------------

pub struct GenerateBarcode {
    /// Whether this overload accepts the 3rd positional `size_px` argument.
    with_size: bool,
}

impl GenerateBarcode {
    pub fn plain() -> Self {
        GenerateBarcode { with_size: false }
    }
    pub fn with_size() -> Self {
        GenerateBarcode { with_size: true }
    }
}

impl ScalarFunction for GenerateBarcode {
    fn name(&self) -> &str {
        "generate_barcode"
    }

    fn metadata(&self) -> FunctionMetadata {
        let (description, example) = if self.with_size {
            (
                "Generate a barcode PNG (BLOB) for the given text in a named symbology at a chosen \
                 pixel width",
                FunctionExample {
                    sql: "SELECT barcode.main.generate_barcode('5901234123457', 'EAN_13', 600);"
                        .into(),
                    description: "Encode an EAN-13 product code as a 600-pixel-wide barcode PNG."
                        .into(),
                    expected_output: None,
                },
            )
        } else {
            (
                "Generate a barcode PNG (BLOB) for the given text in a named symbology at the \
                 default size",
                FunctionExample {
                    sql: "SELECT barcode.main.generate_barcode('CODE128', 'CODE_128');".into(),
                    description: "Encode text as a default-size Code 128 barcode PNG.".into(),
                    expected_output: None,
                },
            )
        };
        FunctionMetadata {
            description: description.into(),
            return_type: Some(DataType::Binary),
            examples: vec![example],
            tags: crate::meta::object_tags(
                "Generate Barcode Image",
                "Encode the given text in a named barcode symbology (e.g. EAN_13, UPC_A, CODE_128, \
                 CODE_39, ITF, CODABAR, DATA_MATRIX, PDF_417, AZTEC, QR_CODE) and return it as a \
                 PNG image BLOB. Optionally takes an image width in pixels (default 256). Use for \
                 producing scannable product/barcode images in SQL.",
                "Generate a **barcode PNG** (BLOB) from text in a named symbology, optionally at a \
                 chosen pixel width.",
                &[
                    "generate barcode",
                    "make barcode",
                    "encode barcode",
                    "barcode png",
                    "ean",
                    "upc",
                    "code 128",
                    "code 39",
                    "symbology",
                    "generate_barcode",
                    "create barcode image",
                ],
                "generate",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        let mut specs = vec![
            ArgSpec::column(
                "text",
                0,
                "varchar",
                "Payload to encode into the barcode (must be valid for the chosen symbology, \
                 e.g. 13 digits for EAN_13)",
            ),
            const_any(
                "format",
                1,
                "Canonical symbology name to encode in, e.g. QR_CODE, EAN_13, CODE_128",
            ),
        ];
        if self.with_size {
            specs.push(const_any(
                "size_px",
                2,
                "Width in pixels of the output image (default 256)",
            ));
        }
        specs
    }

    fn on_bind(&self, params: &BindParams) -> Result<BindResponse> {
        // Validate the format (and size, if present) eagerly at bind.
        let fmt = params
            .arguments
            .const_str(1)
            .ok_or_else(|| ve("generate_barcode: a format string is required"))?;
        barcoding::parse_format(&fmt).map_err(ve)?;
        if self.with_size {
            size_px(&params.arguments, 2)?;
        }
        Ok(BindResponse::result(DataType::Binary))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let fmt_str = params
            .arguments
            .const_str(1)
            .ok_or_else(|| ve("generate_barcode: a format string is required"))?;
        let format = barcoding::parse_format(&fmt_str).map_err(ve)?;
        let px = if self.with_size {
            size_px(&params.arguments, 2)?
        } else {
            barcoding::DEFAULT_GENERATE_PX
        };

        let col = batch.column(0);
        let rows = batch.num_rows();
        let mut out = BinaryBuilder::new();
        for i in 0..rows {
            match text_str(col, i)? {
                None => out.append_null(),
                Some(text) => {
                    let png = barcoding::generate_barcode(text, format, px).map_err(ve)?;
                    out.append_value(&png);
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
    use crate::arrow_io::test_support::{bound_type, run_scalar_text};
    use crate::barcoding;
    use arrow_array::cast::AsArray;
    use arrow_array::{Array, ArrayRef, Int64Array, StringArray};

    /// Positional const args `(text, format[, size])` as DuckDB would hand them.
    fn args(text: &str, format: Option<&str>, size: Option<i64>) -> Arguments {
        let mut cols: Vec<ArrayRef> = vec![Arc::new(StringArray::from(vec![Some(text)]))];
        if let Some(f) = format {
            cols.push(Arc::new(StringArray::from(vec![Some(f)])));
        }
        if let Some(s) = size {
            cols.push(Arc::new(Int64Array::from(vec![Some(s)])));
        }
        let bytes = Arguments::serialize_positional(&cols).unwrap();
        Arguments::parse(&bytes).unwrap()
    }

    #[test]
    fn qr_binds_binary_and_roundtrips() {
        assert_eq!(bound_type(&GenerateQr::plain()), DataType::Binary);
        let out =
            run_scalar_text(&GenerateQr::plain(), &[Some("hi")], Arguments::default()).unwrap();
        assert_eq!(out.data_type(), &DataType::Binary);
        let png = out.as_binary::<i32>().value(0);
        let d = barcoding::decode_first(png).unwrap().unwrap();
        assert_eq!((d.format.as_str(), d.text.as_str()), ("QR_CODE", "hi"));
    }

    #[test]
    fn qr_with_size_roundtrips() {
        let a = args("sized", None, Some(300));
        let out = run_scalar_text(&GenerateQr::with_size(), &[Some("sized")], a).unwrap();
        let png = out.as_binary::<i32>().value(0);
        assert_eq!(barcoding::decode_first(png).unwrap().unwrap().text, "sized");
    }

    #[test]
    fn qr_null_in_null_out() {
        let out = run_scalar_text(&GenerateQr::plain(), &[None], Arguments::default()).unwrap();
        assert!(out.is_null(0));
    }

    #[test]
    fn qr_bad_size_errors_at_bind() {
        let bind = BindParams {
            arguments: args("x", None, Some(0)),
            ..Default::default()
        };
        assert!(GenerateQr::with_size().on_bind(&bind).is_err());
    }

    #[test]
    fn barcode_roundtrips_ean13() {
        let a = args("5901234123457", Some("EAN_13"), None);
        let out = run_scalar_text(&GenerateBarcode::plain(), &[Some("5901234123457")], a).unwrap();
        let png = out.as_binary::<i32>().value(0);
        let d = barcoding::decode_first(png).unwrap().unwrap();
        assert_eq!(d.format, "EAN_13");
        assert_eq!(d.text, "5901234123457");
    }

    #[test]
    fn barcode_bad_format_errors_at_bind() {
        let bind = BindParams {
            arguments: args("x", Some("NOPE"), None),
            ..Default::default()
        };
        assert!(GenerateBarcode::plain().on_bind(&bind).is_err());
    }

    #[test]
    fn barcode_null_in_null_out() {
        let a = args("x", Some("CODE_128"), None);
        let out = run_scalar_text(&GenerateBarcode::plain(), &[None], a).unwrap();
        assert!(out.is_null(0));
    }
}
