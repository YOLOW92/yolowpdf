# yolowpdf public benchmark, 2026-11-26 (YolowOCR-Suite v2 BF16)

All runs below were executed on the same local machine (Linux, 16 vCPU, 15 GiB RAM, NVIDIA GeForce GTX 1070 8 GiB) using `benchmarks/driver.py` (`--workers 16`, cgroup v2 `memory.peak`, `flock bench.lock`) and scored with `YOLOWPDF_FAST_MATH=1` via `benchmarks/score_tool.py`. Competitor and v1 baseline rows are cited directly from `benchmarks/results/2026-11-09/report.md` (no competitors were re-run).

## Model Footprint (v2 BF16 Clean-Room vs v1 Upstream)

| Component | v1 Upstream (`PP-OCRv6` / `PP-DocLayoutV3` / `PP-FormulaNet_plus-L`) | v2 Clean-Room (`YolowOCR-Suite v2` BF16) | Size Ratio |
|---|---:|---:|---:|
| Text Det | 9.94 MB | 5.05 MB (`det_v2`) | 1.97× smaller |
| Text Rec (Multi) | 21.20 MB | 10.74 MB (`rec_v2_multi`, 18,710 vocab) | 1.97× smaller |
| Text Rec (Latin) | — | 6.24 MB (`rec_v2_latin`, 254 vocab) | 3.40× smaller |
| Layout | 133.27 MB | 26.81 MB (`layout_s_v2_r2`, 640×640, 3L) | 4.97× smaller |
| Formula | 727.63 MB | 66.99 MB (`formula_v2_rect_mtp`, 224×672, MTP-4) | 10.86× smaller |
| **Full Suite (Multi)** | **892.04 MB** | **109.59 MB** | **8.14× smaller** |
| **Full Suite (Latin)** | — | **105.09 MB** | **8.49× smaller** |

## OCR: olmOCR-bench `old_scans` + `old_scans_math` (134 pages, 1,118 official tests)

| Tool | Model / Mode | Completed | Completion ↑ | Wall s ↓ | Pages/s ↑ | p50 ms ↓ | p95 ms ↓ | Peak MB ↓ | Workers | Score ↑ | No-base ↑ | `old_scans` ↑ | `old_scans_math` ↑ |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **yolowpdf-ocr-latin-gpu (v2)** | GPU, det+rec-latin | 134/134 | 1.000 | **37.562** | **3.567** | **842.2** | **2918.2** | 628.2 | 4 | 0.3891 | 0.0837 | 0.1673 | 0.0000 |
| **yolowpdf-ocr-gpu (v2)** | GPU, det+rec-multi | 134/134 | 1.000 | **46.548** | **2.879** | 1039.6 | 3746.0 | 665.2 | 4 | 0.3897 | 0.0846 | 0.1692 | 0.0000 |
| **yolowpdf-ocr (v2)** | CPU, det+rec-multi | 134/134 | 1.000 | **106.580** | **1.257** | 6578.3 | 23376.2 | **592.7** | 11 | 0.3897 | 0.0846 | 0.1692 | 0.0000 |
| **yolowpdf-ocr-latin-math-gpu (v2)** | GPU, latin + layout + formula | 134/134 | 1.000 | **361.916** | **0.370** | 9257.2 | 24053.5 | 916.1 | 4 | 0.4793 | 0.2189 | 0.1540 | 0.2838 |
| **yolowpdf-ocr-math-gpu (v2)** | GPU, multi + layout + formula | 134/134 | 1.000 | **367.664** | **0.364** | 9319.3 | 26281.3 | 938.5 | 4 | **0.4799** | **0.2199** | 0.1559 | **0.2838** |
| **yolowpdf-ocr-math (v2)** | CPU, multi + layout + formula | 134/134 | 1.000 | **388.367** | **0.345** | 16861.1 | 43687.7 | 831.2 | 7 | **0.4799** | **0.2199** | 0.1559 | **0.2838** |
| yolowpdf-ocr-gpu (v1, 2026-11-09) | GPU, PP-OCRv6 small | 134/134 | 1.000 | 59.675 | 2.25 | 763.5 | 1777.8 | 673 | 2 | 0.4075 | 0.1112 | 0.2224 | 0.0000 |
| yolowpdf-ocr (v1, 2026-11-09) | CPU, PP-OCRv6 small | 134/134 | 1.000 | 154.847 | 0.86 | 3080.7 | 7355.9 | 616 | 3 | 0.4075 | 0.1112 | 0.2224 | 0.0000 |
| yolowpdf-ocr-math-gpu (v1, 2026-11-09) | GPU, PP-OCRv6 + LayoutV3 + FormulaNet-L | 134/134 | 1.000 | 882.395 | 0.152 | — | — | 2225 | 2 | 0.5428 | 0.3143 | 0.2224 | 0.4061 |
| docling-ocr-gpu (2026-11-09, capped) | GPU, cap 154.847 s | 64/134 | 0.478 | 155.163 | 0.41 | 2827.4 | 11079.6 | 4083 | 2 | 0.2251 | 0.0989 | 0.1977 | 0.0000 |
| docling-ocr (2026-11-09, capped) | CPU, cap 154.847 s | 14/134 | 0.104 | 155.749 | 0.09 | 8453.2 | 41710.4 | 2543 | 2 | 0.0519 | 0.0257 | 0.0513 | 0.0000 |

## PDF Core: olmOCR-bench Born-Digital (1,269 pages, 7,295 tests) & `pdf.js` Hard-PDF (1,089 files)

| Tool | Dataset | Completed | Completion ↑ | Wall s ↓ | Throughput ↑ | p50 ms ↓ | p95 ms ↓ | Peak MB ↓ | Score ↑ | No-base ↑ |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **yolowpdf (2026-11-26)** | olmocr-all | 1269/1269 | 1.000 | **0.562** | **2256.3 p/s** | 3.4 | 10.1 | 37.8 | **0.6950** | **0.6341** |
| yolowpdf (2026-11-09) | olmocr-all | 1269/1269 | 1.000 | 0.668 | 1900.0 p/s | 3.2 | 9.0 | 24.0 | 0.5915 | 0.5100 |
| pymupdf4llm (2026-11-09) | olmocr-all | 1213/1269 | 0.956 | 184.933 | 6.56 p/s | 845.6 | 1980.5 | 600 | 0.489 | 0.396 |
| pypdf (2026-11-09) | olmocr-all | 1213/1269 | 0.956 | 15.285 | 79.36 p/s | 73.9 | 233.5 | 186 | 0.385 | 0.271 |
| markitdown (2026-11-09) | olmocr-all | 1204/1269 | 0.949 | 88.469 | 13.61 p/s | 172.8 | 518.2 | 1171 | 0.373 | 0.258 |
| docling-gpu (2026-11-09, capped) | olmocr-all | 589/1269 | 0.464 | 185.150 | 3.18 p/s | 364.2 | 1823.8 | 2323 | 0.116 | 0.046 |
| docling (2026-11-09, capped) | olmocr-all | 89/1269 | 0.070 | 185.840 | 0.48 p/s | 1317.1 | 12149.1 | 2745 | 0.012 | 0.000 |
| **yolowpdf (2026-11-26)** | pdfjs (median of 3) | 830/1089 | 0.762 | 0.737 | 1125.8 f/s | 0.2 | 12.5 | 102.3 | — | — |
| yolowpdf (2026-11-09) | pdfjs (median of 3) | 830/1089 | 0.762 | 0.496 | 1663 f/s | 0.8 | 14.0 | 104 | — | — |
| pymupdf4llm (2026-11-09) | pdfjs | 841/1089 | 0.772 | 640.861 | 1.312 f/s | 445.3 | 3075.4 | 4094 | — | — |

## Key Diagnostic Findings

1. **Exact Python-vs-Rust Numerical Parity**:
   - `layout_s_v2_r2`: Rust vs PyTorch `from_pretrained` logits/boxes/order match within `~1e-6`.
   - `formula_v2_rect_mtp`: 5/6 real validation equations (up to 471 tokens / 1,141 chars) match PyTorch character-for-character (`1.0000`), and the 6th matches at `0.9984`. MTP-4 self-speculative greedy decoding runs at **3.1–3.5 ms/token on CPU** and is bit-for-bit identical to greedy decoding.
   - CPU vs GPU OCR scores are identical (`0.3897` text-only; `0.4799` with math).
2. **Primary Bottlenecks Behind the v2 vs v1 Accuracy Gap (`0.4799` vs `0.5428`)**:
   - **`layout_s_v2_r2` false-positive `display_formula` boxes on plain text scans**: triggered **600 `$$` blocks across 96/98 plain-text `old_scans` pages** (vs 0 on v1 `PP-DocLayoutV3`). This both consumed ~65–70% of the `math` wall time and dropped `old_scans` `present` pass rate from `20/279` (7.2%) to `12/279` (4.3%).
   - **Degenerate repetition loops & KaTeX-unsupported macros in `formula_v2_rect_mtp`**: 116 of 860 total `$$` blocks hit degenerate whitespace repetition (`\qquad\qquad...` / `\ \ \ ...`) until `max_length = 1024`, causing unclosed `\begin{array}` KaTeX parse errors on otherwise well-recognized equations; `\boldmath` also caused KaTeX parse errors.
   - **Historical cursive/degraded text recognition (`old_scans`)**: `det_v2` + `rec_v2_multi` scored `0.1692` vs v1 `PP-OCRv6_small` `0.2224` (`-5.32 pp`); `rec_v2_latin` (6.24 MB) matched `rec_v2_multi` (`0.1673` vs `0.1692`) while running 24% faster on GPU (`3.567` vs `2.879 p/s`).
