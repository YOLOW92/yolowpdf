# yolowpdf public benchmark, 2026-11-09

16 worker processes per run. Lower ms is better; higher throughput, completion and score are better. The complete raw data is in this directory; commands and rules are in `benchmarks/README.md`.

## Time limits

| Dataset | Longest light-tool run | Applied cap | Set by |
|---|---:|---:|---|
| olmOCR-bench, 1,269 born-digital PDFs | 184.933 s | 184.933 s | pymupdf4llm |
| olmOCR-bench, 134 scanned PDFs | 154.847 s | 154.847 s | yolowpdf-ocr (CPU) |

Docling runs after the light tools and is stopped at the cap. Unfinished pages count as failures in the all-items score.

## PDF: olmOCR-bench, 1,269 born-digital pages

| Tool | Completed | Completion ↑ | Wall s ↓ | Pages/s ↑ | p50 ms ↓ | p95 ms ↓ | Peak MB ↓ | Actual workers | Capped | Official score ↑ | No-baseline ↑ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|
| yolowpdf | 1269/1269 | 1.000 | 0.668 | 1899.96 | 3.2 | 9.0 | 24 | 16 | no | 0.592 | 0.510 |
| pymupdf4llm | 1213/1269 | 0.956 | 184.933 | 6.56 | 845.6 | 1980.5 | 600 | 7 | no | 0.489 | 0.396 |
| pypdf | 1213/1269 | 0.956 | 15.285 | 79.36 | 73.9 | 233.5 | 186 | 16 | no | 0.385 | 0.271 |
| markitdown | 1204/1269 | 0.949 | 88.469 | 13.61 | 172.8 | 518.2 | 1171 | 4 | no | 0.373 | 0.258 |
| docling-gpu | 589/1269 | 0.464 | 185.150 | 3.18 | 364.2 | 1823.8 | 2323 | 2 | yes | 0.116 | 0.046 |
| docling | 89/1269 | 0.070 | 185.840 | 0.48 | 1317.1 | 12149.1 | 2745 | 2 | yes | 0.012 | 0.000 |

Official score is the mean of per-jsonl pass rates from the official olmOCR tests, with baseline included. Missing or capped pages score 0 in this column; it therefore measures quality and completion together. Docling CPU completed 7.0% of pages, GPU 46.4%; their scores are dominated by the cap, not by per-page quality.

Docling detail (same 184.933 s cap):

| Mode | Completed | Pages/s | p50 | p95 | Peak RAM | Actual workers | GPU util | RapidOCR empty warnings | Page-separator limit warnings |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| CPU | 89/1269 | 0.479 | 1,317 ms | 12,149 ms | 2,745 MB | 2 | — | 1 | 1 |
| GPU (cu126) | 589/1269 | 3.181 | 364 ms | 1,824 ms | 2,323 MB | 2 | 75.0% avg, 4,858 MiB | 12 | 2 |

Docling GPU is 6.6x faster than Docling CPU in pages/s and completes 6.6x more pages inside the same cap. It still processes less than half of the 1,269 pages. Its logs contain 12 empty RapidOCR detections on GPU and 2 pages where the separator extractor found more than 5,000 candidates and fell back.

## OCR: olmOCR-bench old_scans + old_scans_math, 134 pages

| Tool | Mode | Completed | Completion ↑ | Wall s ↓ | Pages/s ↑ | p50 ms ↓ | p95 ms ↓ | Peak MB ↓ | Actual workers | Capped | Official score ↑ | No-baseline ↑ |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|
| yolowpdf-ocr-gpu | GPU, small, no formula | 134/134 | 1.000 | 59.675 | 2.25 | 763.5 | 1777.8 | 673 | 2 | no | 0.4075 | 0.1112 |
| yolowpdf-ocr | CPU, small, no formula | 134/134 | 1.000 | 154.847 | 0.86 | 3080.7 | 7355.9 | 616 | 3 | no | 0.4075 | 0.1112 |
| docling-ocr-gpu | GPU | 64/134 | 0.478 | 155.163 | 0.41 | 2827.4 | 11079.6 | 4083 | 2 | yes | 0.2251 | 0.0989 |
| docling-ocr | CPU | 14/134 | 0.104 | 155.749 | 0.09 | 8453.2 | 41710.4 | 2543 | 2 | yes | 0.0519 | 0.0257 |
| yolowpdf-ocr-math-gpu | GPU, small, layout + formula | 134/134 | 1.000 | 882.395 | 0.152 | 43740 (worst page) | — | 2225 | 2 | no | 0.5428 | 0.3143 |

The first self-developed OCR rows use text detection/recognition only; the 36-page `old_scans_math` subset scores 0 there. The formula-enabled GPU row adds PP-DocLayoutV3 + PP-FormulaNet: it completes all 134 pages in 882.4 s, scores **0.5428 / 0.3143**, and recovers `old_scans_math` to 0.4061. This matches the earlier PageSift 1.1 B5 result (0.543 / 0.314), which is a useful extraction-parity check. Formula recognition is expensive: 0.152 pages/s versus 2.245 pages/s without it.

Docling OCR is the same Docling pipeline on scanned pages: CPU completes 14/134 (10.4%) and GPU 64/134 (47.8%) inside 154.847 s. Its official OCR score with missing pages as failures is 0.0519 (CPU) and 0.2251 (GPU). RapidOCR empty detections: 8 on CPU, 27 on GPU.

Tesseract 5.5.0 was installed from Ubuntu packages without root, but its external `pdftoppm`+`tesseract` pipeline was too slow and was stopped; it is excluded from the tables and did not set the cap. MinerU was removed from this round by the repository owner.

## pdf.js hard-PDF corpus

| Tool | Completed | Completion ↑ | Wall s ↓ | Files/s ↑ | p50 ms ↓ | p95 ms ↓ | Peak MB ↓ |
|---|---:|---:|---:|---:|---:|---:|---:|
| yolowpdf | 830/1089 | 0.762 | 0.496 (median of 3) | 1,663 | 0.8 | 14.0 | 104 |
| pymupdf4llm | 841/1089 | 0.772 | 640.861 | 1.312 | 445.3 | 3,075.4 | 4,094 |

The pymupdf4llm run was made while the corpus was still on the NTFS mount `/mnt/e`; it is I/O-bound and is not used for the main PDF comparison. The 1,089-file corpus is public pdf.js at commit `6619bf0c6fa6f7797e6b109cb6d431b0ab1ed00d`.

## What the numbers mean

- The self-developed PDF reader completes all 1,269 born-digital pages in 0.668 s with 16 workers (1,900 pages/s), p50 3.2 ms and p95 9.0 ms. Peak RSS is 24 MB per worker.

- The cap rule is deliberately harsh on slow tools: Docling does not get unlimited time to finish; it is stopped when the longest light tool finishes. Its official score therefore mixes quality and completion. The per-page quality on completed pages is not claimed to be low just because the all-items score is low.

- The self-developed OCR completes all 134 scans on both CPU and GPU. GPU is 2.6x faster (2.245 vs 0.865 pages/s). CPU and GPU produce identical official scores without formulas.
- With layout and formula models, the self-developed OCR still completes all 134 scans but drops to 0.152 pages/s; its official B5 score rises from 0.4075 to 0.5428, matching the earlier PageSift 1.1 result. The cost is 882 s for 134 pages with 2 workers.

- Docling is not competitive on this machine at 16-way concurrency: CPU is memory-heavy and single-page slow; GPU is much better but still capped at 46.4% PDF completion and 47.8% OCR completion.


## Reproduce

```bash
flock ~/pagesift/tasks/0.80/bench.lock python3 -I benchmarks/run_suite.py pdf --runs 1 --datasets olmocr-all
YOLOWPDF_FAST_MATH=1 python3 -I benchmarks/score_tool.py --data benchmarks/data/olmocr-b1 \
  --candidate benchmarks/results/<date>/olmocr-all/yolowpdf/run1/yolowpdf \
  --out benchmarks/results/<date>/scores/olmocr-all/yolowpdf.json --shards 1
```

