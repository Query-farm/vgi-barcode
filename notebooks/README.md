# Barcode label QA

| Notebook | Question | Run |
|---|---|---|
| [01 · Does a changed scan string mean the label failed?](01_label_identity_and_image_size.ipynb) | How can we preserve label identity, distinguish one decoded symbol from every symbol, and compare requested pixel size with actual PNG dimensions? | [Open in Colab](https://colab.research.google.com/github/Query-farm/vgi-barcode/blob/main/notebooks/01_label_identity_and_image_size.ipynb) |

The visible `github-auto://Query-farm/vgi-barcode@v0.4.0` ATTACH downloads the native
release for the current platform. No worker build, account, external image, network
label lookup or scanner is required. The inputs are nine synthetic labels generated
by the real worker. Three of those QR PNGs are then pasted unchanged onto a white
canvas; this makes the single-symbol versus multiple-symbol distinction observable.

In Colab choose **Runtime → Run all**. Setup installs the base shared package and
retrieves its hash-verified companion. Compatible Colab kernel and plotting libraries
remain installed. The `[runner]` extra belongs only to the local publication environment:

```sh
uv sync --locked
uv run --locked vgi-notebooks
uv run --locked vgi-notebooks 01_label_identity_and_image_size.ipynb --parameters '{"SIZE_PX":128}' --output-dir artifacts/size-128
```

To use an interactive local Jupyter environment, select `notebooks/.venv/bin/python`
as the interpreter; the runner creates its own temporary kernel registration. Overrides
require a separate destination and failed executions preserve the saved default.
If an older Colab setup already replaced kernel libraries, save your work, choose
**Runtime → Disconnect and delete runtime**, reconnect and open this current example.

The [shared standard](https://github.com/Query-farm/vgi-example-notebooks/blob/v0.3.1/docs/standard.md),
runner, canonical branding, secret-isolated SQL session and SVG presentation live in
[vgi-example-notebooks](https://github.com/Query-farm/vgi-example-notebooks).
`barcode_example.py` only assembles synthetic images, parses PNG dimensions, checks
results and presents charts/interpretations. Worker calls and identity rules stay in
visible SQL. Keep the committed `charts/` SVG files with the notebook for GitHub
rendering; Colab and Jupyter reruns emit native SVG. The local PNG files under
`artifacts/review/` are optional review exports, not the notebook chart format.

The notebook exercises all six public named SQL surfaces, grouped by `(schema,name)`:
four scalars, `decode_barcodes`, and the table/function access forms of `barcode_formats`.
`coverage.json` records full release and current-source runtime catalogs, successful
execution receipts, visible SQL mappings and narrative review. The actual reader
runtime remains v0.4.0; agreement with current-main signatures does not imply every
future worker release is compatible. Upgrade the pin only after repeating the SQL,
experiment and catalog checks.

The experiment changes only `SIZE_PX`, from 256 to 128. It preserves the nine payloads,
the narrowly defined EAN-13/UPC-A equivalence rule and the three QR payloads expected
from the composite image. Actual symbol dimensions may exceed the requested size.
PNG compression means a smaller width does not guarantee fewer bytes. Detection
sequence is not a promised spatial ordering.

Encoding and decoding use the same rxing implementation. These pristine digital
round trips are not independent certification, a printer/camera test, or evidence
of physical scanning reliability. A NULL scalar decode does not distinguish malformed
bytes from a valid image with no detected code; the example retains a missing-input
flag. It never fetches the authored URL payload. Defaults, the size experiment, a
notebook-only bootstrap and already-imported Colab-compatible libraries are validated
separately; a Colab link is not a claim of manually operating a hosted Colab session.
