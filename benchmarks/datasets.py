"""Build manifests from the public benchmark data.

    python3 -I benchmarks/datasets.py olmocr  [--data DIR] [--subset all|scans] --out FILE
    python3 -I benchmarks/datasets.py pdfjs   [--data DIR] --out FILE

Manifest lines are `id<TAB>path`. For olmOCR the id is the candidate name expected
by the official scorer: `<pdf path without .pdf>_pg1_repeat1`.
"""
import argparse
import json
from pathlib import Path

DEFAULT_DATA = Path("/home/yolow92/yolowpdf-data")


def olmocr(data: Path, subset: str, out: Path):
    bench = data / "olmocr_1403" / "bench_data"
    pdfs = bench / "pdfs"
    names = ["arxiv_math", "headers_footers", "long_tiny_text", "multi_column", "table_tests"] if subset == "all" else ["old_scans", "old_scans_math"]
    seen = {}
    for name in names:
        jl = bench / f"{name}.jsonl"
        if not jl.exists():
            raise SystemExit(f"missing {jl}")
        for line in jl.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            rel = r["pdf"]
            if rel in seen:
                continue
            p = pdfs / rel
            if not p.exists():
                continue
            seen[rel] = p
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        for rel, p in sorted(seen.items()):
            f.write(f"{rel[:-4]}_pg1_repeat1\t{p}\n")
    print(f"{subset}: {len(seen)} PDFs -> {out}")


def pdfjs(data: Path, out: Path):
    root = data / "pdfjs" / "test" / "pdfs"
    files = sorted(p for p in root.glob("*.pdf") if p.is_file())
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        for p in files:
            f.write(f"{p.stem}\t{p}\n")
    print(f"pdfjs: {len(files)} PDFs -> {out}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("kind", choices=["olmocr", "pdfjs"])
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument("--subset", choices=["all", "scans"], default="all")
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    if a.kind == "olmocr":
        olmocr(a.data, a.subset, a.out)
    else:
        pdfjs(a.data, a.out)


if __name__ == "__main__":
    main()
