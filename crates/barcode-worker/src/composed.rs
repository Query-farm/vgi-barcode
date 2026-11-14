//! Composed (multi-object) example queries: each combines two or more of this
//! worker's own functions/tables in one realistic query.
//!
//! Each SQL/description is defined exactly once here, as a `macro_rules!` string
//! literal so it can be spliced with `concat!` into the raw-JSON
//! `vgi.example_queries` / `vgi.executable_examples` tag constants (`concat!`
//! only accepts literals, not `const`s). The module is declared first with
//! `#[macro_use]` in `lib.rs`, so these macros are in scope everywhere. The SQL must stay on one line and
//! contain no `"` or `\`, so it embeds into JSON verbatim. The outputs are
//! pinned by `test/sql/composed.test`; the unit test below keeps that file in
//! sync with these literals.

/// Label QA: encode SKUs as EAN-13, scan each image back, flag mismatches.
/// Driven by `generate_barcode`; also uses `barcode_format` + `decode_barcode`.
macro_rules! label_qa_sql {
    () => {
        "SELECT sku, barcode.main.barcode_format(png) AS format, barcode.main.decode_barcode(png) AS decoded, barcode.main.decode_barcode(png) = sku AS round_trips FROM (SELECT sku, barcode.main.generate_barcode(sku, 'EAN_13') AS png FROM (VALUES ('5901234123457'), ('4006381333931'), ('0036000291452')) AS t(sku)) ORDER BY sku"
    };
}
macro_rules! label_qa_desc {
    () => {
        "Label QA: print EAN-13 barcodes for a list of SKUs, scan each image back, and flag any that do not round-trip (a leading-zero EAN-13 reads back as its 12-digit UPC_A)."
    };
}

/// Relabel: scan an EAN-13 label, mint a QR linking to the product page, and
/// fan the new QR image out. Driven by `decode_barcodes`; also uses
/// `generate_barcode`, `decode_barcode` and `generate_qr`.
macro_rules! relabel_qr_sql {
    () => {
        "SELECT seq, format, text FROM barcode.main.decode_barcodes(barcode.main.generate_qr('https://shop.example/p/' || barcode.main.decode_barcode(barcode.main.generate_barcode('4006381333931', 'EAN_13')))) ORDER BY seq"
    };
}
macro_rules! relabel_qr_desc {
    () => {
        "Relabel a product: read the code off its EAN-13 barcode, encode a product-page URL built from it as a QR code, and decode every symbol in the new QR image to verify it."
    };
}

/// Symbology coverage: count a label set's images per supported format,
/// including formats it never uses. Driven by the `barcode_formats` table; also
/// uses `generate_qr`, `generate_barcode` and `barcode_format`.
macro_rules! format_coverage_sql {
    () => {
        "WITH labels AS (SELECT barcode.main.generate_qr(t.ticket) AS png FROM (VALUES ('ticket-001'), ('ticket-002')) AS t(ticket) UNION ALL SELECT barcode.main.generate_barcode('5901234123457', 'EAN_13') UNION ALL SELECT barcode.main.generate_barcode('PALLET-77', 'CODE_128')) SELECT f.format, count(l.png) AS labels FROM barcode.main.barcode_formats AS f LEFT JOIN labels AS l ON barcode.main.barcode_format(l.png) = f.format GROUP BY f.format ORDER BY labels DESC, f.format"
    };
}
macro_rules! format_coverage_desc {
    () => {
        "Symbology coverage: detect the format of each image in a label set and count labels per supported symbology, listing unused symbologies with 0."
    };
}

/// One `{"description": …, "sql": …}` JSON object literal for a composed
/// example, e.g. `composed_example_json!(label_qa_desc, label_qa_sql)`.
macro_rules! composed_example_json {
    ($desc:ident, $sql:ident) => {
        concat!(
            "{\"description\": \"",
            $desc!(),
            "\", \"sql\": \"",
            $sql!(),
            "\"}"
        )
    };
}

/// Every composed example SQL, for the sync test below.
#[cfg(test)]
const ALL_SQL: [&str; 3] = [label_qa_sql!(), relabel_qr_sql!(), format_coverage_sql!()];

#[cfg(test)]
mod tests {
    use super::ALL_SQL;

    /// The SQL E2E test pins each composed example's rows; it must run the exact
    /// SQL published in the catalog metadata.
    #[test]
    fn composed_sql_is_pinned_by_sql_test() {
        let pinned = include_str!("../../../test/sql/composed.test");
        for sql in ALL_SQL {
            assert!(
                pinned.contains(sql),
                "test/sql/composed.test does not run: {sql}"
            );
            assert!(
                !sql.contains('"') && !sql.contains('\\') && !sql.contains('\n'),
                "composed SQL must embed verbatim into JSON: {sql}"
            );
        }
    }

    /// Each composed example splices into valid, catalog-qualified JSON.
    #[test]
    fn composed_examples_are_valid_json_objects() {
        for json in [
            composed_example_json!(label_qa_desc, label_qa_sql),
            composed_example_json!(relabel_qr_desc, relabel_qr_sql),
            composed_example_json!(format_coverage_desc, format_coverage_sql),
        ] {
            assert!(json.starts_with("{\"description\": \"") && json.ends_with("\"}"));
            assert!(json.contains("barcode.main."));
        }
    }
}
