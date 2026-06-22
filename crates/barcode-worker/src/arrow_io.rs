//! Small Arrow helpers shared across the scalar functions: reading BLOB/VARCHAR
//! input cells. Output building (VARCHAR / BLOB) is simple enough to live in the
//! scalar modules; the in-process test harness below drives a `ScalarFunction`
//! end-to-end without the RPC/IPC plumbing.

use arrow_array::cast::AsArray;
use arrow_array::{Array, ArrayRef};
use arrow_schema::DataType;
use vgi_rpc::{Result, RpcError};

/// Borrow the raw bytes of a BLOB/VARCHAR cell at `row`, or `None` if the cell is
/// null. Errors if the column isn't a binary/utf8 type (i.e. not a BLOB input).
pub fn blob_bytes(col: &ArrayRef, row: usize) -> Result<Option<&[u8]>> {
    if col.is_null(row) {
        return Ok(None);
    }
    Ok(Some(match col.data_type() {
        DataType::Binary => col.as_binary::<i32>().value(row),
        DataType::LargeBinary => col.as_binary::<i64>().value(row),
        DataType::Utf8 => col.as_string::<i32>().value(row).as_bytes(),
        DataType::LargeUtf8 => col.as_string::<i64>().value(row).as_bytes(),
        other => {
            return Err(RpcError::value_error(format!(
                "expected a BLOB (binary) argument, got {other:?}"
            )))
        }
    }))
}

/// Borrow the UTF-8 text of a VARCHAR cell at `row`, or `None` if null. Errors if
/// the column isn't a string type.
pub fn text_str(col: &ArrayRef, row: usize) -> Result<Option<&str>> {
    if col.is_null(row) {
        return Ok(None);
    }
    Ok(Some(match col.data_type() {
        DataType::Utf8 => col.as_string::<i32>().value(row),
        DataType::LargeUtf8 => col.as_string::<i64>().value(row),
        other => {
            return Err(RpcError::value_error(format!(
                "expected a VARCHAR (string) argument, got {other:?}"
            )))
        }
    }))
}

/// Test-only helpers shared by the scalar Arrow-boundary unit tests. These let a
/// `#[cfg(test)]` block drive a `ScalarFunction` end to end in-process (build a
/// one-column input `RecordBatch`, run `on_bind` + `process`, inspect the result)
/// without the RPC/IPC plumbing.
#[cfg(test)]
pub mod test_support {
    use std::sync::Arc;

    use arrow_array::builder::{BinaryBuilder, StringBuilder};
    use arrow_array::{ArrayRef, RecordBatch};
    use arrow_schema::{Field, Schema, SchemaRef};
    use vgi::arguments::Arguments;
    use vgi::{BindParams, ProcessParams, ScalarFunction};
    use vgi_rpc::Result;

    /// A single-column `Binary` (BLOB) input batch. `None` entries become NULLs.
    pub fn blob_batch(rows: &[Option<&[u8]>]) -> RecordBatch {
        let mut b = BinaryBuilder::new();
        for r in rows {
            match r {
                Some(bytes) => b.append_value(bytes),
                None => b.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(b.finish());
        let schema = Arc::new(Schema::new(vec![Field::new(
            "blob",
            arr.data_type().clone(),
            true,
        )]));
        RecordBatch::try_new(schema, vec![arr]).unwrap()
    }

    /// A single-column `Utf8` (VARCHAR) input batch. `None` entries become NULLs.
    pub fn text_batch(rows: &[Option<&str>]) -> RecordBatch {
        let mut b = StringBuilder::new();
        for r in rows {
            match r {
                Some(s) => b.append_value(s),
                None => b.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(b.finish());
        let schema = Arc::new(Schema::new(vec![Field::new(
            "text",
            arr.data_type().clone(),
            true,
        )]));
        RecordBatch::try_new(schema, vec![arr]).unwrap()
    }

    /// Build a `ProcessParams` carrying the given output schema and arguments.
    pub fn process_params(output_schema: SchemaRef, arguments: Arguments) -> ProcessParams {
        ProcessParams {
            output_schema,
            input_schema: None,
            execution_id: Vec::new(),
            init_opaque_data: Vec::new(),
            arguments,
            settings: Default::default(),
            secrets: Default::default(),
            auth_principal: None,
            projection_ids: None,
            pushdown_filters: None,
            join_keys: Vec::new(),
            storage: None,
            order_by_column: None,
            order_by_direction: None,
            order_by_null_order: None,
            order_by_limit: None,
            tablesample_percentage: None,
            tablesample_seed: None,
            attach_opaque_data: None,
            at_unit: None,
            at_value: None,
        }
    }

    /// Run a scalar function over a prebuilt input batch: call `on_bind` to
    /// obtain the declared output schema, then `process`, returning the single
    /// result column. The `arguments` apply to both bind and process.
    pub fn run_scalar_on<F: ScalarFunction>(
        f: &F,
        batch: RecordBatch,
        arguments: Arguments,
    ) -> Result<ArrayRef> {
        let bind = BindParams {
            input_schema: Some(batch.schema()),
            arguments: arguments.clone(),
            ..Default::default()
        };
        let bound = f.on_bind(&bind)?;
        let params = process_params(bound.output_schema.clone(), arguments);
        let out = f.process(&params, &batch)?;
        Ok(out.column(0).clone())
    }

    /// Run a scalar over a single-column BLOB input batch.
    pub fn run_scalar_blob<F: ScalarFunction>(
        f: &F,
        rows: &[Option<&[u8]>],
        arguments: Arguments,
    ) -> Result<ArrayRef> {
        run_scalar_on(f, blob_batch(rows), arguments)
    }

    /// Run a scalar over a single-column VARCHAR input batch.
    pub fn run_scalar_text<F: ScalarFunction>(
        f: &F,
        rows: &[Option<&str>],
        arguments: Arguments,
    ) -> Result<ArrayRef> {
        run_scalar_on(f, text_batch(rows), arguments)
    }

    /// The declared output `DataType` from `on_bind` for a scalar with no
    /// bind-time argument requirements.
    pub fn bound_type<F: ScalarFunction>(f: &F) -> arrow_schema::DataType {
        let bind = BindParams::default();
        let bound = f.on_bind(&bind).unwrap();
        bound.output_schema.field(0).data_type().clone()
    }
}
