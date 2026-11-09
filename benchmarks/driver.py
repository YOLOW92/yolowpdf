"""16-worker benchmark driver for yolowpdf.

    python3 -I benchmarks/driver.py --tool yolowpdf --manifest list.tsv --out out/ \
        --workers 16 [--cap-s 60] [--timeout 300] [--mem-max 4096] [--sample-us 1000]

- one worker process per slot; each worker handles one item at a time over stdin/stdout JSON
- every worker gets a systemd user scope with MemoryMax, MemorySwapMax=0
- per-item exact peak RSS from cgroup memory.peak, CPU/IO deltas, wall time in ns
- a sampler writes aggregate RSS/CPU/GPU every --sample-us microseconds
- --cap-s is a batch wall-clock limit; unfinished/in-flight items are marked capped
- results.jsonl, summary.json, timeline.csv.gz, gpu.csv.gz
"""
import argparse
import gzip
import json
import os
import queue
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import admit  # noqa: E402

ROOT = Path(os.environ.get("YOLOWPDF_ROOT", Path.home() / "yolowpdf"))
TARGET = Path(os.environ.get("YOLOWPDF_TARGET", ROOT / "target/release"))
LIGHT_PY = Path(os.environ.get("YOLOWPDF_LIGHT_PY", Path.home() / "pagesift-bench-env/tools/py/bin/python"))
BENCH = Path(os.environ.get("YOLOWPDF_BENCH", Path.home() / "yolowpdf-bench"))
DATA = Path(os.environ.get("YOLOWPDF_DATA", Path("/home/yolow92/yolowpdf-data")))


def py(tool, venv=None, env=None, **kw):
    e = {"PYTHONUNBUFFERED": "1"}
    if env:
        e.update(env)
    return dict(cmd=[str((BENCH / "py" / (venv or tool)) / "bin/python"), "-I", str(HERE / "py_worker.py"), tool], env=e, **kw)


def rust_example(name):
    p = TARGET / "examples" / name
    if not p.exists():
        raise SystemExit(f"missing {p}; run `cargo build --release --examples`")
    return [str(p)]


OCR_ENV = {
    "YOLOWPDF_OCR_DET": str(Path.home() / ".cache/pagesift/models/PP-OCRv6_small_det_safetensors"),
    "YOLOWPDF_OCR_REC": str(Path.home() / ".cache/pagesift/models/PP-OCRv6_small_rec_safetensors"),
    "PDF_OCR_FORCE": "1",
}
OCR_MATH_ENV = {
    **OCR_ENV,
    "YOLOWPDF_OCR_LAYOUT": str(Path.home() / ".cache/pagesift/models/PP-DocLayoutV3_safetensors"),
    "YOLOWPDF_OCR_FORMULA": str(Path.home() / ".cache/pagesift/models/PP-FormulaNet_plus-L_safetensors"),
}

# per_worker_mb is an admission estimate, not a hard measurement.
TOOLS = {
    # PDF
    "yolowpdf": dict(cmd=rust_example("bench_worker"), per_worker_mb=200, startup_s=10),
    "pymupdf4llm": py("pymupdf4llm", venv="light", per_worker_mb=1000, startup_s=30),
    "pypdf": py("pypdf", venv="light", per_worker_mb=350, startup_s=30),
    "markitdown": py("markitdown", venv="light", per_worker_mb=1800, startup_s=30),
    "docling": py("docling", venv="docling-gpu", env={"DOCLING_DEVICE": "cpu", "HF_HOME": str(BENCH / "hf")}, per_worker_mb=3200, startup_s=900),
    "docling-gpu": py("docling", venv="docling-gpu", env={"DOCLING_DEVICE": "cuda", "HF_HOME": str(BENCH / "hf")}, per_worker_mb=3200, max_workers=2, startup_s=900),
    "mineru": py("mineru", env={"MINERU_HOME": str(BENCH / "mineru-home")}, per_worker_mb=6000, startup_s=1200),
    "mineru-flash": py("mineru-flash", venv="mineru", env={"MINERU_HOME": str(BENCH / "mineru-home")}, per_worker_mb=2500, startup_s=600),
    # OCR
    "yolowpdf-ocr": dict(cmd=rust_example("bench_ocr_worker"), env=OCR_ENV, per_worker_mb=2800, startup_s=180),
    "yolowpdf-ocr-math": dict(cmd=rust_example("bench_ocr_worker"), env=OCR_MATH_ENV, per_worker_mb=3400, startup_s=300),
    "yolowpdf-ocr-gpu": dict(cmd=rust_example("bench_ocr_worker"), env={**OCR_ENV, "OCR_DEVICE": "gpu"}, per_worker_mb=3000, max_workers=4, startup_s=300),
    "yolowpdf-ocr-math-gpu": dict(cmd=rust_example("bench_ocr_worker"), env={**OCR_MATH_ENV, "OCR_DEVICE": "gpu"}, per_worker_mb=3600, max_workers=2, startup_s=420),
    "docling-ocr": py("docling", venv="docling-gpu", env={"DOCLING_DEVICE": "cpu", "HF_HOME": str(BENCH / "hf")}, per_worker_mb=3200, startup_s=900),
    "docling-ocr-gpu": py("docling", venv="docling-gpu", env={"DOCLING_DEVICE": "cuda", "HF_HOME": str(BENCH / "hf")}, per_worker_mb=3200, max_workers=2, startup_s=900),
    "mineru-ocr": py("mineru", env={"MINERU_HOME": str(BENCH / "mineru-home")}, per_worker_mb=5000, startup_s=1200),
    "tesseract": py("tesseract", venv="light", env={
        "TESSERACT": str(Path.home() / "yolowpdf-bench/tesseract/usr/bin/tesseract"),
        "LD_LIBRARY_PATH": str(Path.home() / "yolowpdf-bench/tesseract/usr/lib/x86_64-linux-gnu"),
        "TESSDATA_PREFIX": str(Path.home() / "yolowpdf-bench/tesseract/usr/share/tesseract-ocr/5/tessdata"),
    }, per_worker_mb=700, startup_s=30),
    "paddleocr": py("paddleocr", env={"PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK": "True"}, per_worker_mb=4000, startup_s=600),
}

CG_ROOT = Path("/sys/fs/cgroup")
LIVE = set()
LIVE_LOCK = threading.Lock()
STOP = threading.Event()
CAP_REACHED = threading.Event()
PEAK_LIVE = [0]
HAVE_SYSTEMD = shutil.which("systemd-run") is not None and os.environ.get("YOLOWPDF_NO_SYSTEMD") != "1"
START = time.perf_counter()
FIRST_JOB = [None]


def mem_available_mb():
    with open("/proc/meminfo") as f:
        for line in f:
            if line.startswith("MemAvailable:"):
                return int(line.split()[1]) // 1024
    return 0


def tree_rss_mb(pid):
    total, todo, seen = 0, [pid], set()
    while todo:
        p = todo.pop()
        if p in seen:
            continue
        seen.add(p)
        try:
            with open(f"/proc/{p}/status") as f:
                for line in f:
                    if line.startswith("VmRSS:"):
                        total += int(line.split()[1])
            for t in os.listdir(f"/proc/{p}/task"):
                with open(f"/proc/{p}/task/{t}/children") as f:
                    todo += [int(c) for c in f.read().split()]
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
    return total / 1024


def read_cg(path, name):
    try:
        return (path / name).read_text()
    except OSError:
        return ""


class Worker:
    seq = 0
    seq_lock = threading.Lock()

    def __init__(self, name, spec, mem_max, log):
        self.name, self.spec, self.mem_max, self.log = name, spec, mem_max, log
        self.proc = None
        self.cg = None
        self.lines = queue.Queue()
        self.oom_seen = 0
        self.peak_fd = None
        self.mem_fd = None
        self.cpu_fd = None
        self.slot = None
        self.index = 0

    def start(self, want=lambda: True):
        if STOP.is_set() or not want():
            return None
        self.slot = admit.acquire(self.name, self.spec["per_worker_mb"], cancel=lambda: STOP.is_set() or not want())
        if self.slot is None:
            return None
        with Worker.seq_lock:
            Worker.seq += 1
            n = Worker.seq
            self.index = n
        env = {**os.environ, **self.spec.get("env", {})}
        cmd = list(self.spec["cmd"])
        if HAVE_SYSTEMD:
            unit = f"yolowpdf-{self.name}-{os.getpid()}-{n}"
            cmd = ["systemd-run", "--user", "--scope", "--quiet", f"--unit={unit}",
                   "-p", f"MemoryMax={self.mem_max}M", "-p", "MemorySwapMax=0", "--"] + cmd
        self.proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
                                     env=env, text=True, encoding="utf-8", start_new_session=True)
        with LIVE_LOCK:
            LIVE.add(self)
            PEAK_LIVE[0] = max(PEAK_LIVE[0], len(LIVE))
        self.lines = queue.Queue()
        threading.Thread(target=self._read, args=(self.proc, self.lines), daemon=True).start()
        t0 = time.perf_counter()
        line = self._get(time.perf_counter() + self.spec.get("startup_s", 60), None)
        if not isinstance(line, dict) or not line.get("ready"):
            self.kill()
            return None
        self.cg = self._cgroup()
        self.slot.set_cgroup(self.cg)
        self.oom_seen = self._oom_count()
        if self.cg:
            try:
                self.peak_fd = os.open(self.cg / "memory.peak", os.O_RDWR)
            except OSError:
                self.peak_fd = None
            try:
                self.mem_fd = os.open(self.cg / "memory.current", os.O_RDONLY)
                self.cpu_fd = os.open(self.cg / "cpu.stat", os.O_RDONLY)
            except OSError:
                pass
        return {"startup_ms": (time.perf_counter() - t0) * 1000, "version": line.get("version")}

    @staticmethod
    def _read(proc, q):
        for line in proc.stdout:
            line = line.strip()
            if line.startswith("{"):
                try:
                    q.put(json.loads(line))
                except json.JSONDecodeError:
                    pass
        q.put(None)

    def _cgroup(self):
        if not HAVE_SYSTEMD or not self.proc:
            return None
        try:
            rel = Path(f"/proc/{self.proc.pid}/cgroup").read_text().strip().split("::", 1)[1]
            p = CG_ROOT / rel.lstrip("/")
            return p if (p / "memory.current").exists() else None
        except (OSError, IndexError):
            return None

    def _oom_count(self):
        if not self.cg:
            return 0
        for line in read_cg(self.cg, "memory.events").splitlines():
            if line.startswith("oom_kill "):
                return int(line.split()[1])
        return 0

    def mem_mb(self):
        if self.mem_fd is not None:
            try:
                return int(os.pread(self.mem_fd, 64, 0)) / 2**20
            except (OSError, ValueError):
                return 0.0
        if self.cg:
            try:
                return int(read_cg(self.cg, "memory.current")) / 2**20
            except ValueError:
                return 0.0
        return tree_rss_mb(self.proc.pid) if self.proc else 0.0

    def cpu_us(self):
        if self.cpu_fd is not None:
            try:
                for line in os.pread(self.cpu_fd, 512, 0).decode(errors="replace").splitlines():
                    if line.startswith("usage_usec "):
                        return int(line.split()[1])
            except (OSError, ValueError):
                return 0
        if self.cg:
            for line in read_cg(self.cg, "cpu.stat").splitlines():
                if line.startswith("usage_usec "):
                    return int(line.split()[1])
        if not self.proc:
            return 0
        try:
            text = Path(f"/proc/{self.proc.pid}/stat").read_text()
            parts = text[text.rfind(")") + 2:].split()
            return (int(parts[11]) + int(parts[12])) * 10_000
        except (OSError, ValueError, IndexError):
            return 0

    def io_bytes(self):
        if self.cg:
            r = w = 0
            for line in read_cg(self.cg, "io.stat").splitlines():
                for f in line.split()[1:]:
                    k, v = f.split("=", 1)
                    if k == "rbytes":
                        r += int(v)
                    elif k == "wbytes":
                        w += int(v)
            return r, w
        if not self.proc:
            return 0, 0
        try:
            d = dict(line.split(":") for line in Path(f"/proc/{self.proc.pid}/io").read_text().splitlines())
            return int(d.get("read_bytes", 0)), int(d.get("write_bytes", 0))
        except (OSError, ValueError):
            return 0, 0

    def reset_peak(self):
        if self.peak_fd is not None:
            try:
                os.write(self.peak_fd, b"reset")
            except OSError:
                os.close(self.peak_fd)
                self.peak_fd = None

    def read_peak(self):
        if self.peak_fd is None:
            return 0.0
        try:
            return int(os.pread(self.peak_fd, 64, 0)) / 2**20
        except (OSError, ValueError):
            return 0.0

    def _get(self, deadline, peak):
        while True:
            left = deadline - time.perf_counter()
            if left <= 0:
                return "timeout"
            try:
                return self.lines.get(timeout=min(0.02, left))
            except queue.Empty:
                if peak is not None:
                    peak[0] = max(peak[0], self.mem_mb())

    def run(self, job, timeout):
        self.reset_peak()
        peak = [self.mem_mb()]
        cpu0, (r0, w0) = self.cpu_us(), self.io_bytes()
        t0 = time.perf_counter()
        try:
            self.proc.stdin.write(json.dumps(job, ensure_ascii=False) + "\n")
            self.proc.stdin.flush()
        except (BrokenPipeError, OSError):
            return {"status": "crash", "error": "worker stdin closed", "peak_mb": peak[0]}
        r = self._get(t0 + timeout, peak)
        wall = (time.perf_counter() - t0) * 1000
        peak[0] = max(peak[0], self.mem_mb(), self.read_peak())
        cpu1, (r1, w1) = self.cpu_us(), self.io_bytes()
        extra = {"worker": self.index, "peak_mb": round(peak[0], 2),
                 "cpu_ms": round((cpu1 - cpu0) / 1000, 3),
                 "read_bytes": r1 - r0, "write_bytes": w1 - w0}
        if r == "timeout":
            self.kill()
            return {"status": "timeout", "wall_ms": wall, "error": f"> {timeout:.1f} s", **extra}
        if r is None:
            oom = self._oom_count() > self.oom_seen
            rc = self.proc.wait()
            self.kill()
            return {"status": "oom" if oom else "crash", "wall_ms": wall, "error": f"worker exited rc={rc}", **extra}
        st = "ok" if r.get("ok") and r.get("bytes", 0) > 0 else ("empty" if r.get("ok") else "error")
        return {"status": st, "ms": r.get("ms"), "wall_ms": wall, "bytes": r.get("bytes", 0),
                "error": r.get("error"), "reply": r, **extra}

    def terminate(self, grace_s=20):
        if self.cg:
            try:
                for pid in read_cg(self.cg, "cgroup.procs").split():
                    os.kill(int(pid), signal.SIGTERM)
            except (OSError, ValueError):
                pass
        elif self.proc:
            try:
                os.killpg(self.proc.pid, signal.SIGTERM)
            except (ProcessLookupError, PermissionError):
                pass
        if self.proc:
            try:
                self.proc.wait(timeout=grace_s)
            except subprocess.TimeoutExpired:
                pass
        self.kill()

    def kill(self):
        if self.slot is not None:
            self.slot.release()
            self.slot = None
        with LIVE_LOCK:
            LIVE.discard(self)
        for fd in (self.peak_fd, self.mem_fd, self.cpu_fd):
            if fd is not None:
                try: os.close(fd)
                except OSError: pass
        self.peak_fd = self.mem_fd = self.cpu_fd = None
        if not self.proc:
            return
        if self.cg:
            try:
                (self.cg / "cgroup.kill").write_text("1")
            except OSError:
                pass
        try:
            os.killpg(self.proc.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
        self.proc = None
        self.cg = None

    def close(self):
        if self.proc:
            try:
                self.proc.stdin.close()
                self.proc.wait(timeout=30)
            except (OSError, subprocess.TimeoutExpired):
                pass
            self.kill()


def gpu_sample():
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=utilization.gpu,memory.used,clocks.sm,temperature.gpu",
                              "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=2).stdout.strip()
        return out.splitlines()[0] if out else ""
    except Exception:
        return ""


def sampler(path, interval_us, stop):
    t0 = time.perf_counter()
    last_cpu = {}
    last_t = t0
    with gzip.open(path, "wt", encoding="utf-8") as f:
        f.write("t_ms,rss_mb,cpu_pct,live_workers\n")
        while not stop.is_set():
            now = time.perf_counter()
            with LIVE_LOCK:
                ws = list(LIVE)
            rss = sum(w.mem_mb() for w in ws)
            cpu_us = sum(w.cpu_us() for w in ws)
            dt = max(now - last_t, 1e-9)
            cpu_pct = (cpu_us - last_cpu.get("u", cpu_us)) / 1e6 / dt * 100
            last_cpu["u"] = cpu_us
            last_t = now
            f.write(f"{(now - t0) * 1000:.3f},{rss:.2f},{cpu_pct:.1f},{len(ws)}\n")
            time.sleep(max(interval_us, 200) / 1e6)


def gpu_sampler(path, stop):
    t0 = time.perf_counter()
    with gzip.open(path, "wt", encoding="utf-8") as gf:
        gf.write("t_ms,gpu_util,gpu_mem_mb,gpu_clock_mhz,gpu_temp_c\n")
        while not stop.is_set():
            s = gpu_sample()
            if s:
                gf.write(f"{(time.perf_counter() - t0) * 1000:.3f},{s}\n")
            time.sleep(0.05)


MEM_LOW = [False]


def mem_watchdog(stop):
    while not stop.wait(1.0):
        if mem_available_mb() < 1200:
            MEM_LOW[0] = True
            STOP.set()
            return


def on_sigterm(signum, frame):
    if STOP.is_set():
        return
    STOP.set()
    with LIVE_LOCK:
        ws = list(LIVE)
    for w in ws:
        threading.Thread(target=w.terminate, daemon=True).start()


def read_manifest(path):
    items = []
    for i, line in enumerate(Path(path).read_text().splitlines()):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) >= 2:
            id_, inp = parts[0], parts[1]
        else:
            inp = line
            id_ = f"{i:05d}-{Path(inp).stem}"[:120]
        items.append((id_, inp))
    return items


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tool", required=True, choices=sorted(TOOLS))
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--workers", type=int, default=16)
    ap.add_argument("--timeout", type=float, default=3600.0, help="per-item timeout, seconds")
    ap.add_argument("--mem-max", type=int, default=4096)
    ap.add_argument("--cap-s", type=float, default=0.0, help="batch wall-clock cap, 0 = no cap")
    ap.add_argument("--sample-us", type=int, default=1000)
    ap.add_argument("--reserve", type=int, default=None)
    a = ap.parse_args()

    spec = dict(TOOLS[a.tool])
    if os.environ.get("YOLOWPDF_PER_WORKER_MB"):
        spec["per_worker_mb"] = json.loads(Path(os.environ["YOLOWPDF_PER_WORKER_MB"]).read_text()).get(a.tool, spec["per_worker_mb"])
    items = read_manifest(a.manifest)
    out = Path(a.out) / a.tool
    out.mkdir(parents=True, exist_ok=True)
    admit.RESERVE_MB = a.reserve if a.reserve is not None else max(admit.RESERVE_MB, 3072)
    admit.SLOTS = min(16, os.cpu_count() or 16)
    signal.signal(signal.SIGTERM, on_sigterm)

    n_workers = max(1, min(a.workers, len(items) or 1, spec.get("max_workers", 1 << 30)))
    jobs = queue.Queue()
    for it in items:
        jobs.put(it)
    res_lock = threading.Lock()
    res_f = open(out / "results.jsonl", "w", encoding="utf-8")
    log = open(out / "worker.log", "w")
    starts, versions, peaks = [], set(), []
    cap_deadline = [None]
    sampler_thread = [None]
    gpu_thread = [None]

    def loop():
        w = Worker(a.tool, spec, a.mem_max, log)
        alive, fails = False, 0
        while not STOP.is_set():
            if jobs.empty():
                break
            if cap_deadline[0] is not None and time.perf_counter() >= cap_deadline[0]:
                CAP_REACHED.set()
                STOP.set()
                break
            if not alive:
                info = w.start(want=lambda: not jobs.empty())
                if info is None:
                    if STOP.is_set() or jobs.empty():
                        break
                    fails += 1
                    try:
                        id_, inp = jobs.get_nowait()
                    except queue.Empty:
                        break
                    with res_lock:
                        res_f.write(json.dumps({"tool": a.tool, "id": id_, "input": inp,
                                                "status": "crash", "error": "worker failed to start"}, ensure_ascii=False) + "\n")
                    if fails >= 3:
                        break
                    continue
                alive, fails = True, 0
                with res_lock:
                    starts.append(info["startup_ms"])
                    versions.add(info["version"])
            if cap_deadline[0] is not None and time.perf_counter() >= cap_deadline[0]:
                CAP_REACHED.set()
                STOP.set()
                break
            try:
                id_, inp = jobs.get_nowait()
            except queue.Empty:
                break
            if FIRST_JOB[0] is None:
                FIRST_JOB[0] = time.perf_counter()
                if a.cap_s > 0:
                    cap_deadline[0] = FIRST_JOB[0] + a.cap_s
                sampler_thread[0] = threading.Thread(target=sampler, args=(out / "timeline.csv.gz", a.sample_us, STOP), daemon=True)
                sampler_thread[0].start()
                gpu_thread[0] = threading.Thread(target=gpu_sampler, args=(out / "gpu.csv.gz", STOP), daemon=True)
                gpu_thread[0].start()
                threading.Thread(target=mem_watchdog, args=(STOP,), daemon=True).start()
            left = max(0.1, (cap_deadline[0] - time.perf_counter()) if a.cap_s > 0 else a.timeout)
            job = {"id": id_, "input": str(Path(inp).resolve()), "out": str((out / f"{id_}.md").resolve())}
            rec, = (w.run(job, min(a.timeout, left)),)
            if cap_deadline[0] is not None and time.perf_counter() >= cap_deadline[0]:
                CAP_REACHED.set()
                STOP.set()
            if CAP_REACHED.is_set() and rec["status"] in ("timeout", "crash", "error"):
                rec["status"] = "capped"
            alive = w.proc is not None
            with res_lock:
                peaks.append(rec.get("peak_mb") or 0)
                res_f.write(json.dumps({"tool": a.tool, "id": id_, "input": inp, **rec}, ensure_ascii=False) + "\n")
                res_f.flush()
        w.close()

    t0 = time.perf_counter()
    threads = [threading.Thread(target=loop) for _ in range(n_workers)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    wall = time.perf_counter() - (FIRST_JOB[0] or t0)
    STOP.set()
    for th in (sampler_thread[0], gpu_thread[0]):
        if th is not None:
            th.join(timeout=5)
    res_f.close()
    log.close()

    rows = [json.loads(l) for l in (out / "results.jsonl").read_text().splitlines()]
    done = {r["id"] for r in rows}
    with open(out / "results.jsonl", "a", encoding="utf-8") as f:
        for id_, inp in items:
            if id_ not in done:
                status = "capped" if CAP_REACHED.is_set() else ("stopped" if STOP.is_set() else "crash")
                r = {"tool": a.tool, "id": id_, "input": inp, "status": status,
                     "error": "not run: batch cap reached" if status == "capped" else "not run"}
                rows.append(r)
                f.write(json.dumps(r, ensure_ascii=False) + "\n")
    counts = {}
    for r in rows:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    ok = counts.get("ok", 0)
    summary = {
        "tool": a.tool, "versions": sorted(v for v in versions if v), "items": len(items),
        "workers_asked": n_workers, "peak_live_workers": PEAK_LIVE[0], "wall_s": round(wall, 3),
        "completed": ok, "completion": round(ok / max(len(items), 1), 6),
        "goodput": round(ok / max(wall, 1e-9), 3), "status": counts,
        "startup_ms": [round(x, 1) for x in starts],
        "max_peak_mb": round(max(peaks, default=0), 2),
        "cap_s": a.cap_s, "cap_reached": CAP_REACHED.is_set(), "timeout_s": a.timeout,
        "mem_max_mb": a.mem_max, "sample_us": a.sample_us, "cgroup": HAVE_SYSTEMD,
        "mem_low": MEM_LOW[0],
        "per_worker_mb": spec["per_worker_mb"],
    }
    (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=1))
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
