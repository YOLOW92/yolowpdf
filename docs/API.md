# API reference

The generated Rust documentation (`cargo doc --no-deps --open`) is authoritative.
This page lists the main entry points.

## PDF

```rust
pub fn extract(bytes: &[u8]) -> Result<PdfDocument, Error>;
pub fn extract_with(bytes: &[u8], opts: &Options) -> Result<PdfDocument, Error>;
```

`Options`:

| Field | Meaning |
|---|---|
| `timeout` | hard per-file time limit; the adaptive limit can be smaller |
| `adaptive_timeout` | learn the byte-to-time relation per thread (default on) |
| `max_input_bytes` | input size limit (default 256 MiB) |
| `max_pages` | page limit (default 5,000) |
| `max_stream_bytes` | decompressed stream limit (default 64 MiB) |
| `page_limit` | process only the first N pages |
| `images` | list/export/embed images |
| `ocr` | `Arc<dyn PageOcr>`; default none |
| `force_ocr` | OCR every page with images, not only detected scans |

`PdfDocument` has `markdown`, `pages`, `metadata`, `scanned`, `inferred_title`,
`warnings`, `images`, and `ocr`. Each `PdfPage` has its Markdown, character count,
image references, scan flag, quality report and OCR flag. Each `PdfImage` provides
`file_name()`, `mime_type()`, and (when `ImageOptions::export` is enabled) `data_url()`
powered by [`yolow64`](https://crates.io/crates/yolow64).

`Error` is `Malformed`, `Encrypted`, `TooLarge` or `Timeout`. A parser panic is
caught inside `extract_with` and returned as `Malformed("parser panicked")`.

## OCR

`PageOcr` is the interface between the two crates:

```rust
pub trait PageOcr: Send + Sync + Debug {
    fn prepare(&self, deadline: Option<Instant>) -> Result<(), String> { Ok(()) }
    fn recognize(&self, width: u32, height: u32, gray: &[u8], deadline: Option<Instant>)
        -> Result<OcrOutput, String>;
    fn info(&self) -> OcrInfo { OcrInfo::default() }
}
```

`yolowpdf_ocr::pdf::PdfOcr` implements it, with two entry points:

1. **Managed cache & tier routing** (recommended — defaults to the MIT `ModelSuite::Yolow` BF16 suite, with `Quality::{Auto, Fast, Accurate}`, `RecScript::{Multi, Latin}`, and optional `ModelSuite::Paddle` upstream compatibility):

```rust
use yolowpdf_ocr::{Device, ModelSuite, Quality, RecScript};
use yolowpdf_ocr::pdf::{PdfOcr, PdfOcrConfig};

let cfg = PdfOcrConfig::new(cache_root)
    .suite(ModelSuite::Yolow)
    .quality(Quality::Auto)
    .script(RecScript::Multi) // or RecScript::Latin for English/European scans
    .device(Device::Gpu { index: 0 })
    .formulas(true);
let choice = cfg.selected_models(); // unified { det, rec, layout, formula }
let engine = PdfOcr::new(cfg);
```

2. **Explicit local directories** (benchmarking or custom fine-tuned weights such as `rec_v3_large` / `formula_v3_pro`):

```rust
let engine = PdfOcr::from_dirs(det, rec, math_dirs, Device::Cpu)?;
```

- `det`: text detection model directory (`det_v2`, `det_v3_large`, or PP-OCRv6 det).
- `rec`: text recognition model directory (`rec_v2_multi`, `rec_v2_latin`, `rec_v3_large`, `rec_v3_latin_large`, or PP-OCRv6 rec).
- `math_dirs`: optional `(layout, formula)` directories (`layout_s_v2` / `layout_l_v3` / `PP-DocLayoutV3` and `formula_v2_rect_mtp` / `formula_v3_pro` / `PP-FormulaNet_plus-L`).
- `Device::Cpu` or `Device::Gpu { index }`; a GPU that cannot start falls back to
  CPU and reports why.
- `PdfOcr::engine()` exposes device status and GPU statistics after use.

Models are not bundled. `crates/yolowpdf-ocr/src/models.rs` pins the Hugging Face
repositories, revisions, file sizes and SHA-256 for the MIT-licensed
[`YOLOW92/yolowocr-v2`](https://huggingface.co/YOLOW92/yolowocr-v2) BF16 suite
(`V2_DET`, `V2_REC_MULTI`, `V2_REC_LATIN`, `V2_LAYOUT`, `V2_FORMULA`),
[`YOLOW92/yolowocr-v3`](https://huggingface.co/YOLOW92/yolowocr-v3) (`V3_REC_MULTI`,
`V3_REC_LATIN` used by `Quality::Accurate`, plus mid-schedule preview checkpoints
`V3_DET`, `V3_LAYOUT`, `V3_FORMULA`), and upstream PP-OCRv6 small/medium,
PP-DocLayoutV3 and PP-FormulaNet_plus-L. `ModelChoice::models(formulas)` returns
the exact list of models required for downloading and SHA-256 verification.

Concurrency is document-level: one `extract_with` call processes one document;
run multiple calls on multiple threads/processes for throughput. The OCR engine
has its own GPU batch scheduler, so several document threads can feed one GPU.
