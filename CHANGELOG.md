# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/).

## 0.2.0

Consolidated release summarizing all changes since `0.1.2` across both `yolowpdf` (PDF core) and `yolowpdf-ocr` (pure-Rust OCR + open-source `YolowOCR-Suite v2` models).

### PDF Core (`yolowpdf`)
- **2-D Math & Matrix Reconstruction:** Native 2-D formula reconstruction (fractions, roots, sub/superscripts, big-operator limits, overlapping scripts) from glyph geometry with a bucketed spatial index; 2-D matrix/cases/binomial reconstruction (`pmatrix`, `bmatrix`, `vmatrix`, `Bmatrix`, `cases`, `\binom`); Ghostscript/dvips 1×1 `ImageMask` rule recovery for fraction and radical bars; 2-state Viterbi HMM text/math segmentation protecting inline units and prose.
- **Layout & Tables:** Recursive X-Y cut column detection with gutter checks for mixed 2/3-column pages, margin line numbers, and small-type multi-column text; faster lattice grid construction; full-page background rectangles are no longer treated as table frames.
- **Fonts & Encoding:** CTAN `mathabx` (`TeX-matha/b/x`) T1/Cork decoding, `CMBR*` non-bold handling, CMMI `0x0F`/`0xB2` and Greek `U+1D6B9`/`U+1D6F3` fixes; OML fix-up scoped strictly to 8-bit simple fonts.
- **Parsing Speed & Robustness:** Flat direct-indexed object `SlotTable`, zero-copy lexer and filter paths, reused inflate state, unified prefix-decoded Type1 `Length1` inflate path, 256-slot simple font lookup table, loop-invariant hoisting in text placement; long numeric tokens are no longer truncated and extreme glyph coordinates no longer overflow.

### OCR Engine & Open-Source v2 Models (`yolowpdf-ocr`)
- **Open-Source `YolowOCR-Suite v2` (BF16, MIT):** Published five from-scratch BF16 safetensors models on Hugging Face ([`YOLOW92/yolowocr-v2`](https://huggingface.co/YOLOW92/yolowocr-v2)): `det_v2` (5.05 MB), `rec_v2_multi` (10.74 MB, 18,710 vocab), `rec_v2_latin` (6.24 MB, 254 vocab), `layout_s_v2` (26.81 MB, 640×640 3L), and `formula_v2_rect_mtp` (66.99 MB, 224×672 rect + MTP-4) — **109.59 MB** full multilingual suite (**8.14× smaller** than the 892 MB upstream FP32 reference stack). Pinned revision, file sizes, and SHA-256 digests are registered in `yolowpdf_ocr::models::{V2_DET, V2_REC_MULTI, V2_REC_LATIN, V2_LAYOUT, V2_FORMULA}` with full backward compatibility for upstream `PP-OCRv6` / `PP-DocLayoutV3` / `PP-FormulaNet_plus-L`.
- **Rectangular Formula Encoder & Lossless MTP-4 Speculative Decoding:** Generalized `formula_enc` to rectangular vision inputs (e.g. `224×672`, decomposed `(14, 42)` relative attention, configurable `post_conv2_stride`), added `preprocess_to`, and implemented lossless batched Multi-Token Prediction (`MtpHead`) self-speculative greedy decoding in `formula::Decoder` (bit-for-bit identical to greedy decoding, ~3.1–3.5 ms/token on CPU).
- **Layout Threshold & Formula Box Deduplication:** `Layout::load` now honors `draw_threshold` from `inference.yml` (default `0.5`); `Math::formula_boxes` applies class-level IoU NMS deduplication while preserving reading order.

### Measured (`2026-11-26`, `YOLOWPDF_FAST_MATH=1` scoring)
- **Born-digital PDFs (`olmocr-all`, 1,269 pages, 7,295 tests, 16 workers):** `1269/1269` in **0.562 s** (**2,256.3 pages/s**, p50 3.4 ms, p95 10.1 ms, peak 37.8 MB); mean score **0.5915 → 0.6950** (`no-base` **0.5100 → 0.6341**, passed **3,484 → 5,170**; `arxiv_math` 0.0881 → 0.6505, `multi_column` 0.6821 → 0.7161, `long_tiny_text` 0.4525 → 0.4796, `table_tests` 0.5930, `headers_footers` 0.7316, `baseline` 0.9992).
- **Hard PDFs (`pdf.js`, 1,089 files, 16 workers, median of 3):** `830/1089` in **0.737 s** (1,125.8 files/s, p50 0.2 ms, peak 102.3 MB, 0 panics).
- **Scanned & Math OCR (`olmocr-scans`, 134 pages, 1,118 tests, `YolowOCR-Suite v2` BF16):**
  - `yolowpdf-ocr-latin-gpu` (text only): `134/134` in **37.562 s** (**3.567 p/s**, peak 628.2 MB, score 0.3891 / no-base 0.0837).
  - `yolowpdf-ocr-gpu` (text only): `134/134` in **46.548 s** (**2.879 p/s**, peak 665.2 MB, score 0.3897 / no-base 0.0846).
  - `yolowpdf-ocr` (CPU text only): `134/134` in **106.580 s** (**1.257 p/s**, peak 592.7 MB, score 0.3897 / no-base 0.0846).
  - `yolowpdf-ocr-math-gpu` (layout + formula): `134/134` in **367.664 s** (**0.364 p/s**, **2.40× faster** than v1 882.4 s; peak **938.5 MB**, **58% less** than v1 2,225 MB; score **0.4799** / no-base **0.2199**).
  - `yolowpdf-ocr-math` (CPU layout + formula): `134/134` in **388.367 s** (**0.345 p/s**, peak 831.2 MB, score **0.4799** / no-base **0.2199**).

## 0.1.2

- **Base64 Data URLs (`yolow64` 0.3):** `yolowpdf` now depends on [`yolow64`](https://crates.io/crates/yolow64) `0.3` and exposes `PdfImage::mime_type()` and `PdfImage::data_url()` to encode exported PDF images directly as `data:<mime>;base64,...` URIs for multimodal and LLM pipelines.
- **Documentation & Visualizations:** Redesigned `README.md` and `docs/README.zh-CN.md` with SVG benchmark and architecture diagrams (`docs/figures/`) generated by `benchmarks/figures/generate.py`.

## 0.1.1

PDF decoding now enforces its resource budget before allocation.

- **Predictors:** PNG/TIFF predictor row buffers are checked against the decode limit before `prev`/`cur` are allocated; `Columns`, `Colors` and `BitsPerComponent` arithmetic is checked, and the TIFF 1/2/4-bit component table is included in the budget.
- **Filters:** ASCII85, ASCIIHex, RunLength and Crypt verify the output limit before reserving, extending or resizing buffers, and return `TooLarge` instead of truncating.
- **Compatibility:** valid PDFs, including PNG filters 0–4 and TIFF 1/2/4/8/16-bit samples, decode to the same bytes. Corrupt inputs that previously parsed by accident can now return a format or size error.
