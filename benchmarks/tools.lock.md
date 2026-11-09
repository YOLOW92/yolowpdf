# Tool versions and environments

Machine: Intel Core i7-10700 (8 cores / 16 threads), 15 GB RAM, NVIDIA GTX 1070
8 GB (sm_61), Linux x86-64. Python 3.12.15 via uv. Rust 1.99.0.

| Tool | Version | Environment | Notes |
|---|---|---|---|
| yolowpdf | 0.1.0, this workspace | `target/release/examples/bench_worker` | fat LTO, 16 processes |
| yolowpdf-ocr | 0.1.0, this workspace | `target/release/examples/bench_ocr_worker` | PP-OCRv6 small/medium; CPU or GPU |
| pymupdf4llm | pymupdf 1.28.2 | `~/pagesift-bench-env/tools/py` | `to_markdown` defaults |
| pypdf | 6.21.2 | same | `PdfReader` + `extract_text` |
| MarkItDown | 0.1.9 | same | `MarkItDown().convert(path).text_content` |
| Docling | 2.142.1 | `~/yolowpdf-bench/py/docling-gpu` | torch 2.14.1+cu126; CPU mode and GPU mode |
| MinerU | 4.0.12 | not scored | removed from this round by the repository owner |
| Tesseract | 5.5.0 | extracted Ubuntu .deb under `~/yolowpdf-bench/tesseract` | installed without root; external pdftoppm pipeline was too slow, excluded from the tables |
| PaddleOCR | pending | `~/yolowpdf-bench/py/paddleocr` | CPU wheel; not the same model revision as PP-OCRv6 |

Model files (downloaded, not redistributed):

- PP-OCRv6 small det/rec, medium det/rec.
- PP-DocLayoutV3, PP-FormulaNet_plus-L.
- Pinned revisions and SHA-256 are in `crates/yolowpdf-ocr/src/models.rs`.

Data revisions:

- olmOCR-bench: `54a96a6fb6a2bd3b297e59869491db4d3625b711`.
- pdf.js: `6619bf0c6fa6f7797e6b109cb6d431b0ab1ed00d`.

The light Python environment is the existing `~/pagesift-bench-env/tools/py`
(PyMuPDF 1.28.2, pypdf 6.21.2, MarkItDown 0.1.9). No packages were added to it
for this benchmark.
