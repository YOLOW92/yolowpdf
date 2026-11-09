# Benchmarks

Public benchmarks, 16 workers on every run, official scoring code.

## Data

| Name | Source | Files | Used for |
|---|---|---:|---|
| `olmocr-all` | [olmOCR-bench](https://huggingface.co/datasets/allenai/olmOCR-bench), revision `54a96a6f` | 1,269 born-digital PDFs | PDF quality and speed |
| `olmocr-scans` | same dataset, `old_scans` + `old_scans_math` | 134 scanned PDFs | OCR quality and speed |
| `pdfjs` | [pdf.js](https://github.com/mozilla/pdf.js), commit `6619bf0c` | 1,089 PDFs | hard-PDF completion and speed |

Data is not redistributed. `benchmarks/datasets.py` builds the manifests; `hashes.sha256`
records every input file. Default data root: `/home/yolow92/yolowpdf-data`, override with
`YOLOWPDF_DATA`.

The PDF score is the official olmOCR-bench score: the mean of per-jsonl pass rates,
including the baseline category added by the official code. We call the official
`olmocr.bench.tests` `run()` for every test; missing output is a failed test. The
scanner is sharded only to use more cores. There is no custom quality metric in the
main tables.

## Concurrency

Every conversion run uses exactly **16 worker processes**. There are no 1/2/4/8/32
runs and no single-thread numbers. A worker handles one PDF at a time. Each worker is
placed in a systemd user scope with `MemoryMax` and `MemorySwapMax=0`; actual live
workers are reported when memory or VRAM admits fewer than 16.

## Time cap for Docling

Light tools run first. For each dataset:

```
T_raw = longest wall time of a light-tool run with completion >= 90%
T_max = max(60 s, T_raw)
```

Docling (CPU and GPU) then runs, in that order, with `--cap-s T_max`. MinerU is excluded from this round by the repository owner.
When the cap is reached the whole worker set is stopped; in-flight and unstarted
items are `capped`/`not_reached` and count as not completed. Official scores treat
them as zero. A second score is reported on the items every tool completed, so
quality and speed stay separable.

The cap is per dataset. `caps.json` records `raw_max_s`, `cap_s`, and the tool that
set it. The 60 s floor exists so that a 2-second light-tool run does not leave
Docling with a one-page sample; it is part of the rule and is reported.

## Sampling

- Per item: `CLOCK_MONOTONIC` wall time, cgroup `memory.peak`, cgroup `cpu.stat`
  delta, cgroup `io.stat` delta, status, bytes, and tool-side milliseconds.
- Timeline: default 1 ms (the actual observed interval is reported), 10 ms for
  capped model runs, GPU via `nvidia-smi` every 50 ms on a separate thread.
- Fast tools therefore have exact per-item microseconds and a dense aggregate
  curve; no rounding to 0.1 ms.

## Commands

```bash
# Data (download + manifests)
python3 -I benchmarks/datasets.py olmocr --subset all  --out benchmarks/manifests/olmocr-all.tsv
python3 -I benchmarks/datasets.py olmocr --subset scans --out benchmarks/manifests/olmocr-scans.tsv
python3 -I benchmarks/datasets.py pdfjs --out benchmarks/manifests/pdfjs.tsv

# PDF: light tools 3 runs, then capped Docling
flock ~/pagesift/tasks/0.80/bench.lock \
  python3 -I benchmarks/run_suite.py pdf --runs 3 --datasets olmocr-all,pdfjs

# OCR: light OCR tools 3 runs, then capped Docling
flock ~/pagesift/tasks/0.80/bench.lock \
  python3 -I benchmarks/run_suite.py ocr --runs 3

# Official scores
python3 -I benchmarks/score_tool.py --data benchmarks/data/olmocr-b1 \
  --candidate benchmarks/results/<date>/olmocr-all/yolowpdf/run1/yolowpdf \
  --out benchmarks/results/<date>/scores/olmocr-all/yolowpdf.json --shards 8
```

`run_suite.py` writes `results/<date>/<dataset>/<tool>/run<N>/` with
`results.jsonl`, `summary.json`, `timeline.csv.gz`, `gpu.csv.gz`, and Markdown
outputs. `report.py` builds `report.md`.
