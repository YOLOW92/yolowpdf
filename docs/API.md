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
image references, scan flag, quality report and OCR flag.

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

`yolowpdf_ocr::pdf::PdfOcr` implements it:

```rust
let engine = PdfOcr::from_dirs(det, rec, math_dirs, Device::Cpu)?;
```

- `det`: PP-OCRv6 detection model directory.
- `rec`: PP-OCRv6 recognition model directory.
- `math_dirs`: optional `(layout, formula)` directories.
- `Device::Cpu` or `Device::Gpu { index }`; a GPU that cannot start falls back to
  CPU and reports why.
- `PdfOcr::engine()` exposes device status and GPU statistics after use.

Models are not bundled. `crates/yolowpdf-ocr/src/models.rs` pins the Hugging Face
repositories, revisions, file sizes and SHA-256 for PP-OCRv6 small/medium,
PP-DocLayoutV3 and PP-FormulaNet_plus-L. Download them into the cache directory
and verify before use.

Concurrency is document-level: one `extract_with` call processes one document;
run multiple calls on multiple threads/processes for throughput. The OCR engine
has its own GPU batch scheduler, so several document threads can feed one GPU.
