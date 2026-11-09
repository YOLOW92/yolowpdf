# Upstream

This repository extracts the PDF and OCR code from PageSift 1.2.4
(https://github.com/YOLOW92/pagesift), commit `07a7053e` (`release: 1.2.4`).

Copied and renamed:

| Upstream crate | Here |
|---|---|
| `pagesift-pdf` | `crates/yolowpdf` |
| `pagesift-ocr` | `crates/yolowpdf-ocr` |
| `pagesift-stat` | `crates/yolowpdf-stat` |
| `pagesift-infer` | `crates/yolowpdf-infer` |
| `pagesift-gpu` | `crates/yolowpdf-gpu` |
| `pagesift-testkit` | `crates/yolowpdf-testkit` |
| `pagesift-sys` | `crates/yolowpdf-sys` |
| `kernels/` | `kernels/` |

The code is the same implementation, with package names, repository metadata,
the license (MIT here; PageSift itself remains AGPL-3.0-only), and benchmark
paths changed. The copyright owner relicensed this extracted code under MIT.
No third-party AGPL runtime dependency is used.

Public API entry points: `yolowpdf::extract`, `yolowpdf::extract_with`,
`yolowpdf_ocr::pdf::PdfOcr`, and the `PageOcr` trait in the core crate.
