"""Presentation and synthetic image assembly for the barcode narrative.

Worker calls and the QA/equivalence rules stay in visible notebook SQL.
"""

import hashlib
from io import BytesIO

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from PIL import Image
from IPython.display import display
from matplotlib.patches import Patch
from vgi_notebooks import finish, prose

COLORS = {"EAN_13": "#247f9e", "CODE_128": "#745b9e", "QR_CODE": "#b88a29"}


def validate_size(size):
    if type(size) is not int or not 64 <= size <= 512:
        raise ValueError("SIZE_PX must be an integer from 64 to 512 pixels.")


def qa_summary(results, outcomes, coverage, size):
    display(results, outcomes, coverage)
    assert results.label_id.is_unique and (results.png_bytes > 0).all()
    exact_rows = results[results.outcome == "Exact match"]
    assert (exact_rows.payload == exact_rows.scanned_text).all()
    assert set(COLORS) <= set(coverage.format)
    assert int(coverage.tested_labels.sum()) == len(results)
    counts = results.outcome.value_counts()
    exact, equivalent, missing, mismatch = [
        int(counts.get(k, 0))
        for k in ["Exact match", "EAN/UPC equivalent", "No decode", "Mismatch"]
    ]
    payloads = list(
        results.sort_values("label_id")[
            ["label_id", "requested_format", "payload"]
        ].itertuples(index=False, name=None)
    )
    fingerprint = hashlib.sha256(repr(payloads).encode()).hexdigest()
    tested = int((coverage.tested_labels > 0).sum())
    prose(
        f"At **{size} requested pixels**, the {len(results)} synthetic labels yield **{exact} exact matches**, "
        f"**{equivalent} EAN/UPC equivalents**, **{missing} missing decodes** and **{mismatch} mismatches**. "
        f"A strict text-and-format gate would reject {len(results) - exact}; the explicit equivalence rule accepts "
        f"{equivalent} of those. Our inputs exercise **{tested} of {len(coverage)} advertised formats**; zeros mean "
        "untested formats, not unsupported formats or failed decodes. "
        f"**Payload fingerprint:** `{fingerprint}`. Image size is excluded so the experiment can verify unchanged inputs."
    )
    return fingerprint


def headers_and_preview(image_rows, results, con, size):
    images = {
        row.label_id: Image.open(BytesIO(bytes(row.png))).copy()
        for row in image_rows.itertuples()
    }
    dimensions = pd.DataFrame(
        [
            {"label_id": name, "width_px": img.width, "height_px": img.height}
            for name, img in images.items()
        ]
    )
    con.register("png_header_frame", dimensions)
    con.execute("CREATE TABLE png_dimensions AS SELECT * FROM png_header_frame")
    con.unregister("png_header_frame")
    chosen = ["ean-upc-equivalent", "code-leading-zero", "qr-unicode"]
    fig, axes = plt.subplots(1, 3, figsize=(12, 5.4))
    for ax, name in zip(axes, chosen):
        row = results.set_index("label_id").loc[name]
        img = images[name]
        ax.imshow(img, cmap="gray", vmin=0, vmax=255, interpolation="nearest")
        ax.grid(False)
        ax.axis("off")
        ax.set_title(f"{row.requested_format} → {row.scanned_format}", fontsize=11)
        ax.text(
            0.5,
            -0.10,
            f"Input: {row.payload}\nScan: {row.scanned_text}\n{img.width} × {img.height} px · {row.outcome}",
            ha="center",
            va="top",
            transform=ax.transAxes,
            fontsize=9,
        )
    finish(
        fig,
        "Preserve identity before normalizing a scan",
        "Leading zeros and Unicode belong to the authored payloads.",
        f"Source: synthetic labels · native vgi-barcode v0.4.0 · {size} px requested · screen previews are not physical size",
        name="01-representations",
    )
    return images


def storage_summary(measurements, size):
    display(measurements)
    assert (
        measurements.label_id.is_unique
        and (measurements.width_px > 0).all()
        and (measurements.height_px > 0).all()
    )
    fig, ax = plt.subplots(figsize=(11.5, 6.5))
    y = np.arange(len(measurements))
    ax.barh(
        y,
        measurements.png_bytes,
        color=[COLORS[f] for f in measurements.requested_format],
    )
    ax.set_yticks(y, measurements.label_id)
    ax.invert_yaxis()
    for i, row in measurements.iterrows():
        ax.text(
            row.png_bytes + measurements.png_bytes.max() * 0.015,
            i,
            f"{int(row.png_bytes):,} B · {int(row.width_px)}×{int(row.height_px)} px",
            va="center",
            fontsize=9,
        )
    ax.set(
        xlabel="PNG file size (bytes)", xlim=(0, measurements.png_bytes.max() * 1.55)
    )
    ax.grid(axis="x")
    ax.legend(
        handles=[Patch(color=color, label=fmt) for fmt, color in COLORS.items()],
        loc="lower right",
        fontsize=9,
    )
    finish(
        fig,
        f"Actual image sizes for a {size}-pixel request",
        "Compression and symbol geometry both influence the file size.",
        "Source: synthetic payloads · worker-generated PNG bytes and independently parsed PNG dimensions · vgi-barcode v0.4.0",
        name="02-storage",
    )
    prose(
        f"The generated PNGs occupy **{int(measurements.png_bytes.sum()):,} bytes** in total. "
        f"Actual widths range from **{int(measurements.width_px.min())} to {int(measurements.width_px.max())} pixels**. "
        "These measurements describe this payload set and encoder version, not a universal compression rule."
    )


def make_sheet(images):
    """Place three already-generated QR PNGs on a white canvas without resampling."""
    chosen = ["qr-parcel", "qr-url", "qr-unicode"]
    tiles = [images[name].convert("RGB") for name in chosen]
    gap = 48
    sheet = Image.new(
        "RGB",
        (sum(x.width for x in tiles) + gap * 4, max(x.height for x in tiles) + gap * 2),
        "white",
    )
    x = gap
    for tile in tiles:
        sheet.paste(tile, (x, gap))
        x += tile.width + gap
    output = BytesIO()
    sheet.save(output, format="PNG")
    return sheet, output.getvalue().hex()


def sheet_summary(sheet, decoded_sheet, first_symbol, expected, size):
    display(decoded_sheet, first_symbol)
    expected_text = set(expected.payload)
    observed_text = set(decoded_sheet.text)
    assert expected_text == observed_text and len(decoded_sheet) == len(expected)
    assert set(decoded_sheet.format) == {"QR_CODE"}
    assert first_symbol.scanned_text.iloc[0] in observed_text
    fig, ax = plt.subplots(figsize=(12, 5.3))
    ax.imshow(sheet, interpolation="nearest")
    ax.axis("off")
    finish(
        fig,
        "One image contains three independent labels",
        f"{sheet.width} × {sheet.height} px canvas · 48 px white spacing · tiles pasted without resampling",
        f"Source: three synthetic QR payloads, encoded once by vgi-barcode v0.4.0 at {size} requested pixels; Pillow only assembles the canvas",
        name="03-multiple-symbols",
    )
    prose(
        f"The table decoder returned **{len(decoded_sheet)} symbols**, covering all **{len(expected_text)} expected payloads**. "
        f"The scalar returned only **`{first_symbol.scanned_text.iloc[0]}`**. A successful single decode therefore "
        "does not establish that an image contains only one label. `seq` is the worker's observed detection order; "
        "compare the payload set, not a promised left-to-right order. This clean image does not test occlusion or camera conditions."
    )


def controls_summary(controls):
    display(controls)
    assert (
        controls.loc[controls.input_case == "good QR control", "scanned_text"].iloc[0]
        == "synthetic-control"
    )
    assert (
        controls.loc[controls.input_case != "good QR control", "scanned_text"]
        .isna()
        .all()
    )
    prose(
        "Missing input and malformed bytes both yield no decode, while the good control remains readable. "
        "Keep `missing_input` when diagnosing failures; NULL is not an empty string or proof that a valid image contains no barcode."
    )
