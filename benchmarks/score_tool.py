"""Run the official olmOCR tests for one candidate directory and aggregate.

    python3 -I benchmarks/score_tool.py --data DIR --candidate DIR --out JSON [--shards 8] [--only-pdfs FILE]
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", type=Path, required=True)
    ap.add_argument("--candidate", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--shards", type=int, default=8)
    ap.add_argument("--only-pdfs", type=Path)
    a = ap.parse_args()
    with tempfile.TemporaryDirectory(prefix="olmscore-") as td:
        procs = []
        for i in range(a.shards):
            out = Path(td) / f"{i}.jsonl"
            cmd = [sys.executable, "-I", str(HERE / "score_olmocr.py"), str(a.data), str(a.candidate), str(out), str(i), str(a.shards)]
            if a.only_pdfs:
                cmd += ["--only-pdfs", str(a.only_pdfs)]
            procs.append((subprocess.Popen(cmd), out))
        for p, _ in procs:
            if p.wait() != 0:
                raise SystemExit(f"scorer failed: {p.returncode}")
        rows = []
        for _, p in procs:
            rows += [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines()]
    cats = {}
    for r in rows:
        cats.setdefault(r["category"], []).append(r["passed"])
    per = {c: sum(v) / len(v) for c, v in sorted(cats.items())}
    nob = [v for c, v in per.items() if c != "baseline"]
    summary = {
        "tests": len(rows),
        "passed": sum(r["passed"] for r in rows),
        "categories": per,
        "olmocr": sum(per.values()) / len(per) if per else None,
        "nobase": sum(nob) / len(nob) if nob else None,
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(summary, ensure_ascii=False, indent=1))
    print(json.dumps(summary, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
