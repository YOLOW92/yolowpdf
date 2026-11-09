# yolowpdf

PDF to Markdown and OCR, extracted from [PageSift](https://github.com/YOLOW92/pagesift)
into an independent MIT-licensed workspace.

## Story

PageSift needed a PDF reader that could handle real-world files without a native
PDF library: broken xref tables, encrypted streams, CID fonts without ToUnicode,
multi-column layout, tables, formulas, figures, and scanned pages. The result is
this workspace. `yolowpdf` is the PDF core; `yolowpdf-ocr` adds PP-OCRv6 text
recognition, document layout and formula recognition.

## At a glance

- `yolowpdf`: PDF parsing, text extraction, layout reconstruction, tables,
  formulas, images and scanned-page detection. No model dependencies.
- `yolowpdf-ocr`: pure-Rust OCR on CPU or an NVIDIA GPU, with automatic CPU
  fallback and a report of the reason.
- Public API: `yolowpdf::extract`, `yolowpdf::extract_with`, and the `PageOcr`
  trait; `yolowpdf_ocr::pdf::PdfOcr` implements it.
- Rust 1.89 or newer, edition 2024.
- MIT licensed. Runtime dependencies are permissive; third-party notices are in
  [THIRD-PARTY.md](THIRD-PARTY.md).

## Install

```toml
[dependencies]
yolowpdf = { git = "https://github.com/YOLOW92/yolowpdf" }
# OCR, optional:
yolowpdf-ocr = { git = "https://github.com/YOLOW92/yolowpdf" }
```

The workspace is not published to crates.io yet.

## Usage

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("document.pdf")?;
    let doc = yolowpdf::extract(&bytes)?;
    println!("{}", doc.markdown);
    println!("pages: {}, scanned: {}", doc.pages.len(), doc.scanned);
    Ok(())
}
```

With options:

```rust
let opts = yolowpdf::Options {
    timeout: Some(std::time::Duration::from_secs(30)),
    images: yolowpdf::ImageOptions { markdown: true, ..Default::default() },
    ..Default::default()
};
let doc = yolowpdf::extract_with(&bytes, &opts)?;
```

OCR:

```rust
use std::sync::Arc;
use yolowpdf_ocr::device::Device;
use yolowpdf_ocr::pdf::PdfOcr;

let engine = Arc::new(PdfOcr::from_dirs(
    det_dir, rec_dir, None, Device::Gpu { index: 0 },
)?);
let opts = yolowpdf::Options { ocr: Some(engine), ..Default::default() };
let doc = yolowpdf::extract_with(&bytes, &opts)?;
```

The models are downloaded by the caller, not by the core crate. The OCR crate
contains the pinned model list (repository, revision, size, SHA-256); see
`crates/yolowpdf-ocr/src/models.rs` and `docs/API.md`.

## Performance

Public data, official scoring code, exactly **16 worker processes** per run. Docling
runs last and is stopped at the longest light-tool wall time; unfinished pages count
as failures. Full method, raw JSONL, timelines and per-tool logs:
[`benchmarks/results/2026-11-09/report.md`](benchmarks/results/2026-11-09/report.md).

**PDF — olmOCR-bench, 1,269 born-digital pages**

| Tool | Completed ↑ | Wall s ↓ | Pages/s ↑ | p50 ms ↓ | p95 ms ↓ | Peak MB ↓ | Official score ↑ |
|---|---:|---:|---:|---:|---:|---:|---:|
| **yolowpdf** | **1269/1269** | **0.668** | **1900** | **3.2** | **9.0** | **24** | **0.592** |
| pymupdf4llm | 1213/1269 | 184.933 | 6.56 | 845.6 | 1980.5 | 600 | 0.489 |
| pypdf | 1213/1269 | 15.285 | 79.4 | 73.9 | 233.5 | 186 | 0.385 |
| MarkItDown | 1204/1269 | 88.469 | 13.6 | 172.8 | 518.2 | 1171 | 0.373 |
| Docling GPU | 589/1269 | 185.150 | 3.18 | 364.2 | 1823.8 | 2323 | 0.116 |
| Docling CPU | 89/1269 | 185.840 | 0.48 | 1317.1 | 12149.1 | 2745 | 0.012 |

Docling runs under the same 184.933 s cap as every heavy tool. GPU usage averaged
75.0% and 4,858 MiB of VRAM; CPU used 2 workers and no GPU. The CPU run completed
7.0% of pages, the GPU run 46.4%. The all-items official score therefore mixes
quality and completion; it is not a claim that Docling's pages are that bad.

**OCR — olmOCR-bench old_scans + old_scans_math, 134 scanned pages**

| Tool | Mode | Completed ↑ | Wall s ↓ | Pages/s ↑ | Peak MB ↓ | Official score ↑ | No-baseline ↑ |
|---|---|---:|---:|---:|---:|---:|---:|
| **yolowpdf-ocr** | GPU, layout + formula | **134/134** | 882.395 | 0.152 | 2225 | **0.5428** | **0.3143** |
| yolowpdf-ocr | GPU, text only | **134/134** | **59.675** | **2.245** | 673 | 0.4075 | 0.1112 |
| yolowpdf-ocr | CPU, text only | **134/134** | 154.847 | 0.865 | 616 | 0.4075 | 0.1112 |
| Docling OCR GPU | default | 64/134 | 155.163 | 0.41 | 4083 | 0.2251 | 0.0989 |
| Docling OCR CPU | default | 14/134 | 155.749 | 0.09 | 2543 | 0.0519 | 0.0257 |

The formula-enabled run matches the earlier PageSift 1.1 B5 result (0.543 / 0.314),
which is an extraction-parity check. Formula recognition costs 15× throughput.

**Hard PDFs — pdf.js, 1,089 files**

| Tool | Completed ↑ | Wall s ↓ | Files/s ↑ | Peak MB ↓ |
|---|---:|---:|---:|---:|
| yolowpdf | 830/1089 | 0.496 | 1663 | 104 |
| pymupdf4llm | 841/1089 | 640.861 | 1.31 | 4094 |

The pymupdf4llm pdf.js run was I/O-bound on the NTFS mount and is not used for the
main PDF comparison.

## Features and guarantees

- Parses xref tables and streams, object streams, damaged files, encryption,
  Flate, Brotli, JPEG, JPEG 2000, JBIG2, CCITT, PNG, TIFF and WebP images.
- Rebuilds paragraphs, headings, multi-column reading order, headers/footers,
  tables and inline math; exports images independently of OCR.
- OCR is lazy: a document without scanned pages pays no model-loading cost.
- A parser panic is caught and returned as an error; timeouts are explicit.
- `cargo test --release --workspace` covers the parser, fonts, tables, OCR
  plumbing, stress cases and fuzz regressions. Fuzz targets are in
  `crates/yolowpdf/fuzz`.

## License

MIT. See [LICENSE](LICENSE), [THIRD-PARTY.md](THIRD-PARTY.md) and
[UPSTREAM.md](UPSTREAM.md).
