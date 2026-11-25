//! 2D 矩阵与分段函数重构引擎（`\begin{pmatrix}`, `\begin{bmatrix}`, `\begin{vmatrix}`,
//! `\begin{Bmatrix}`, `\begin{cases}`, `\binom`）。
//!
//! 参考 Zanibbi, Blostein & Cordy (DRACULAE, IEEE TPAMI 2002) 的二维定界符框分解算法：
//! 1. 识别垂直堆叠的 CMEX/LMEX/Symbol-PUA 定界符组件（如 `⎛⎜⎝`、`⎡⎢⎣`、`⎧⎨⎩`、``、``、``）
//!    以及单个大号 `Extension` 定界符（`(`、`[`、`{`、`|`）；
//! 2. 配对左右等高定界符重构矩阵，或识别左大括号 `{` 右侧多行条件式重构 `\begin{cases}`；
//! 3. 在定界符内部按 Y 投影分行、按 X 投影谷分列，用 `render_subexpr` 渲染每个格（天然支持格内 `\frac`/`\sqrt`），
//!    将整块折叠为单个 `MathKind::Prebuilt` 放回垂直中线基线，避免被无框线表格误吞。

use crate::font::MathKind;
use crate::geom::Glyph;
use crate::math::spatial::{YBuckets, looks_like_prose, render_subexpr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelimKind {
    ParenL,
    ParenR,
    BrackL,
    BrackR,
    BraceL,
    BraceR,
    /// 左右大括号共用的竖直延伸段（`U+23AA` / `U+F8F4`），只能依附于同侧大括号钩，禁止把左右括号粘合。
    BraceExt,
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PiecePos {
    Top,
    Mid,
    Bot,
    Ext,
    Single,
}

fn classify_delim(g: &Glyph, body: f64) -> Option<(DelimKind, PiecePos)> {
    let t = g.text.trim();
    let mut cs = t.chars();
    let (Some(c), None) = (cs.next(), cs.next()) else {
        return None;
    };
    use DelimKind::*;
    use PiecePos::*;
    // Unicode Mathematical Technical (U+239B..U+23AD) & Adobe/Symbol PUA (U+F8EB..U+F8FE)
    let piece = match c {
        '\u{239B}' | '\u{F8EB}' => Some((ParenL, Top)),
        '\u{239C}' | '\u{F8EC}' => Some((ParenL, Ext)),
        '\u{239D}' | '\u{F8ED}' => Some((ParenL, Bot)),
        '\u{239E}' | '\u{F8F6}' => Some((ParenR, Top)),
        '\u{239F}' | '\u{F8F7}' => Some((ParenR, Ext)),
        '\u{23A0}' | '\u{F8F8}' => Some((ParenR, Bot)),
        '\u{23A1}' | '\u{F8EE}' => Some((BrackL, Top)),
        '\u{23A2}' | '\u{F8EF}' => Some((BrackL, Ext)),
        '\u{23A3}' | '\u{F8F0}' => Some((BrackL, Bot)),
        '\u{23A4}' | '\u{F8F9}' => Some((BrackR, Top)),
        '\u{23A5}' | '\u{F8FA}' => Some((BrackR, Ext)),
        '\u{23A6}' | '\u{F8FB}' => Some((BrackR, Bot)),
        '\u{23A7}' | '\u{F8F1}' => Some((BraceL, Top)),
        '\u{23A8}' | '\u{F8F2}' => Some((BraceL, Mid)),
        '\u{23A9}' | '\u{F8F3}' => Some((BraceL, Bot)),
        '\u{23AA}' | '\u{F8F4}' => Some((BraceExt, Ext)),
        '\u{23AB}' | '\u{F8FC}' => Some((BraceR, Top)),
        '\u{23AC}' | '\u{F8FD}' => Some((BraceR, Mid)),
        '\u{23AD}' | '\u{F8FE}' => Some((BraceR, Bot)),
        _ => None,
    };
    if piece.is_some() {
        return piece;
    }
    // 单个扩展定界符（CMEX/TeX-mathx 大号括号或明显放大字号的定界符）
    let tall_single = g.math == MathKind::Extension || g.size >= 1.55 * body;
    if !tall_single {
        return None;
    }
    match c {
        '(' => Some((ParenL, Single)),
        ')' => Some((ParenR, Single)),
        '[' => Some((BrackL, Single)),
        ']' => Some((BrackR, Single)),
        '{' => Some((BraceL, Single)),
        '}' => Some((BraceR, Single)),
        '|' | '∣' => Some((Bar, Single)),
        _ => None,
    }
}

struct DelimBar {
    kind: DelimKind,
    x0: f64,
    x1: f64,
    y_bot: f64,
    y_top: f64,
    ids: Vec<usize>,
    is_stacked: bool,
}

fn matching_close(open: DelimKind) -> Option<DelimKind> {
    match open {
        DelimKind::ParenL => Some(DelimKind::ParenR),
        DelimKind::BrackL => Some(DelimKind::BrackR),
        DelimKind::BraceL => Some(DelimKind::BraceR),
        DelimKind::Bar => Some(DelimKind::Bar),
        _ => None,
    }
}

fn compatible_piece(a: DelimKind, b: DelimKind) -> bool {
    a == b
        || matches!(
            (a, b),
            (DelimKind::BraceL, DelimKind::BraceExt)
                | (DelimKind::BraceExt, DelimKind::BraceL)
                | (DelimKind::BraceR, DelimKind::BraceExt)
                | (DelimKind::BraceExt, DelimKind::BraceR)
        )
}

/// Pass 3：在分式、根号、大算子折叠完成后重构 2D 矩阵与 `\begin{cases}`。
pub(crate) fn reconstruct_matrices(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    body: f64,
    page_width: f64,
) {
    let n_orig = glyphs.len();
    let mut pieces: Vec<(usize, DelimKind, PiecePos)> = Vec::new();
    for i in 0..n_orig {
        if used[i] {
            continue;
        }
        if let Some((k, p)) = classify_delim(&glyphs[i], body) {
            pieces.push((i, k, p));
        }
    }
    if pieces.is_empty() {
        return;
    }

    // 1) 把同一垂直线上堆叠的定界符组件聚合成垂直定界符柱（DelimBar）
    let mut bars: Vec<DelimBar> = Vec::new();
    let mut p_used = vec![false; pieces.len()];
    // 非 Single 堆叠件优先于 Single，同组按 x 升序、y 降序排好，
    // 解决 CMEX 顶部片断映成普通 `[` 时因从上到下扫描导致顶件先被单件消费的问题
    pieces.sort_by(|a, b| {
        (a.2 == PiecePos::Single)
            .cmp(&(b.2 == PiecePos::Single))
            .then(glyphs[a.0].x.total_cmp(&glyphs[b.0].x))
            .then(glyphs[b.0].y.total_cmp(&glyphs[a.0].y))
    });

    for idx in 0..pieces.len() {
        if p_used[idx] {
            continue;
        }
        let (gi, kind, pos) = pieces[idx];
        let g0 = &glyphs[gi];
        let mut ids = vec![gi];
        let mut x0 = g0.x;
        let mut x1 = g0.x_end.max(g0.x + 0.35 * body);
        let mut y_hi = g0.y;
        let mut y_lo = g0.y;
        let mut actual_kind = kind;
        p_used[idx] = true;

        if pos != PiecePos::Single || kind == DelimKind::Bar {
            let cx0 = 0.5 * (x0 + x1);
            let mut changed = true;
            while changed {
                changed = false;
                for j in 0..pieces.len() {
                    if p_used[j] {
                        continue;
                    }
                    let (gj, kj, pj) = pieces[j];
                    let g = &glyphs[gj];
                    let gcx = 0.5 * (g.x + g.x_end.max(g.x + 0.35 * body));
                    if (gcx - cx0).abs() > 0.55 * body {
                        continue;
                    }
                    let kind_ok = if pos == PiecePos::Single {
                        kj == DelimKind::Bar
                    } else {
                        compatible_piece(actual_kind, kj)
                            || (pj == PiecePos::Single
                                && actual_kind != DelimKind::BraceExt
                                && actual_kind == kj)
                    };
                    if !kind_ok {
                        continue;
                    }
                    // 与当前柱的上下端相邻（间距 <= 2.2 * body）
                    if g.y >= y_lo - 2.2 * body && g.y <= y_hi + 2.2 * body {
                        p_used[j] = true;
                        ids.push(gj);
                        x0 = x0.min(g.x);
                        x1 = x1.max(g.x_end.max(g.x + 0.35 * body));
                        y_hi = y_hi.max(g.y);
                        y_lo = y_lo.min(g.y);
                        if actual_kind == DelimKind::BraceExt
                            && matches!(kj, DelimKind::BraceL | DelimKind::BraceR)
                        {
                            actual_kind = kj;
                        }
                        changed = true;
                    }
                }
            }
        }

        // 孤立无钩的大括号延伸杆不构成有效左右定界符
        if actual_kind == DelimKind::BraceExt {
            continue;
        }
        let is_stacked = ids.len() >= 2;
        // 精确计算定界符的视觉垂直包络：
        // CMEX 10pt 单字 `\Bigg(` / `\Bigg[` / `\Bigg\{` 悬挂基线在字形最顶，向下延伸约 2.3*size（覆盖 2 行矩阵/cases），
        // 顶部仅略高于 y_hi（0.30*size），收紧顶部即可杜绝误吸上一行正文而完整保留 2×2 矩阵与 2 行 cases！
        let top_ext = if is_stacked {
            y_hi + 0.65 * body
        } else if g0.math == MathKind::Extension {
            y_hi + 0.30 * g0.size.max(body)
        } else {
            y_hi + 0.85 * g0.size.max(body)
        };
        let bot_ext = if is_stacked {
            y_lo - 0.65 * body
        } else if g0.math == MathKind::Extension {
            y_lo - 2.30 * g0.size.max(body)
        } else {
            y_lo - 0.30 * g0.size.max(body)
        };
        if top_ext - bot_ext < 1.75 * body {
            continue;
        }
        bars.push(DelimBar {
            kind: actual_kind,
            x0,
            x1,
            y_bot: bot_ext,
            y_top: top_ext,
            ids,
            is_stacked,
        });
    }

    if bars.is_empty() {
        return;
    }
    bars.sort_by(|a, b| a.x0.total_cmp(&b.x0));

    let mut bar_used = vec![false; bars.len()];
    let mut qbuf: Vec<usize> = Vec::new();

    // 2) 先由内向外（li 逆序）配对左右定界符（支持嵌套矩阵 `\left(\begin{pmatrix}...\end{pmatrix}\right)`）
    for li in (0..bars.len()).rev() {
        if bar_used[li] {
            continue;
        }
        let Some(need_r) = matching_close(bars[li].kind) else {
            continue;
        };
        // 找右侧最近且垂直范围高度吻合的闭合定界符
        let mut best_ri: Option<usize> = None;
        for ri in (li + 1)..bars.len() {
            if bar_used[ri] || bars[ri].kind != need_r {
                continue;
            }
            let l = &bars[li];
            let r = &bars[ri];
            let span_w = r.x0 - l.x1;
            if span_w < 0.8 * body || span_w > 0.78 * page_width {
                continue;
            }
            let ov_lo = l.y_bot.max(r.y_bot);
            let ov_hi = l.y_top.min(r.y_top);
            let min_h = (l.y_top - l.y_bot).min(r.y_top - r.y_bot).max(1.0);
            if (ov_hi - ov_lo) / min_h < 0.65 {
                continue;
            }
            if (l.y_top - r.y_top).abs() > 1.6 * body || (l.y_bot - r.y_bot).abs() > 1.6 * body {
                continue;
            }
            // 嵌套检查：中间不能再有未消费的同种开定界符柱
            let has_inner_open = ((li + 1)..ri).any(|mid| {
                !bar_used[mid]
                    && bars[mid].kind == l.kind
                    && bars[mid].y_top.min(l.y_top) > bars[mid].y_bot.max(l.y_bot) + body
            });
            if has_inner_open {
                continue;
            }
            best_ri = Some(ri);
            break;
        }
        let Some(ri) = best_ri else { continue };

        let l = &bars[li];
        let r = &bars[ri];
        let y_lo = l.y_bot.min(r.y_bot) - 0.12 * body;
        let y_hi = l.y_top.max(r.y_top) + 0.12 * body;
        let x_lo = l.x1 - 0.10 * body;
        let x_hi = r.x0 + 0.10 * body;

        if try_build_matrix(
            glyphs,
            used,
            buckets,
            l.kind,
            &l.ids,
            Some(&r.ids),
            l.is_stacked || r.is_stacked,
            x_lo,
            x_hi,
            l.x0,
            r.x1,
            y_lo,
            y_hi,
            body,
            false,
            &mut qbuf,
        ) {
            bar_used[li] = true;
            bar_used[ri] = true;
        }
    }

    // 3) 未配对的真正多行左大括号 `{` → `\begin{cases} ... \end{cases}`
    for li in 0..bars.len() {
        if bar_used[li] || bars[li].kind != DelimKind::BraceL {
            continue;
        }
        let l = &bars[li];
        // 单字 `{` 必须足够高（如 CMEX `\Bigg\{` 垂直包络 >= 2.2*body）或是堆叠大括号
        if !l.is_stacked && (l.y_top - l.y_bot) < 2.2 * body {
            continue;
        }
        // 若右侧同高度带已有闭合 `}`，则是成对集合花括号 `\{ ... \}` 而非单侧 `\begin{cases}`
        let has_matching_close = ((li + 1)..bars.len()).any(|ri| {
            !bar_used[ri]
                && bars[ri].kind == DelimKind::BraceR
                && bars[ri].x0 > l.x1
                && (bars[ri].x0 - l.x1) <= 0.78 * page_width
                && {
                    let ov = l.y_top.min(bars[ri].y_top) - l.y_bot.max(bars[ri].y_bot);
                    let mh = (l.y_top - l.y_bot)
                        .min(bars[ri].y_top - bars[ri].y_bot)
                        .max(1.0);
                    ov / mh >= 0.60
                }
        });
        if has_matching_close {
            continue;
        }
        let y_lo = l.y_bot - 0.12 * body;
        let y_hi = l.y_top + 0.12 * body;
        let x_lo = l.x1 - 0.10 * body;
        let x_hi = (l.x1 + 0.65 * page_width).min(page_width);
        if try_build_matrix(
            glyphs,
            used,
            buckets,
            DelimKind::BraceL,
            &l.ids,
            None,
            l.is_stacked,
            x_lo,
            x_hi,
            l.x0,
            x_hi,
            y_lo,
            y_hi,
            body,
            true,
            &mut qbuf,
        ) {
            bar_used[li] = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn try_build_matrix(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    kind: DelimKind,
    left_ids: &[usize],
    right_ids: Option<&[usize]>,
    has_stacked_delim: bool,
    x_lo: f64,
    x_hi: f64,
    outer_x0: f64,
    outer_x1: f64,
    y_lo: f64,
    y_hi: f64,
    body: f64,
    is_cases: bool,
    qbuf: &mut Vec<usize>,
) -> bool {
    if left_ids.iter().any(|&id| used[id])
        || right_ids.is_some_and(|r| r.iter().any(|&id| used[id]))
    {
        return false;
    }
    buckets.query(y_lo, y_hi, qbuf);
    let mut inner: Vec<usize> = qbuf
        .iter()
        .copied()
        .filter(|&i| {
            if used[i] || left_ids.contains(&i) || right_ids.is_some_and(|r| r.contains(&i)) {
                return false;
            }
            let g = &glyphs[i];
            if g.text.trim().is_empty() {
                return false;
            }
            let cx = 0.5 * (g.x + g.x_end.max(g.x));
            g.y >= y_lo && g.y <= y_hi && cx >= x_lo && cx <= x_hi
        })
        .collect();
    if inner.len() < 2 {
        return false;
    }

    // 针对 `\begin{cases}`：截断右侧远处的公式编号 `(182)` 或跨栏右栏正文
    if is_cases {
        strip_far_right_eq_tags(glyphs, &mut inner, x_lo, body);
    }
    if inner.len() < 2 {
        return false;
    }

    // 按垂直投影分成多行（先找大字号/Prebuilt 的主基线层，再把小字号上下标归入最近行）
    let mut rows = cluster_matrix_rows(glyphs, &inner, body);
    // 剥离上下边界误吸的相邻段落截断行（同基线紧贴定界符外侧仍有连续英文正文的行）
    while !rows.is_empty()
        && row_clips_outside_prose(
            glyphs, used, buckets, &rows[0], left_ids, right_ids, outer_x0, outer_x1, body,
            is_cases, qbuf,
        )
    {
        rows.remove(0);
    }
    while !rows.is_empty()
        && row_clips_outside_prose(
            glyphs,
            used,
            buckets,
            rows.last().unwrap(),
            left_ids,
            right_ids,
            outer_x0,
            outer_x1,
            body,
            is_cases,
            qbuf,
        )
    {
        rows.pop();
    }
    if rows.len() < 2 || rows.len() > 16 {
        return false;
    }
    // 内部任意一行若被定界符截断自外侧连续正文，说明这不是矩阵
    if rows.iter().any(|r| {
        row_clips_outside_prose(
            glyphs, used, buckets, r, left_ids, right_ids, outer_x0, outer_x1, body, is_cases, qbuf,
        )
    }) {
        return false;
    }
    // 同步裁剪后的 inner（只消费实际保留下来的行内字形，不吞掉被剥离的上下正文行）
    inner.clear();
    for r in &rows {
        inner.extend_from_slice(r);
    }

    // 防误吞普通正文块：矩阵所有行都不能是散文；cases 允许短条件（如 `for k = 0`），但禁止纯长英文散文句
    if !is_cases && rows.iter().any(|r| looks_like_prose(glyphs, r, body)) {
        return false;
    }
    if is_cases {
        if rows
            .iter()
            .any(|r| looks_like_prose(glyphs, r, body) && !row_has_math_rel(glyphs, r))
        {
            return false;
        }
        let prose_rows = rows
            .iter()
            .filter(|r| looks_like_prose(glyphs, r, body))
            .count();
        if prose_rows * 2 > rows.len() {
            return false;
        }
    }

    // 至少要有一定数学特征（数字、数学字体或符号）
    let has_math = inner.iter().any(|&i| {
        let g = &glyphs[i];
        g.math != MathKind::Text
            || g.text
                .chars()
                .any(|c| c.is_ascii_digit() || "+-=<>()[]".contains(c))
    });
    if !has_math {
        return false;
    }

    // 计算列切分：矩阵按跨行投影谷切分，cases 按宽条件空隙切分
    let col_cuts = if is_cases {
        find_cases_cut(glyphs, &rows, body)
    } else {
        find_matrix_column_cuts(glyphs, &rows, body)
    };
    let ncols = col_cuts.len() + 1;
    // 普通括号 `(...)` 包着 2 行但只有 1 列：仅当两行都是极短原子且无 `\frac` 时输出 `\binom`；
    // 3 行及以上堆叠圆括号列向量（如 `(x_1 \\ x_2 \\ x_3)`）正常输出 `\begin{pmatrix}`。
    if !is_cases && ncols == 1 && kind == DelimKind::ParenL && rows.len() == 2 {
        if inner.iter().any(|&i| glyphs[i].math == MathKind::Prebuilt) {
            return false;
        }
        let (top_tex, _, _) = render_subexpr(glyphs, &rows[0], body);
        let (bot_tex, _, _) = render_subexpr(glyphs, &rows[1], body);
        if top_tex.is_empty() || bot_tex.is_empty() || top_tex.len() > 24 || bot_tex.len() > 24 {
            return false;
        }
        let tex = format!("\\binom{{{top_tex}}}{{{bot_tex}}}");
        return commit_prebuilt(
            glyphs, used, buckets, left_ids, right_ids, &inner, outer_x0, outer_x1, y_lo, y_hi,
            body, tex,
        );
    }
    // 除 cases 外，单列矩阵必须有真正堆叠定界符且至少 3 行（禁止单字 `\Big\{...\Big\}` 包分式误判成 1 列矩阵）
    if !is_cases && ncols < 2 && (!has_stacked_delim || rows.len() < 3) {
        return false;
    }

    let mut row_strs: Vec<String> = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut cells: Vec<Vec<usize>> = vec![Vec::new(); ncols];
        for &id in row {
            let cx = 0.5 * (glyphs[id].x + glyphs[id].x_end.max(glyphs[id].x));
            let col = col_cuts.iter().filter(|&&c| cx > c).count();
            cells[col].push(id);
        }
        let mut cell_strs: Vec<String> = Vec::with_capacity(ncols);
        for c in &cells {
            if c.is_empty() {
                cell_strs.push(String::new());
            } else {
                let (ct, _, _) = render_subexpr(glyphs, c, body);
                cell_strs.push(ct);
            }
        }
        // cases 末尾空列裁剪
        if is_cases {
            while cell_strs.len() > 1 && cell_strs.last().is_some_and(String::is_empty) {
                cell_strs.pop();
            }
        }
        row_strs.push(cell_strs.join(" & "));
    }

    let env = if is_cases {
        "cases"
    } else {
        match kind {
            DelimKind::ParenL => "pmatrix",
            DelimKind::BrackL => "bmatrix",
            DelimKind::BraceL => "Bmatrix",
            DelimKind::Bar => "vmatrix",
            _ => return false,
        }
    };
    let tex = format!(
        "\\begin{{{env}}} {} \\end{{{env}}}",
        row_strs.join(" \\\\ ")
    );

    let actual_x1 = if is_cases {
        inner
            .iter()
            .map(|&i| glyphs[i].x_end.max(glyphs[i].x))
            .fold(outer_x0 + body, f64::max)
    } else {
        outer_x1
    };

    commit_prebuilt(
        glyphs, used, buckets, left_ids, right_ids, &inner, outer_x0, actual_x1, y_lo, y_hi, body,
        tex,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_prebuilt(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    left_ids: &[usize],
    right_ids: Option<&[usize]>,
    inner: &[usize],
    x0: f64,
    x1: f64,
    y_lo: f64,
    y_hi: f64,
    body: f64,
    tex: String,
) -> bool {
    let mcid = glyphs[left_ids[0]].mcid;
    let artifact = glyphs[left_ids[0]].artifact;
    for &id in left_ids {
        used[id] = true;
    }
    if let Some(r) = right_ids {
        for &id in r {
            used[id] = true;
        }
    }
    for &id in inner {
        used[id] = true;
    }
    let eq_y = 0.5 * (y_lo + y_hi) - 0.25 * body;
    let new_idx = glyphs.len();
    glyphs.push(Glyph {
        text: tex.into(),
        x: x0,
        y: eq_y,
        x_end: x1.max(x0 + body),
        size: body,
        bold: false,
        math: MathKind::Prebuilt,
        mcid,
        artifact,
    });
    used.push(false);
    buckets.push(new_idx, eq_y);
    true
}

/// 检查候选矩阵/cases 的某一行是否其实是定界符上下外溢截断的相邻连续正文行：
/// 若同基线紧贴定界符外侧存在普通文字字母或句点（如 `"compu" | "tetheav"`、`"pr" | "oblem..."`），
/// 说明该行横穿定界符边界，属于外侧段落而非矩阵行。
#[allow(clippy::too_many_arguments)]
fn row_clips_outside_prose(
    glyphs: &[Glyph],
    used: &[bool],
    buckets: &YBuckets,
    row: &[usize],
    left_ids: &[usize],
    right_ids: Option<&[usize]>,
    outer_x0: f64,
    outer_x1: f64,
    body: f64,
    is_cases: bool,
    qbuf: &mut Vec<usize>,
) -> bool {
    if row.is_empty() {
        return false;
    }
    let mut ys: Vec<f64> = row
        .iter()
        .filter(|&&i| glyphs[i].size >= 0.80 * body)
        .map(|&i| glyphs[i].y)
        .collect();
    if ys.is_empty() {
        ys = row.iter().map(|&i| glyphs[i].y).collect();
    }
    ys.sort_by(f64::total_cmp);
    let row_y = ys[ys.len() / 2];
    let (mut rx0, mut rx1) = (f64::INFINITY, f64::NEG_INFINITY);
    for &i in row {
        rx0 = rx0.min(glyphs[i].x);
        rx1 = rx1.max(glyphs[i].x_end.max(glyphs[i].x));
    }
    buckets.query(row_y - 0.24 * body, row_y + 0.24 * body, qbuf);
    for &i in qbuf.iter() {
        if used[i]
            || row.contains(&i)
            || left_ids.contains(&i)
            || right_ids.is_some_and(|r| r.contains(&i))
        {
            continue;
        }
        let g = &glyphs[i];
        if g.math != MathKind::Text || g.text.trim().is_empty() || (g.y - row_y).abs() > 0.22 * body
        {
            continue;
        }
        let is_prose_char = g
            .text
            .chars()
            .any(|c| c.is_ascii_alphabetic() || matches!(c, '.' | ','));
        if !is_prose_char {
            continue;
        }
        let gx1 = g.x_end.max(g.x);
        let left_clip = gx1 <= outer_x0 + 0.20 * body
            && (rx0 - gx1) < 0.65 * body
            && (outer_x0 - gx1) < 0.55 * body;
        let right_clip = !is_cases
            && g.x >= outer_x1 - 0.20 * body
            && (g.x - rx1) < 0.65 * body
            && (g.x - outer_x1) < 0.55 * body;
        if left_clip || right_clip {
            return true;
        }
    }
    false
}

/// 判断 `\begin{cases}` 候选行是否包含数学关系/比较符（区别于误吞的纯自然语言散文句）。
fn row_has_math_rel(glyphs: &[Glyph], row: &[usize]) -> bool {
    row.iter().any(|&i| {
        let g = &glyphs[i];
        g.math == MathKind::Prebuilt
            || g.text.chars().any(|c| {
                matches!(
                    c,
                    '=' | '<'
                        | '>'
                        | '≤'
                        | '≥'
                        | '≠'
                        | '∈'
                        | '∉'
                        | '⊂'
                        | '⊆'
                        | '∀'
                        | '∃'
                        | '≈'
                )
            })
    })
}

/// 剥离 `\begin{cases}` 右侧远处（> 4.5*body 空白之外）的公式编号如 `(182)`。
fn strip_far_right_eq_tags(glyphs: &[Glyph], inner: &mut Vec<usize>, x_lo: f64, body: f64) {
    let mut xs: Vec<(f64, f64)> = inner
        .iter()
        .map(|&i| (glyphs[i].x, glyphs[i].x_end.max(glyphs[i].x)))
        .collect();
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut max_end = x_lo;
    let mut cut_x = f64::INFINITY;
    for (a, b) in xs {
        if max_end > x_lo + body && a - max_end > 4.2 * body {
            cut_x = a - 0.1;
            break;
        }
        max_end = max_end.max(b);
    }
    if cut_x.is_finite() {
        inner.retain(|&i| glyphs[i].x < cut_x);
    }
}

/// 把矩阵内部字形按 Y 投影聚成行（主字号定行基线，小字号上下标归入最近行）。
fn cluster_matrix_rows(glyphs: &[Glyph], ids: &[usize], body: f64) -> Vec<Vec<usize>> {
    let mut majors: Vec<usize> = ids
        .iter()
        .copied()
        .filter(|&i| glyphs[i].size >= 0.80 * body || glyphs[i].math == MathKind::Prebuilt)
        .collect();
    if majors.is_empty() {
        majors = ids.to_vec();
    }
    majors.sort_by(|&a, &b| glyphs[b].y.total_cmp(&glyphs[a].y));
    let mut row_ys: Vec<f64> = Vec::new();
    for &i in &majors {
        let y = glyphs[i].y;
        match row_ys.last_mut() {
            Some(ry) if (*ry - y).abs() <= 0.62 * body => {}
            _ => row_ys.push(y),
        }
    }
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); row_ys.len()];
    for &i in ids {
        let y = glyphs[i].y;
        let best = row_ys
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| ((*a - y).abs()).total_cmp(&((*b - y).abs())))
            .map(|(k, _)| k)
            .unwrap_or(0);
        rows[best].push(i);
    }
    rows.retain(|r| !r.is_empty());
    rows
}

/// 通过跨行水平投影谷查找矩阵列切分点。
fn find_matrix_column_cuts(glyphs: &[Glyph], rows: &[Vec<usize>], body: f64) -> Vec<f64> {
    let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in rows {
        for &i in r {
            x0 = x0.min(glyphs[i].x);
            x1 = x1.max(glyphs[i].x_end.max(glyphs[i].x));
        }
    }
    let width = x1 - x0;
    if !width.is_finite() || width <= body {
        return Vec::new();
    }
    let bins = ((width * 2.0).ceil() as usize).clamp(4, 2048);
    let scale = bins as f64 / width;
    let mut cover = vec![0usize; bins];
    for r in rows {
        for &i in r {
            let a = (((glyphs[i].x - x0) * scale).floor().max(0.0) as usize).min(bins - 1);
            let b = (((glyphs[i].x_end.max(glyphs[i].x) - x0) * scale)
                .ceil()
                .max(0.0) as usize)
                .max(a + 1)
                .min(bins);
            for c in &mut cover[a..b] {
                *c += 1;
            }
        }
    }
    let min_gap = 0.55 * body;
    let mut cuts = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &c) in cover.iter().enumerate() {
        if c == 0 {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take()
            && s > 0
            && (i - s) as f64 / scale >= min_gap
        {
            cuts.push(x0 + 0.5 * (s + i) as f64 / scale);
        }
    }
    cuts
}

/// 为 `\begin{cases}` 查找单一对齐条件列分界（表达式与 `for ...` / `if ...` 之间的明显水平空白）。
fn find_cases_cut(glyphs: &[Glyph], rows: &[Vec<usize>], body: f64) -> Vec<f64> {
    let cuts = find_matrix_column_cuts(glyphs, rows, body * 1.4);
    if let Some(&first) = cuts.first() {
        vec![first]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::spatial::reconstruct;

    fn g(text: &str, x: f64, y: f64, size: f64, math: MathKind) -> Glyph {
        Glyph {
            text: text.into(),
            x,
            y,
            x_end: x + 0.5 * size * text.chars().count().max(1) as f64,
            size,
            bold: false,
            math,
            mcid: None,
            artifact: false,
        }
    }

    #[test]
    fn reconstructs_2x2_bmatrix_from_stacked_delims() {
        let mut glyphs = vec![
            // Left stacked bracket
            g("\u{F8EE}", 100.0, 520.0, 10.0, MathKind::Extension),
            g("\u{F8F0}", 100.0, 504.0, 10.0, MathKind::Extension),
            // Row 1: 1  0
            g("1", 110.0, 518.0, 10.0, MathKind::Text),
            g("0", 130.0, 518.0, 10.0, MathKind::Text),
            // Row 2: 0  1
            g("0", 110.0, 502.0, 10.0, MathKind::Text),
            g("1", 130.0, 502.0, 10.0, MathKind::Text),
            // Right stacked bracket
            g("\u{F8F9}", 142.0, 520.0, 10.0, MathKind::Extension),
            g("\u{F8FB}", 142.0, 504.0, 10.0, MathKind::Extension),
        ];
        let mut rules = Vec::new();
        reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(
            glyphs.iter().any(|x| x.math == MathKind::Prebuilt
                && x.text == "\\begin{bmatrix} 1 & 0 \\\\ 0 & 1 \\end{bmatrix}"),
            "got: {glyphs:?}"
        );
    }

    #[test]
    fn reconstructs_piecewise_cases_from_left_brace() {
        let mut glyphs = vec![
            g("\u{F8F1}", 100.0, 522.0, 10.0, MathKind::Extension),
            g("\u{F8F2}", 100.0, 512.0, 10.0, MathKind::Extension),
            g("\u{F8F3}", 100.0, 502.0, 10.0, MathKind::Extension),
            g("0", 112.0, 518.0, 10.0, MathKind::Text),
            g("x", 136.0, 518.0, 10.0, MathKind::Italic),
            g("=", 143.0, 518.0, 10.0, MathKind::Text),
            g("0", 150.0, 518.0, 10.0, MathKind::Text),
            g("1", 112.0, 504.0, 10.0, MathKind::Text),
            g("x", 136.0, 504.0, 10.0, MathKind::Italic),
            g(">", 143.0, 504.0, 10.0, MathKind::Text),
            g("0", 150.0, 504.0, 10.0, MathKind::Text),
        ];
        let mut rules = Vec::new();
        reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(
            glyphs.iter().any(|x| x.math == MathKind::Prebuilt
                && x.text == "\\begin{cases} 0 & x=0 \\\\ 1 & x>0 \\end{cases}"),
            "got: {glyphs:?}"
        );
    }

    #[test]
    fn single_line_big_braces_and_brackets_never_swallow_adjacent_prose() {
        // 复现 2503.05175_pg7 / 2503.05562_pg16 / 2503.08646_pg17：单行 \Big\{ ... \Big\} 上下各有一行英文正文
        let mut glyphs = vec![
            // 上一行正文（横穿花括号左右边界）
            g("See", 78.0, 524.0, 10.0, MathKind::Text),
            g("Figure", 104.0, 524.0, 10.0, MathKind::Text),
            g("4", 138.0, 524.0, 10.0, MathKind::Text),
            g("for", 148.0, 524.0, 10.0, MathKind::Text),
            // 中间单行公式：\Big\{ \pi(x) : x \in X \Big\}
            g("{", 100.0, 513.0, 10.0, MathKind::Extension),
            g("π", 108.0, 508.0, 10.0, MathKind::Italic),
            g("(", 114.0, 508.0, 10.0, MathKind::Text),
            g("x", 118.0, 508.0, 10.0, MathKind::Italic),
            g(")", 124.0, 508.0, 10.0, MathKind::Text),
            g(":", 129.0, 508.0, 10.0, MathKind::Text),
            g("x", 134.0, 508.0, 10.0, MathKind::Italic),
            g("∈", 140.0, 508.0, 10.0, MathKind::Symbol),
            g("X", 148.0, 508.0, 10.0, MathKind::Italic),
            g("}", 156.0, 513.0, 10.0, MathKind::Extension),
            // 下一行正文
            g("as", 82.0, 494.0, 10.0, MathKind::Text),
            g("a", 95.0, 494.0, 10.0, MathKind::Text),
            g("union", 104.0, 494.0, 10.0, MathKind::Text),
            g("of", 135.0, 494.0, 10.0, MathKind::Text),
            g("points", 148.0, 494.0, 10.0, MathKind::Text),
        ];
        let mut rules = Vec::new();
        reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(
            !glyphs.iter().any(|x| x.math == MathKind::Prebuilt
                && (x.text.contains("\\begin{cases}") || x.text.contains("\\begin{Bmatrix}"))),
            "single-line paired brace must not swallow surrounding prose: {glyphs:?}"
        );
    }

    #[test]
    fn stacks_single_top_bracket_and_reconstructs_3x1_pmatrix() {
        // 顶部 `[` 为 PiecePos::Single、底部为 `⎣` 的 CMEX 堆叠方括号
        let mut b_glyphs = vec![
            g("[", 100.0, 520.0, 10.0, MathKind::Extension),
            g("\u{23A3}", 100.0, 504.0, 10.0, MathKind::Extension),
            g("a", 110.0, 518.0, 10.0, MathKind::Italic),
            g("b", 130.0, 518.0, 10.0, MathKind::Italic),
            g("c", 110.0, 502.0, 10.0, MathKind::Italic),
            g("d", 130.0, 502.0, 10.0, MathKind::Italic),
            g("]", 142.0, 520.0, 10.0, MathKind::Extension),
            g("\u{23A6}", 142.0, 504.0, 10.0, MathKind::Extension),
        ];
        let mut rules = Vec::new();
        reconstruct(&mut b_glyphs, &mut rules, 600.0, 800.0);
        assert!(
            b_glyphs.iter().any(|x| x.math == MathKind::Prebuilt
                && x.text == "\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}"),
            "top Single bracket must stack with bottom piece: {b_glyphs:?}"
        );

        // 3×1 堆叠圆括号列向量
        let mut v_glyphs = vec![
            g("\u{239B}", 100.0, 532.0, 10.0, MathKind::Extension),
            g("\u{239C}", 100.0, 518.0, 10.0, MathKind::Extension),
            g("\u{239D}", 100.0, 504.0, 10.0, MathKind::Extension),
            g("x", 112.0, 530.0, 10.0, MathKind::Italic),
            g("y", 112.0, 516.0, 10.0, MathKind::Italic),
            g("z", 112.0, 502.0, 10.0, MathKind::Italic),
            g("\u{239E}", 124.0, 532.0, 10.0, MathKind::Extension),
            g("\u{239F}", 124.0, 518.0, 10.0, MathKind::Extension),
            g("\u{23A0}", 124.0, 504.0, 10.0, MathKind::Extension),
        ];
        reconstruct(&mut v_glyphs, &mut rules, 600.0, 800.0);
        assert!(
            v_glyphs.iter().any(|x| x.math == MathKind::Prebuilt
                && x.text == "\\begin{pmatrix} x \\\\ y \\\\ z \\end{pmatrix}"),
            "3x1 stacked pmatrix column vector must reconstruct: {v_glyphs:?}"
        );
    }
}
