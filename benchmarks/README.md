# Benchmarks

Exploratory runs on public benchmark datasets. Actual concurrency and measurement limits are described in the results report.

## Data

| Name | Source | Files | Used for |
|---|---|---:|---|
| `olmocr-all` | [olmOCR-bench](https://huggingface.co/datasets/allenai/olmOCR-bench), revision `54a96a6f` | 1,269 born-digital PDFs | PDF quality and speed |
| `olmocr-scans` | same dataset, `old_scans` + `old_scans_math` | 134 scanned PDFs | OCR quality and speed |
| `pdfjs` | [pdf.js](https://github.com/mozilla/pdf.js), commit `6619bf0c` | 1,089 PDFs | hard-PDF completion and speed |

Data is not redistributed. `benchmarks/datasets.py` builds the manifests; `hashes.sha256`
records every input file. Default data root: `/home/yolow92/yolowpdf-data`, override with
`YOLOWPDF_DATA`.

The scorer invokes olmOCR tests, treating missing output as failure. Some results
use `YOLOWPDF_FAST_MATH=1`, an unverified math prefilter, so equivalence to the
unmodified official scorer is not established. Scores mix completion and quality.
A nonempty result can include placeholder text; it does not prove useful extraction.
Common-completed-item quality scoring was not completed for the published run.

## Concurrency

The main runs targeted 16 workers; memory and VRAM admission often reduced actual
live workers. The separately recorded formula run requested two workers. Each worker
handles one PDF at a time and is placed in a systemd user scope. See the report
and `summary.json` for actual peak live workers.

## Time cap for Docling

Light tools run first. For each dataset:

```
T_raw = longest wall time of a light-tool run with completion >= 90%
T_max = max(60 s, T_raw)
```

Docling (CPU and GPU) then runs, in that order, with `--cap-s T_max`.
When the cap is reached the whole worker set is stopped; in-flight and unstarted
items are `capped`/`not_reached` and count as not completed. The scorer treats missing output as failed tests. No completed common-items
quality comparison is available for this run.

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
  --out benchmarks/results/<date>/scores/olmocr-all/yolowpdf.json --shards 1
```

`run_suite.py` writes `results/<date>/<dataset>/<tool>/run<N>/` with
`results.jsonl`, `summary.json`, `timeline.csv.gz`, `gpu.csv.gz`, and Markdown
outputs. `report.py` builds `report.md`.
