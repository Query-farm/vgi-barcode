# Barcode notebooks

[01 · Does a changed scan string mean the label failed?](01_label_identity_and_image_size.ipynb) generates nine synthetic labels with the published worker, distinguishes exact matches from one explicit EAN-13/UPC-A equivalence, measures PNG dimensions/bytes and preserves malformed versus missing-input diagnostics. The experiment changes requested size 256 → 128 pixels.

## Run

Requirements: [uv](https://docs.astral.sh/uv/) and internet access for the locked notebook environment and native worker release. Keep this directory together; a worker checkout, Rust toolchain and local worker build are not required.

```sh
cd notebooks
uv sync --locked
uv run --locked python execute.py
```

Haybarn 2.0.0rc11 and notebook dependencies are locked in `uv.lock`. The visible literal `github-auto://Query-farm/vgi-barcode@v0.4.0` downloads the matching native worker and verifies its checksum. To upgrade, change the release pin and re-execute default and experiment.

For an optional editor kernel:

```sh
uv run --locked python -m ipykernel install --user --name vgi-barcode-notebooks --display-name "Python (vgi-barcode notebooks)"
```

Use this directory as working directory, then Restart Kernel → Run All. The headless runner creates a temporary kernel registration and only replaces a notebook after successful execution. Cleanup explicitly detaches the worker before closing the engine connection.

## Separate experiment

```sh
uv run --locked python execute.py --parameters '{"SIZE_PX": 128}' --output-dir artifacts/size-128
```

Overrides require separate output. The experiment includes relative branding, documentation, runner and environment companions. Restore 256 for defaults. Logs, extracted charts, experiments and virtual environments are ignored.

## Interpretation

All inputs are explicitly synthetic test labels. The EAN values are test examples, not claims about product ownership. No URLs are fetched. Input identity remains textual; only a detected UPC-A result from a 13-digit leading-zero EAN input can receive exactly one prepended zero for the documented equivalence check. Other leading zeros remain meaningful.

Both encoding and decoding use the worker's rxing implementation. Digital round trips and PNG byte sizes do not measure print quality, scanner performance, error correction under damage, capacity or physical dimensions. The worker may enforce a minimum image size; actual dimensions are read from generated PNG headers. NULL decoding does not distinguish malformed image bytes from an otherwise valid image with no detected code; the notebook preserves the missing-input flag separately.
