"""Build benchmarks/results/<date>/report.md from run directories."""
import gzip
import json
import os
import statistics
import sys
from pathlib import Path

ROOT = Path(os.environ.get("YOLOWPDF_ROOT", Path.home() / "yolowpdf"))
DATE = sys.argv[1] if len(sys.argv) > 1 else sorted((ROOT / "benchmarks/results").glob("20*"))[-1].name
RES = ROOT / "benchmarks/results" / DATE
CAPS = json.loads((RES / "caps.json").read_text()) if (RES / "caps.json").exists() else {}


def load_runs(ds, tool):
    out = []
    for d in sorted((RES / ds / tool).glob("run*")):
        f = d / tool / "summary.json"
        if f.exists():
            out.append((d, json.loads(f.read_text())))
    return out


def latency(ds, tool, run):
    p = RES / ds / tool / run / tool / "results.jsonl"
    if not p.exists():
        return None
    rows = [json.loads(l) for l in p.read_text().splitlines()]
    ms = sorted(r["wall_ms"] for r in rows if r.get("status") == "ok" and r.get("wall_ms"))
    if not ms:
        return None
    return ms[int(0.5 * (len(ms) - 1))], ms[int(0.95 * (len(ms) - 1))], len(ms)


def sample_ms(ds, tool, run):
    p = RES / ds / tool / run / tool / "timeline.csv.gz"
    if not p.exists():
        return None
    try:
        with gzip.open(p, "rt") as f:
            ts = [float(l.split(",", 1)[0]) for l in f.read().splitlines()[1:]]
        return statistics.median(b - a for a, b in zip(ts, ts[1:])) if len(ts) > 2 else None
    except (OSError, ValueError):
        return None


def score(ds, tool, kind="scores"):
    p = RES / kind / ds / f"{tool}.json"
    return json.loads(p.read_text()) if p.exists() else None


def main():
    lines = [f"# yolowpdf benchmark, {DATE}", "", "16 workers on every run. Lower ms is better; higher throughput and score are better.", ""]
    if CAPS:
        lines += ["## Time caps", "", "| Dataset | Longest light-tool run | Applied cap | Set by |", "|---|---:|---:|---|"]
        for ds, c in CAPS.items():
            lines.append(f"| {ds} | {c['raw_max_s']:.3f} s | {c['cap_s']:.1f} s | {c['set_by'] or '—'} |")
        lines.append("")
    for ds in sorted({p.name for p in RES.iterdir() if p.is_dir() and p.name not in ("scores", "scores-common")}):
        tools = sorted(p.name for p in (RES / ds).iterdir() if p.is_dir())
        lines += [f"## {ds}", "", "| Tool | Run | Completed | Completion | Wall s | Files/s | p50 ms | p95 ms | Peak MB | Capped | Samples ms | olmOCR ↑ | no-baseline ↑ | Common-items ↑ |", "|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|"]
        for t in tools:
            runs = load_runs(ds, t)
            if not runs:
                continue
            walls = [s["wall_s"] for _, s in runs]
            best = runs[0][1]
            lat = latency(ds, t, runs[0][0].name)
            sc = score(ds, t)
            common = score(ds, t, "scores-common")
            lines.append("| " + " | ".join([
                t, str(len(runs)), f"{best['completed']}/{best['items']}", f"{best['completion']:.3f}",
                f"{statistics.median(walls):.3f}", f"{best['goodput']:.1f}",
                f"{lat[0]:.1f}" if lat else "—", f"{lat[1]:.1f}" if lat else "—",
                f"{best['max_peak_mb']:.0f}", "yes" if best.get("cap_reached") else "no",
                f"{sample_ms(ds, t, runs[0][0].name):.2f}" if sample_ms(ds, t, runs[0][0].name) else "—",
                f"{sc['olmocr']:.3f}" if sc and sc.get("olmocr") is not None else "—",
                f"{sc['nobase']:.3f}" if sc and sc.get("nobase") is not None else "—",
                f"{common['olmocr']:.3f}" if common and common.get("olmocr") is not None else "—",
            ]) + " |")
        lines.append("")
    (RES / "report.md").write_text("\n".join(lines) + "\n")
    print(f"wrote {RES / 'report.md'}")


if __name__ == "__main__":
    main()
