"""Write the PDF ids that every listed tool completed, for common-items scoring."""
import json
import sys
from pathlib import Path

res = Path(sys.argv[1])
ds = sys.argv[2]
tools = sys.argv[3:]
out = res / "common" / f"{ds}.txt"
out.parent.mkdir(parents=True, exist_ok=True)
ids = None
for t in tools:
    p = res / ds / t / "run1" / t / "results.jsonl"
    if not p.exists():
        raise SystemExit(f"missing {p}")
    rows = [json.loads(l) for l in p.read_text().splitlines()]
    ok = {r["id"] for r in rows if r.get("status") == "ok"}
    ids = ok if ids is None else ids & ok
pdf_ids = [i[:-len("_pg1_repeat1")] + ".pdf" for i in sorted(ids)]
out.write_text("\n".join(pdf_ids) + "\n")
print(f"{len(ids)} common ids -> {out}")
