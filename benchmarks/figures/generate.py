#!/usr/bin/env python3
"""Generate SVG benchmark charts and architecture diagrams for yolowpdf 0.1.2."""

from __future__ import annotations

from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "figures"

FONT_SANS = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"
FONT_MONO = "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace"

BG = "#f8fafc"
CARD = "#ffffff"
CARD_ALT = "#f1f5f9"
BORDER = "#e2e8f0"
GRID = "#f1f5f9"
TEXT = "#0f172a"
SUB = "#475569"
MUTED = "#64748b"

NAVY = "#0f172a"
NAVY_2 = "#1e293b"
INDIGO = "#4f46e5"
INDIGO_SOFT = "#eef2ff"
INDIGO_BORDER = "#c7d2fe"
TEAL = "#0d9488"
TEAL_SOFT = "#f0fdfa"
TEAL_BORDER = "#99f6e4"
BLUE = "#0284c7"
BLUE_SOFT = "#f0f9ff"
BLUE_BORDER = "#bae6fd"
AMBER = "#d97706"
AMBER_SOFT = "#fffbeb"
AMBER_BORDER = "#fde68a"
EMERALD = "#059669"
EMERALD_SOFT = "#ecfdf5"
EMERALD_BORDER = "#a7f3d0"
ROSE = "#e11d48"
ROSE_SOFT = "#fff1f2"
SLATE_BAR = "#94a3b8"


def svg_doc(width: int, height: int, body: list[str]) -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
        f'width="100%" role="img">\n'
        "  <defs>\n"
        '    <linearGradient id="headerGrad" x1="0" y1="0" x2="1" y2="0">\n'
        f'      <stop offset="0%" stop-color="{NAVY}"/>\n'
        f'      <stop offset="60%" stop-color="{NAVY_2}"/>\n'
        '      <stop offset="100%" stop-color="#312e81"/>\n'
        "    </linearGradient>\n"
        '    <linearGradient id="primaryGrad" x1="0" y1="0" x2="1" y2="0">\n'
        '      <stop offset="0%" stop-color="#4338ca"/>\n'
        '      <stop offset="100%" stop-color="#6366f1"/>\n'
        "    </linearGradient>\n"
        '    <linearGradient id="tealGrad" x1="0" y1="0" x2="1" y2="0">\n'
        '      <stop offset="0%" stop-color="#0f766e"/>\n'
        '      <stop offset="100%" stop-color="#14b8a6"/>\n'
        "    </linearGradient>\n"
        '    <linearGradient id="blueGrad" x1="0" y1="0" x2="1" y2="0">\n'
        '      <stop offset="0%" stop-color="#0369a1"/>\n'
        '      <stop offset="100%" stop-color="#0ea5e9"/>\n'
        "    </linearGradient>\n"
        '    <filter id="cardShadow" x="-4%" y="-4%" width="108%" height="112%">\n'
        '      <feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="#0f172a" flood-opacity="0.05"/>\n'
        "    </filter>\n"
        '    <marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">\n'
        f'      <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="{INDIGO}"/>\n'
        "    </marker>\n"
        "  </defs>\n"
        "  <style>\n"
        f"    .title {{ font: 700 18.5px {FONT_SANS}; fill: #ffffff; letter-spacing: -0.2px; }}\n"
        f"    .subtitle {{ font: 400 12px {FONT_SANS}; fill: #cbd5e1; }}\n"
        f"    .panel-title {{ font: 700 14.5px {FONT_SANS}; fill: {TEXT}; }}\n"
        f"    .panel-sub {{ font: 400 11.5px {FONT_SANS}; fill: {MUTED}; }}\n"
        f"    .label {{ font: 500 12px {FONT_SANS}; fill: {SUB}; }}\n"
        f"    .label-hi {{ font: 700 12.5px {FONT_SANS}; fill: {TEXT}; }}\n"
        f"    .val {{ font: 500 11.5px {FONT_MONO}; fill: {SUB}; }}\n"
        f"    .val-hi {{ font: 700 12px {FONT_MONO}; fill: {INDIGO}; }}\n"
        f"    .val-teal {{ font: 700 12px {FONT_MONO}; fill: {TEAL}; }}\n"
        f"    .axis {{ font: 500 10.5px {FONT_MONO}; fill: {MUTED}; }}\n"
        f"    .kpi-num {{ font: 800 22px {FONT_MONO}; letter-spacing: -0.5px; }}\n"
        f"    .kpi-title {{ font: 700 11px {FONT_SANS}; fill: {TEXT}; }}\n"
        f"    .kpi-sub {{ font: 400 10.5px {FONT_SANS}; fill: {MUTED}; }}\n"
        f"    .badge {{ font: 700 10.5px {FONT_MONO}; }}\n"
        f"    .code {{ font: 600 11.5px {FONT_MONO}; fill: {TEXT}; }}\n"
        f"    .note {{ font: 400 11px {FONT_SANS}; fill: {SUB}; }}\n"
        "  </style>\n"
        f'  <rect width="{width}" height="{height}" rx="14" fill="{BG}" stroke="{BORDER}"/>\n'
        + "\n".join(body)
        + "\n</svg>\n"
    )


def header_band(width: int, title: str, subtitle: str, badge_text: str) -> list[str]:
    bw = max(len(badge_text) * 7 + 26, 130)
    bx = width - bw - 22
    return [
        f'  <rect x="0" y="0" width="{width}" height="68" rx="14" fill="url(#headerGrad)"/>',
        f'  <rect x="0" y="54" width="{width}" height="14" fill="url(#headerGrad)"/>',
        f'  <text x="26" y="30" class="title">{escape(title)}</text>',
        f'  <text x="26" y="50" class="subtitle">{escape(subtitle)}</text>',
        f'  <rect x="{bx}" y="19" width="{bw}" height="28" rx="14" fill="#1e1b4b" stroke="#6366f1" stroke-width="1"/>',
        f'  <text x="{bx + bw / 2:.1f}" y="37" text-anchor="middle" class="badge" fill="#e0e7ff">{escape(badge_text)}</text>',
    ]


def card_box(x: float, y: float, w: float, h: float, title: str = "", sub: str = "") -> list[str]:
    out = [
        f'  <rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="{CARD}" stroke="{BORDER}" filter="url(#cardShadow)"/>',
    ]
    if title:
        out.append(f'  <text x="{x + 18}" y="{y + 25}" class="panel-title">{escape(title)}</text>')
    if sub:
        out.append(f'  <text x="{x + 18}" y="{y + 43}" class="panel-sub">{escape(sub)}</text>')
    return out


def make_hero_pdf_bench() -> None:
    width, height = 1040, 440
    body = header_band(
        width,
        "yolowpdf — Born-Digital PDF to Markdown Benchmark (olmOCR-bench, 1,269 Pages)",
        "i7-10700 · 15 GB RAM · Pure Rust zero-native-dependency parser · Exploratory run",
        "1,900 pages/s · 24 MB",
    )

    kpis = [
        ("1,900 pages/s", "Throughput (0.668 s wall)", "23.9x pypdf · 290x pymupdf4llm", INDIGO, INDIGO_SOFT, INDIGO_BORDER),
        ("3.2 ms / 9.0 ms", "p50 / p95 Page Latency", "vs 73.9 ms pypdf · 845.6 ms pymupdf4llm", TEAL, TEAL_SOFT, TEAL_BORDER),
        ("24 MB Peak", "Per-Worker cgroup Memory", "25x less than pymupdf4llm (600 MB)", EMERALD, EMERALD_SOFT, EMERALD_BORDER),
        ("0.592 Score", "1,269 / 1,269 Completed", "#1 overall completion + quality score", BLUE, BLUE_SOFT, BLUE_BORDER),
    ]
    kw = (width - 40 - 36) / 4
    for i, (num, t, s, col, bg, bd) in enumerate(kpis):
        kx = 20 + i * (kw + 12)
        ky = 82
        body.extend([
            f'  <rect x="{kx:.1f}" y="{ky}" width="{kw:.1f}" height="76" rx="10" fill="{CARD}" stroke="{bd}" filter="url(#cardShadow)"/>',
            f'  <rect x="{kx:.1f}" y="{ky}" width="6" height="76" rx="3" fill="{col}"/>',
            f'  <text x="{kx + 18:.1f}" y="{ky + 29}" class="kpi-num" fill="{col}">{escape(num)}</text>',
            f'  <text x="{kx + 18:.1f}" y="{ky + 48}" class="kpi-title">{escape(t)}</text>',
            f'  <text x="{kx + 18:.1f}" y="{ky + 64}" class="kpi-sub">{escape(s)}</text>',
        ])

    # Left panel: Throughput & Latency
    pw = (width - 54) / 2
    py, ph = 172, 248
    body.extend(card_box(20, py, pw, ph, "Throughput (pages/s ↑) & Wall Time", "1,269 born-digital pages · Higher throughput is better"))

    rows_left = [
        ("yolowpdf", 1900.0, "1,900 pages/s  (0.668 s · p50 3.2 ms)", True),
        ("pypdf", 79.4, "79.4 pages/s  (15.3 s · p50 73.9 ms)", False),
        ("MarkItDown", 13.6, "13.6 pages/s  (88.5 s · p50 172.8 ms)", False),
        ("pymupdf4llm", 6.56, "6.56 pages/s  (184.9 s · p50 845.6 ms)", False),
        ("Docling GPU", 3.18, "3.18 pages/s  (185.2 s · 589/1269)", False),
        ("Docling CPU", 0.48, "0.48 pages/s  (185.8 s · 89/1269)", False),
    ]
    bx = 124
    bw = pw - 124 - 250
    for idx, (name, val, label, hi) in enumerate(rows_left):
        ry = py + 62 + idx * 29
        if hi:
            body.append(f'  <rect x="28" y="{ry - 4}" width="{pw - 16:.1f}" height="24" rx="5" fill="{INDIGO_SOFT}"/>')
        body.append(
            f'  <text x="{bx - 10}" y="{ry + 12}" text-anchor="end" class="{"label-hi" if hi else "label"}">{escape(name)}</text>'
        )
        # Use square-root visual scale so 6.56..79.4 are still clearly visible next to 1,900
        w = max((val / 1900.0) ** 0.45 * bw, 5.0)
        fill = "url(#primaryGrad)" if hi else SLATE_BAR
        body.append(f'  <rect x="{bx}" y="{ry}" width="{w:.1f}" height="16" rx="4" fill="{fill}"/>')
        body.append(
            f'  <text x="{bx + w + 8:.1f}" y="{ry + 12}" class="{"val-hi" if hi else "val"}">{escape(label)}</text>'
        )

    # Right panel: Quality Score & Peak Memory
    rx = 20 + pw + 14
    body.extend(card_box(rx, py, pw, ph, "Combined Score (↑) & Peak Worker Memory (MB ↓)", "Higher score & lower per-worker cgroup peak memory are better"))

    rows_right = [
        ("yolowpdf", 0.592, "0.592", "24 MB", "1269/1269", True),
        ("pymupdf4llm", 0.489, "0.489", "600 MB", "1213/1269", False),
        ("pypdf", 0.385, "0.385", "186 MB", "1213/1269", False),
        ("MarkItDown", 0.373, "0.373", "1,171 MB", "1204/1269", False),
        ("Docling GPU*", 0.116, "0.116", "2,323 MB", "589/1269", False),
        ("Docling CPU*", 0.012, "0.012", "2,745 MB", "89/1269", False),
    ]
    bx2 = rx + 118
    bw2 = pw - 118 - 195
    for idx, (name, score, s_txt, mem_txt, comp_txt, hi) in enumerate(rows_right):
        ry = py + 62 + idx * 29
        if hi:
            body.append(f'  <rect x="{rx + 8:.1f}" y="{ry - 4}" width="{pw - 16:.1f}" height="24" rx="5" fill="{TEAL_SOFT}"/>')
        body.append(
            f'  <text x="{bx2 - 10:.1f}" y="{ry + 12}" text-anchor="end" class="{"label-hi" if hi else "label"}">{escape(name)}</text>'
        )
        w = max((score / 0.65) * bw2, 4.0)
        fill = "url(#tealGrad)" if hi else SLATE_BAR
        body.append(f'  <rect x="{bx2:.1f}" y="{ry}" width="{w:.1f}" height="16" rx="4" fill="{fill}"/>')
        info = f"{s_txt}  |  {mem_txt}  ({comp_txt})"
        body.append(
            f'  <text x="{bx2 + w + 8:.1f}" y="{ry + 12}" class="{"val-teal" if hi else "val"}">{escape(info)}</text>'
        )

    (OUT / "hero-pdf-bench.svg").write_text(svg_doc(width, height, body), encoding="utf-8")


def make_ocr_bench() -> None:
    width, height = 1040, 360
    body = header_band(
        width,
        "yolowpdf-ocr — Scanned PDF & Math OCR Benchmark (134 Pages)",
        "olmOCR-bench old_scans + old_scans_math · PP-OCRv6 + Optional PP-DocLayoutV3 & PP-FormulaNet · CPU or NVIDIA CUDA",
        "134/134 Completed",
    )

    pw = (width - 54) / 2
    py, ph = 84, 256
    body.extend(card_box(20, py, pw, ph, "OCR Score ↑ (All-Items & No-Baseline)", "134 scanned pages · Unfinished capped pages count as 0"))

    ocr_scores = [
        ("yolowpdf-ocr (GPU + formula)", 0.5428, "0.5428  (no-base 0.3143 · 134/134)", "primary"),
        ("yolowpdf-ocr (GPU text-only)", 0.4075, "0.4075  (no-base 0.1112 · 134/134)", "teal"),
        ("yolowpdf-ocr (CPU text-only)", 0.4075, "0.4075  (no-base 0.1112 · 134/134)", "teal"),
        ("Docling OCR GPU (default)", 0.2251, "0.2251  (no-base 0.0989 · 64/134)", "other"),
        ("Docling OCR CPU (default)", 0.0519, "0.0519  (no-base 0.0257 · 14/134)", "other"),
    ]
    bx = 196
    bw = pw - 196 - 235
    for idx, (name, sc, label, kind) in enumerate(ocr_scores):
        ry = py + 66 + idx * 35
        hi = kind != "other"
        if kind == "primary":
            body.append(f'  <rect x="28" y="{ry - 5}" width="{pw - 16:.1f}" height="26" rx="5" fill="{INDIGO_SOFT}"/>')
        body.append(
            f'  <text x="{bx - 10}" y="{ry + 12}" text-anchor="end" class="{"label-hi" if hi else "label"}">{escape(name)}</text>'
        )
        w = max((sc / 0.60) * bw, 5.0)
        fill = "url(#primaryGrad)" if kind == "primary" else ("url(#tealGrad)" if kind == "teal" else SLATE_BAR)
        cls = "val-hi" if kind == "primary" else ("val-teal" if kind == "teal" else "val")
        body.append(f'  <rect x="{bx}" y="{ry}" width="{w:.1f}" height="16" rx="4" fill="{fill}"/>')
        body.append(f'  <text x="{bx + w + 8:.1f}" y="{ry + 12}" class="{cls}">{escape(label)}</text>')

    rx = 20 + pw + 14
    body.extend(card_box(rx, py, pw, ph, "Throughput (pages/s ↑) & Peak Worker Memory (MB ↓)", "Pure-Rust CPU/CUDA inference · GPU batch scheduler"))

    ocr_perf = [
        ("yolowpdf-ocr (GPU text-only)", 2.245, "2.245 pages/s  |  673 MB  (59.7 s)", "teal"),
        ("yolowpdf-ocr (CPU text-only)", 0.865, "0.865 pages/s  |  616 MB  (154.8 s)", "teal"),
        ("Docling OCR GPU (default)", 0.410, "0.410 pages/s  |  4,083 MB  (155.2 s)", "other"),
        ("yolowpdf-ocr (GPU + formula)", 0.152, "0.152 pages/s  |  2,225 MB  (882.4 s)", "primary"),
        ("Docling OCR CPU (default)", 0.090, "0.090 pages/s  |  2,543 MB  (155.7 s)", "other"),
    ]
    bx2 = rx + 186
    bw2 = pw - 186 - 230
    for idx, (name, pps, label, kind) in enumerate(ocr_perf):
        ry = py + 66 + idx * 35
        hi = kind != "other"
        if kind == "teal" and idx == 0:
            body.append(f'  <rect x="{rx + 8:.1f}" y="{ry - 5}" width="{pw - 16:.1f}" height="26" rx="5" fill="{TEAL_SOFT}"/>')
        body.append(
            f'  <text x="{bx2 - 10:.1f}" y="{ry + 12}" text-anchor="end" class="{"label-hi" if hi else "label"}">{escape(name)}</text>'
        )
        w = max((pps / 2.4) * bw2, 5.0)
        fill = "url(#primaryGrad)" if kind == "primary" else ("url(#tealGrad)" if kind == "teal" else SLATE_BAR)
        cls = "val-hi" if kind == "primary" else ("val-teal" if kind == "teal" else "val")
        body.append(f'  <rect x="{bx2:.1f}" y="{ry}" width="{w:.1f}" height="16" rx="4" fill="{fill}"/>')
        body.append(f'  <text x="{bx2 + w + 8:.1f}" y="{ry + 12}" class="{cls}">{escape(label)}</text>')

    (OUT / "ocr-bench.svg").write_text(svg_doc(width, height, body), encoding="utf-8")


def make_hard_pdfs() -> None:
    width, height = 1040, 290
    body = header_band(
        width,
        "Stress & Malformed PDF Corpus — pdf.js Test Suite (1,089 Files)",
        "Broken xref tables, truncated streams, encrypted objects, exotic CJK CMaps & pathological layouts",
        "1,663 files/s · 0 Panics",
    )

    pw = (width - 54) / 2
    py, ph = 84, 188
    body.extend(card_box(20, py, pw, ph, "Throughput (files/s ↑) & Wall Time", "1,089 difficult/corrupt PDFs from the pdf.js test suite"))

    rows = [
        ("yolowpdf", 1663.0, "1,663 files/s  (0.496 s wall · 830/1089)", True),
        ("pymupdf4llm*", 1.31, "1.31 files/s  (640.9 s wall · 841/1089)", False),
    ]
    bx = 116
    bw = pw - 116 - 278
    for idx, (name, val, label, hi) in enumerate(rows):
        ry = py + 66 + idx * 42
        if hi:
            body.append(f'  <rect x="28" y="{ry - 5}" width="{pw - 16:.1f}" height="28" rx="5" fill="{INDIGO_SOFT}"/>')
        body.append(f'  <text x="{bx - 10}" y="{ry + 13}" text-anchor="end" class="{"label-hi" if hi else "label"}">{escape(name)}</text>')
        w = bw if hi else 8.0
        fill = "url(#primaryGrad)" if hi else SLATE_BAR
        body.append(f'  <rect x="{bx}" y="{ry}" width="{w:.1f}" height="18" rx="4" fill="{fill}"/>')
        body.append(f'  <text x="{bx + w + 10:.1f}" y="{ry + 13}" class="{"val-hi" if hi else "val"}">{escape(label)}</text>')
    body.append(f'  <text x="38" y="{py + 165}" class="panel-sub">*pymupdf4llm ran on an NTFS mount (different I/O; not a controlled speed test).</text>')

    rx = 20 + pw + 14
    body.extend(card_box(rx, py, pw, ph, "Peak Memory (MB ↓) & Resource Safety Guards", "Bounded stream allocations (0.1.1+) & catch_unwind boundary"))

    mem_rows = [
        ("yolowpdf", 104.0, "104 MB peak  (39.4x less memory)", True),
        ("pymupdf4llm", 4094.0, "4,094 MB peak", False),
    ]
    bx2 = rx + 115
    bw2 = pw - 115 - 210
    for idx, (name, val, label, hi) in enumerate(mem_rows):
        ry = py + 66 + idx * 42
        if hi:
            body.append(f'  <rect x="{rx + 8:.1f}" y="{ry - 5}" width="{pw - 16:.1f}" height="28" rx="5" fill="{EMERALD_SOFT}"/>')
        body.append(f'  <text x="{bx2 - 10:.1f}" y="{ry + 13}" text-anchor="end" class="{"label-hi" if hi else "label"}">{escape(name)}</text>')
        w = max((val / 4094.0) * bw2, 8.0)
        fill = "url(#tealGrad)" if hi else SLATE_BAR
        body.append(f'  <rect x="{bx2:.1f}" y="{ry}" width="{w:.1f}" height="18" rx="4" fill="{fill}"/>')
        body.append(f'  <text x="{bx2 + w + 10:.1f}" y="{ry + 13}" class="{"val-teal" if hi else "val"}">{escape(label)}</text>')
    body.append(f'  <text x="{rx + 18:.1f}" y="{py + 165}" class="panel-sub">Pre-allocation budget checks on PNG/TIFF predictors &amp; stream filters.</text>')

    (OUT / "hard-pdfs.svg").write_text(svg_doc(width, height, body), encoding="utf-8")


def make_architecture() -> None:
    width, height = 1040, 370
    body = header_band(
        width,
        "yolowpdf Architecture — Zero-Native-Dependency PDF Reader & Optional OCR",
        "From raw PDF bytes to clean Markdown, LaTeX math, tables, and yolow64 Base64 Data URLs",
        "Rust 2024 · MIT",
    )

    stages = [
        (
            "1. File Structure",
            "xref & Stream Layer",
            [
                "xref tables & object streams",
                "Damaged-file full-scan repair",
                "AES / RC4 stream decryption",
                "Flate, Brotli, LZW, ASCII85/Hex",
                "Pre-allocation budget guards",
            ],
            INDIGO,
            INDIGO_SOFT,
            INDIGO_BORDER,
        ),
        (
            "2. Fonts & CJK",
            "Glyph & Encoding Layer",
            [
                "Type1, TrueType, CFF, Type3",
                "CIDFonts & ToUnicode CMaps",
                "Adobe-GB1 / CNS1 / J1 / KR1",
                "CJK collection fallback",
                "Text matrices & glyph widths",
            ],
            BLUE,
            BLUE_SOFT,
            BLUE_BORDER,
        ),
        (
            "3. Layout Engine",
            "Reading Order & Quality",
            [
                "Multi-column block ordering",
                "Heading hierarchy inference",
                "Header & footer removal",
                "Cross-page text & hyphens",
                "Wilson CI PageQuality score",
            ],
            TEAL,
            TEAL_SOFT,
            TEAL_BORDER,
        ),
        (
            "4. Tables, Math & Img",
            "Structured Reconstruction",
            [
                "Ruled & borderless tables",
                "Inline LaTeX math baselines",
                "Raw JPEG / JP2 / JBIG2 / CCITT",
                "yolow64 SIMD Base64 Data URL",
                "Scanned-page ink detection",
            ],
            EMERALD,
            EMERALD_SOFT,
            EMERALD_BORDER,
        ),
        (
            "5. Optional PageOcr",
            "yolowpdf-ocr (CPU / CUDA)",
            [
                "PP-OCRv6 det + rec models",
                "Optional PP-DocLayoutV3",
                "Optional PP-FormulaNet_plus-L",
                "CUDA PTX GPU batch scheduler",
                "Auto CPU fallback + OcrReport",
            ],
            AMBER,
            AMBER_SOFT,
            AMBER_BORDER,
        ),
    ]

    sw = (width - 40 - 4 * 16) / 5
    sy, sh = 86, 196
    for i, (title, sub, bullets, col, bg, bd) in enumerate(stages):
        sx = 20 + i * (sw + 16)
        body.extend([
            f'  <rect x="{sx:.1f}" y="{sy}" width="{sw:.1f}" height="{sh}" rx="10" fill="{CARD}" stroke="{bd}" stroke-width="1.5" filter="url(#cardShadow)"/>',
            f'  <rect x="{sx:.1f}" y="{sy}" width="{sw:.1f}" height="48" rx="10" fill="{bg}"/>',
            f'  <rect x="{sx:.1f}" y="{sy + 38}" width="{sw:.1f}" height="10" fill="{bg}"/>',
            f'  <text x="{sx + 12:.1f}" y="{sy + 21}" class="label-hi" fill="{col}">{escape(title)}</text>',
            f'  <text x="{sx + 12:.1f}" y="{sy + 38}" class="kpi-sub">{escape(sub)}</text>',
        ])
        for j, b in enumerate(bullets):
            by = sy + 68 + j * 24
            body.append(f'  <circle cx="{sx + 15:.1f}" cy="{by - 4}" r="2.5" fill="{col}"/>')
            body.append(f'  <text x="{sx + 23:.1f}" y="{by}" class="note">{escape(b)}</text>')
        if i < 4:
            ax1 = sx + sw + 2
            ax2 = sx + sw + 14
            ay = sy + sh / 2
            body.append(f'  <line x1="{ax1:.1f}" y1="{ay:.1f}" x2="{ax2:.1f}" y2="{ay:.1f}" stroke="{INDIGO}" stroke-width="1.8" marker-end="url(#arrow)"/>')

    # Bottom summary banner
    by = 298
    body.extend([
        f'  <rect x="20" y="{by}" width="{width - 40}" height="54" rx="10" fill="{CARD}" stroke="{BORDER}" filter="url(#cardShadow)"/>',
        f'  <text x="38" y="{by + 23}" class="label-hi">Unified Output: PdfDocument</text>',
        f'  <text x="235" y="{by + 23}" class="code">markdown · pages · metadata · inferred_title · images (file_name / data_url) · warnings · ocr</text>',
        f'  <text x="38" y="{by + 42}" class="panel-sub">Adaptive OLS + EVT parse timeouts (yolowpdf-stat) · Process allocator tuning (yolowpdf-sys) · Base64 Data URL encoding powered by yolow64 0.3</text>',
    ])

    (OUT / "architecture.svg").write_text(svg_doc(width, height, body), encoding="utf-8")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    make_hero_pdf_bench()
    make_ocr_bench()
    make_hard_pdfs()
    make_architecture()
    print("Generated 4 SVG figures in", OUT)


if __name__ == "__main__":
    main()
