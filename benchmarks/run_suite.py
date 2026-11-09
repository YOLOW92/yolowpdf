"""Run the 16-worker suite: light tools first, then capped Docling/MinerU.

    python3 -I benchmarks/run_suite.py pdf  [--runs 3] [--tools ...] [--datasets ...] [--smoke N]
    python3 -I benchmarks/run_suite.py ocr  [--runs 3] ...

Every run uses exactly 16 workers. T_max = max(60 s, longest light-tool run with
completion >= 0.9) per dataset; heavy tools get --cap-s T_max and run once.
"""
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("YOLOWPDF_ROOT", Path.home() / "yolowpdf"))
DATA = Path(os.environ.get("YOLOWPDF_DATA", Path("/mnt/e/yolowpdf-data")))
DATE = time.strftime("%Y-%m-%d")
OUT = Path(os.environ.get("YOLOWPDF_RESULTS", ROOT / "benchmarks/results" / DATE))
DRIVER = [sys.executable, "-I", str(HERE / "driver.py")]

PDF_LIGHT = ["yolowpdf", "pymupdf4llm", "pypdf", "markitdown"]
PDF_HEAVY = ["docling", "docling-gpu"]
OCR_LIGHT = ["yolowpdf-ocr", "yolowpdf-ocr-gpu", "yolowpdf-ocr-math-gpu", "tesseract", "paddleocr"]
OCR_HEAVY = ["docling-ocr", "docling-ocr-gpu"]

DATASETS = {
    "olmocr-all": HERE / "manifests/olmocr-all.tsv",
    "pdfjs": HERE / "manifests/pdfjs.tsv",
    "olmocr-scans": HERE / "manifests/olmocr-scans.tsv",
}


def run_one(dataset, tool, run, cap=0.0, smoke=0, timeout=600.0):
    manifest = DATASETS[dataset]
    lines = [l for l in manifest.read_text().splitlines() if l.strip()]
    if smoke:
        lines = lines[:smoke]
        m = OUT / dataset / f"manifest-smoke{smoke}.tsv"
    else:
        m = manifest
    if smoke:
        m.parent.mkdir(parents=True, exist_ok=True)
        m.write_text("\n".join(lines) + "\n")
    out = OUT / dataset / tool / f"run{run}"
    cmd = DRIVER + ["--tool", tool, "--manifest", str(m), "--out", str(out),
                    "--workers", "16", "--timeout", str(timeout), "--sample-us", "1000"]
    if cap > 0:
        cmd += ["--cap-s", str(cap), "--sample-us", "10000"]
    # OCR gets more per-item headroom; PDF pages are cheap.
    if tool.startswith(("yolowpdf-ocr", "docling", "mineru", "paddleocr", "tesseract")):
        cmd[cmd.index("--timeout") + 1] = "1800"
    print(f"[{time.strftime('%T')}] {dataset}/{tool} run{run} cap={cap or 'none'}", flush=True)
    r = subprocess.run(cmd)
    if r.returncode != 0:
        raise SystemExit(f"driver failed: {dataset}/{tool} run{run}")
    return json.loads((out / tool / "summary.json").read_text())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["pdf", "ocr"])
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--smoke", type=int, default=0)
    ap.add_argument("--tools", default="")
    ap.add_argument("--datasets", default="")
    a = ap.parse_args()
    if a.mode == "pdf":
        light, heavy = PDF_LIGHT, PDF_HEAVY
        default_ds = ["olmocr-all", "pdfjs"]
    else:
        light, heavy = OCR_LIGHT, OCR_HEAVY
        default_ds = ["olmocr-scans"]
    if a.tools:
        want = set(a.tools.split(","))
        light = [t for t in light if t in want]
        heavy = [t for t in heavy if t in want]
    datasets = a.datasets.split(",") if a.datasets else default_ds
    OUT.mkdir(parents=True, exist_ok=True)
    caps = json.loads((OUT / "caps.json").read_text()) if (OUT / "caps.json").exists() else {}
    for ds in datasets:
        summaries = []
        for tool in light:
            for run in range(1, a.runs + 1):
                summaries.append((tool, run_one(ds, tool, run, smoke=a.smoke)))
        if light:
            good = [s for t, s in summaries if s["completion"] >= 0.9 and s["wall_s"] > 0]
            raw = max((s["wall_s"] for s in good), default=0.0)
            cap = max(60.0, raw)
            caps[ds] = {"raw_max_s": raw, "cap_s": cap,
                        "set_by": max(good, key=lambda s: s["wall_s"])["tool"] if good else None}
        elif ds not in caps:
            raise SystemExit(f"no cap for {ds}; run light tools first or pass a caps.json")
        else:
            cap = caps[ds]["cap_s"]
        print(f"[{time.strftime('%T')}] cap {ds}: {caps[ds]}", flush=True)
        (OUT / "caps.json").write_text(json.dumps(caps, ensure_ascii=False, indent=1))
        for tool in heavy:
            run_one(ds, tool, 1, cap=cap, smoke=a.smoke)
    (OUT / "caps.json").write_text(json.dumps(caps, ensure_ascii=False, indent=1))
    print(f"[{time.strftime('%T')}] done -> {OUT}", flush=True)


if __name__ == "__main__":
    main()
