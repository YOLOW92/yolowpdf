"""Score candidate Markdown with the official olmOCR-bench tests.

Run from the olmocr-bench checkout (or set YOLOWPDF_OLMOCR_BENCH) so that
`olmocr.bench.tests` is importable. This wrapper only shards tests; every test
is `olmocr.bench.tests`'s own `run()` and the score is the official mean of
per-jsonl pass rates, including the baseline category.

    python3 -I score_olmocr.py <bench_data> <candidate_dir> <out.json> [shard nshards] [--only-pdfs FILE]
"""
import json
import os
import sys
import zlib
from pathlib import Path

bench = Path(os.environ.get("YOLOWPDF_OLMOCR_BENCH", Path.home() / "pagesift-bench-env/tools/olmocr-bench"))
sys.path.insert(0, str(bench))
if os.environ.get("YOLOWPDF_FAST_MATH") == "1":
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import bench_fast  # noqa: F401,E402  patches the official MathTest/KaTeX path only
from olmocr.bench.tests import BaselineTest, load_tests  # noqa: E402


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = None
    if "--only-pdfs" in sys.argv:
        only = set(Path(sys.argv[sys.argv.index("--only-pdfs") + 1]).read_text().splitlines())
    data, cand, out = Path(args[0]), Path(args[1]), Path(args[2])
    shard, nshards = (int(args[3]), int(args[4])) if len(args) >= 5 else (0, 1)
    tests = []
    for jl in sorted(data.glob("*.jsonl")):
        tests += [(jl.stem, t) for t in load_tests(str(jl))]
    pdfs = sorted({t.pdf for _, t in tests})
    for pdf in pdfs:
        if not any(t.type == "baseline" and t.pdf == pdf for _, t in tests):
            tests.append(("baseline", BaselineTest(id=f"{pdf}_baseline", pdf=pdf, page=1, type="baseline")))
    rows = []
    for cat, t in tests:
        if zlib.crc32(t.pdf.encode()) % nshards != shard:
            continue
        if only is not None and t.pdf not in only:
            continue
        md = cand / (os.path.splitext(t.pdf)[0] + f"_pg{t.page}_repeat1.md")
        if not md.exists():
            passed, why = False, "missing"
        else:
            try:
                passed, why = t.run(md.read_text(encoding="utf-8", errors="replace"))
            except Exception as e:  # noqa: BLE001
                passed, why = False, f"error: {e}"
        rows.append({"pdf": t.pdf, "category": cat, "type": t.type, "id": t.id, "passed": bool(passed), "why": str(why)[:500]})
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"shard {shard}/{nshards}: {len(rows)} tests -> {out}")


if __name__ == "__main__":
    main()
