"""Score every tool in the official olmOCR datasets, then common-item scores."""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("YOLOWPDF_ROOT", Path.home() / "yolowpdf"))
DATE = __import__("time").strftime("%Y-%m-%d")
RES = Path(os.environ.get("YOLOWPDF_RESULTS", ROOT / "benchmarks/results" / DATE))
PYTHON = os.environ.get("YOLOWPDF_SCORE_PY", str(Path.home() / "pagesift-bench-env/tools/py/bin/python"))

DATA = {
    "olmocr-all": HERE / "data/olmocr-b1",
    "olmocr-scans": HERE / "data/olmocr-b5",
}


def tools_for(ds):
    return sorted(p.name for p in (RES / ds).iterdir() if p.is_dir() and (p / "run1").exists())


def run(ds, tool, only=None, kind="scores"):
    out = RES / kind / ds / f"{tool}.json"
    cmd = [PYTHON, "-I", str(HERE / "score_tool.py"), "--data", str(DATA[ds]),
           "--candidate", str(RES / ds / tool / "run1" / tool), "--out", str(out), "--shards", "8"]
    if only:
        cmd += ["--only-pdfs", str(only)]
    print(f"[{__import__('time').strftime('%T')}] score {ds}/{tool} -> {out}", flush=True)
    subprocess.run(cmd, check=True, env={**os.environ, "PLAYWRIGHT_BROWSERS_PATH": str(Path.home() / "yolowpdf-bench/ms-playwright")})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--datasets", default="olmocr-all,olmocr-scans")
    ap.add_argument("--common", action="store_true")
    a = ap.parse_args()
    for ds in a.datasets.split(","):
        if ds not in DATA:
            continue
        tools = tools_for(ds)
        if not tools:
            continue
        if a.common:
            only = RES / "common" / f"{ds}.txt"
            subprocess.run([sys.executable, "-I", str(HERE / "common_ids.py"), str(RES), ds] + tools, check=True)
            for t in tools:
                run(ds, t, only, "scores-common")
        else:
            for t in tools:
                run(ds, t)


if __name__ == "__main__":
    main()
