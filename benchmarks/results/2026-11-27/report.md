# yolowpdf OCR benchmark, 2026-11-27 (YolowOCR-Suite v3 Large / Pro BF16)

All new rows below were executed on the same local machine (Linux, 16 vCPU, 15 GiB RAM, NVIDIA GeForce GTX 1070 8 GiB) using `benchmarks/driver.py` (`--workers 16`, cgroup v2 `memory.peak`) and scored with `YOLOWPDF_FAST_MATH=1` via `benchmarks/score_tool.py`. v2 and v1/competitor rows are cited from `benchmarks/results/2026-11-26/report.md` and `benchmarks/results/2026-11-09/report.md`.

## v3 Trained BF16 Artifacts (`exports/v3_bf16`, zero Rust inference changes)

| Model | Params | BF16 Size | Steps Reached | Validation Metric |
|---|---:|---:|---:|---|
| `det_v3_large` (`pp_ocrv6_medium_det`) | 21.96M | 44.20 MB | 6,645 / 24,000 | `F1=0.9179` (`P=0.8777`, `R=0.9621`) |
| `rec_v3_large` (`LCNetV4-L` + 4L SVTR `h=256`, 18,710 vocab) | 22.18M | 44.4 MB | 38,412 / 40,000 | `1-CER=0.9673`, `EM=42.0%` |
| `rec_v3_latin_large` (`LCNetV4-L` + 4L SVTR `h=256`, 254 vocab) | 17.43M | 35.2 MB | 18,000 / 18,000 | `1-CER=0.9651`, `EM=36.3%` |
| `layout_l_v3` (`HGNetV2-L`, 6L 300-Query, `800×800`) | 33.18M | 67.0 MB | 4,262 / 24,000 | `F1_all=0.9411`, `F1_F=0.9176`, `plain_fp=0.00/pg` (val) |
| `formula_v3_pro` (12L ViT `h=768` + 8L Dec `d=512` + MTP-4, `256×768`) | 127.37M | 255.2 MB | 15,811 / 35,000 | `mean_sim=0.8227`, `hwe=0.7808 (lp=0.00)`, `spe=0.9253`, `mtp_tau=3.94×` |

## OCR: olmOCR-bench `old_scans` + `old_scans_math` (134 pages, 1,118 official tests)

| Configuration | Models | Completed | Wall s ↓ | Peak MB ↓ | Score ↑ | No-base ↑ | `old_scans` ↑ | `old_scans_math` ↑ | Plain `$$` FP ↓ | Formula Loops ↓ |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **det_v2 + rec_v3_large (GPU)** | 5.05 + 44.4 MB | 134/134 | 133.7 | 791.3 | **0.3993** | **0.0990** | **0.1958** | 0.0022 | 0 | 0 |
| **det_v2 + rec_v3_latin_large (GPU)** | 5.05 + 35.2 MB | 134/134 | 138.0 | 706.9 | 0.3948 | 0.0922 | 0.1844 | 0.0000 | 0 | 0 |
| v3-latin-gpu (`det_v3_large + rec_v3_latin_large`) | 44.2 + 35.2 MB | 134/134 | 156.3 | 774.9 | 0.3929 | 0.0894 | 0.1787 | 0.0000 | 0 | 0 |
| v3-multi-gpu (`det_v3_large + rec_v3_large`) | 44.2 + 44.4 MB | 134/134 | 161.8 | 932.9 | 0.3891 | 0.0837 | 0.1673 | 0.0000 | 0 | 0 |
| v3-math-gpu (all 4 v3 models) | 410.8 MB | 134/134 | 232.3 | 1890.1 | 0.3987 | 0.0980 | 0.1654 | 0.0306 | **25** (vs 600) | **3** |
| v3-formula-on-pp-layout-gpu | `det_v3 + rec_v3 + PP-Layout + formula_v3_pro` | 134/134 | 332.7 | 1712.7 | 0.4728 | 0.2092 | 0.1673 | 0.2511 | 0 | **4 / 190 (2.1%)** |
| yolowpdf-ocr-gpu (v2, 2026-11-26) | `det_v2 + rec_v2_multi` (15.8 MB) | 134/134 | **46.5** | **665.2** | 0.3897 | 0.0846 | 0.1692 | 0.0000 | 0 | 0 |
| yolowpdf-ocr-math-gpu (v2, 2026-11-26) | all 4 v2 models (109.6 MB) | 134/134 | 367.7 | 938.5 | 0.4799 | 0.2199 | 0.1559 | 0.2838 | 600 | 116 / 860 (13.5%) |
| yolowpdf-ocr-gpu (v1, 2026-11-09) | `PP-OCRv6_small` (31.1 MB) | 134/134 | 59.7 | 673.0 | 0.4075 | 0.1112 | 0.2224 | 0.0000 | 0 | 0 |
| yolowpdf-ocr-math-gpu (v1, 2026-11-09) | `PP-OCRv6 + LayoutV3 + FormulaNet-L` (892 MB) | 134/134 | 882.4 | 2225.0 | 0.5428 | 0.3143 | 0.2224 | 0.4061 | 0 | — |
| docling-ocr-gpu (2026-11-09, capped) | GPU, cap 154.8 s | 64/134 | 155.2 | 4083.0 | 0.2251 | 0.0989 | 0.1977 | 0.0000 | — | — |

## Key Findings

1. **`rec_v3_large` + `det_v2` jumps `+2.66 pp` on `old_scans` (`0.1692 → 0.1958`)**: virtually matching Docling-GPU (`0.1977` at 4,083 MB RAM / 47.8% completion) while finishing 100% of pages in 791 MB RAM with 49.5 MB total weights. `rec_v3_latin_large` also gains `+1.71 pp` over `rec_v2_latin` (`0.1673 → 0.1844`).
2. **Formula repetition loops (v2 Defect 2) reduced by 6.4×**: `formula_v3_pro` dropped degenerate loops from `116/860` (`13.5%`) to `4/190` (`2.1%`), with lossless MTP-4 acceleration reaching `3.94×` on validation.
3. **Plain-text false-positive formula boxes (v2 Defect 1) reduced by 24×**: `layout_l_v3` cut plain-text `$$` boxes from `600` to `25`.
4. **Budget-truncated models identified cleanly**: within the 145 CU budget, `det_v3_large` (6.6k/24k), `layout_l_v3` (4.3k/24k, prior to Stage-2 `mask_enhanced=ON` at 13.2k), and `formula_v3_pro` (15.8k/35k) had not finished their full schedules, explaining why fully-trained `det_v2` (18k) + `rec_v3_large` (38.4k) outperforms `det_v3_large` (6.6k) + `rec_v3_large`, and why `layout_l_v3` recalled 51/190 real math boxes despite slashing plain-text FP 24×.
