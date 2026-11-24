//! 基于递归 X-Y Cut（Nagy & Seth 1984; Ha, Haralick & Phillips 1995）与局部最大空白带
//! （Breuel 2002）的版面分栏与阅读顺序重建。
//!
//! 相比全页单一 1D 投影直方图，先按跨栏元素（标题、摘要、表格、图区）和显著垂直空隙切出水平带，
//! 再在每个水平带内部独立检测栏间空白并递归细分，可正确处理：
//! - 顶栏通排（标题 + 摘要）+ 下方双栏/三栏论文版式；
//! - 报纸/杂志的混排多栏（顶部 2 栏、底部 3–4 栏、侧边栏嵌套子栏）；
//! - 防止单栏含右对齐公式编号或缩进列表被误拆成两栏。

use super::{CJK_SPACE_GAP, Line, is_cjk};

/// 递归切分的最大深度，避免病态版面过深递归。
const MAX_XY_DEPTH: usize = 5;

/// 按阅读顺序把一页的片段分成块：每块是同一栏里连续的行，跨栏的内容单独成块。
pub(crate) fn blocks(frags: Vec<Line>) -> Vec<Vec<Line>> {
    if frags.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    segment_region(frags, 0, &mut out);
    out
}

/// 递归 X-Y Cut：
/// 1) 优先在当前区域直接检测局部栏间空白（避免紧窄栏距被误判为通排行）；
/// 2) 若检测到栏间空白，检查是否存在大垂直空白切换不同栏数（如上 2 栏下 3 栏），否则按栏切分并递归细分各子栏；
/// 3) 若整区未找到栏间空白（常因大段通排标题/摘要/横幅挡住投影），再剥离真正连续无内隙的通排行，
///    对非通排带按大 Y 空白拆分后独立做局部栏切分。
fn segment_region(mut frags: Vec<Line>, depth: usize, out: &mut Vec<Vec<Line>>) {
    if frags.is_empty() {
        return;
    }
    frags.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x0.total_cmp(&b.x0)));
    if depth >= MAX_XY_DEPTH || frags.len() < 4 {
        out.push(merge_rows(frags));
        return;
    }

    let left = frags.iter().map(|f| f.x0).fold(f64::INFINITY, f64::min);
    let right = frags.iter().map(|f| f.x1).fold(f64::NEG_INFINITY, f64::max);
    let width = right - left;
    if !width.is_finite() || width <= 1.0 {
        out.push(merge_rows(frags));
        return;
    }
    let med = median_size(&frags);

    // 路径 A：整区已直接检测到栏间空白
    let direct_gs = local_gutters(&frags);
    if !direct_gs.is_empty() {
        // 仅当存在显著大 Y 空隙把带分成栏数/栏位明显不同的上下块时，才先做 Y-Cut
        let rows = group_y_rows(frags);
        let sub_bands = split_by_large_y_gaps(rows, left, width, med, Some(&direct_gs));
        if sub_bands.len() > 1 {
            for sb in sub_bands {
                let flat: Vec<Line> = sb.into_iter().flatten().collect();
                segment_band_columns(flat, width, depth, out);
            }
        } else {
            let flat: Vec<Line> = sub_bands.into_iter().flatten().flatten().collect();
            split_with_gutters(flat, &direct_gs, width, depth, out);
        }
        return;
    }

    // 路径 B：整区无全局栏间空白 → 剥离真正连续跨栏的通排行（摘要/通排标题/宽表）后再局部切分；
    // 只有当某个非通排子带确实存在栏间空白时才切分，否则合并回单一连续块，避免把单栏正文/参考文献切碎。
    let rows = group_y_rows(frags);
    if rows.len() < 2 {
        let flat: Vec<Line> = rows.into_iter().flatten().collect();
        out.push(merge_rows(flat));
        return;
    }

    // 与 gutters_of 一致的紧窄空隙阈值（禁止把 5–7pt 窄栏误判成连续通排行！）
    let min_col_gap = (0.68 * med).max(3.5);
    let spanning: Vec<bool> = rows
        .iter()
        .map(|r| is_spanning_row(r, left, width, min_col_gap))
        .collect();

    if spanning.iter().all(|&s| s) {
        let flat: Vec<Line> = rows.into_iter().flatten().collect();
        out.push(merge_rows(flat));
        return;
    }

    let mut bands: Vec<(bool, Vec<Vec<Line>>)> = Vec::new();
    for (r, is_span) in rows.into_iter().zip(spanning) {
        match bands.last_mut() {
            Some((sp, list)) if *sp == is_span => list.push(r),
            _ => bands.push((is_span, vec![r])),
        }
    }

    let mut pending: Vec<Line> = Vec::new();
    let flush_pending = |pending: &mut Vec<Line>, out: &mut Vec<Vec<Line>>| {
        if !pending.is_empty() {
            let mut p = std::mem::take(pending);
            p.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x0.total_cmp(&b.x0)));
            out.push(merge_rows(p));
        }
    };

    for (is_span, band_rows) in bands {
        if is_span {
            pending.extend(band_rows.into_iter().flatten());
        } else {
            for sb in split_by_large_y_gaps(band_rows, left, width, med, None) {
                let mut flat: Vec<Line> = sb.into_iter().flatten().collect();
                if flat.is_empty() {
                    continue;
                }
                flat.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x0.total_cmp(&b.x0)));
                let gs = local_gutters(&flat);
                if gs.is_empty() {
                    pending.extend(flat);
                } else {
                    flush_pending(&mut pending, out);
                    split_with_gutters(flat, &gs, width, depth, out);
                }
            }
        }
    }
    flush_pending(&mut pending, out);
}

/// 对一个水平带做局部栏间空白检测并切分。
fn segment_band_columns(
    mut frags: Vec<Line>,
    parent_width: f64,
    depth: usize,
    out: &mut Vec<Vec<Line>>,
) {
    if frags.is_empty() {
        return;
    }
    frags.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x0.total_cmp(&b.x0)));
    let gs = local_gutters(&frags);
    if gs.is_empty() {
        out.push(merge_rows(frags));
        return;
    }
    split_with_gutters(frags, &gs, parent_width, depth, out);
}

/// 按给定栏间空白 `gs` 把 `frags` 分成跨栏块与各列，并对宽子容器按需递归细分。
fn split_with_gutters(
    frags: Vec<Line>,
    gs: &[(f64, f64)],
    parent_width: f64,
    depth: usize,
    out: &mut Vec<Vec<Line>>,
) {
    let column_of = |f: &Line| -> Option<usize> {
        let mut col = 0;
        for (i, &(g0, g1)) in gs.iter().enumerate() {
            let mid = (g0 + g1) / 2.0;
            if f.x0 < g0 - 8.0 && f.x1 > g1 + 8.0 {
                return None;
            }
            if (f.x0 + f.x1) / 2.0 >= mid {
                col = i + 1;
            }
        }
        Some(col)
    };

    let med = median_size(&frags);
    let mut band: Vec<Vec<Line>> = vec![Vec::new(); gs.len() + 1];
    let mut spanning: Vec<Line> = Vec::new();
    let flush_cols = |band: &mut [Vec<Line>], out: &mut Vec<Vec<Line>>| {
        for col in band.iter_mut() {
            if !col.is_empty() {
                let c = std::mem::take(col);
                let cx0 = c.iter().map(|f| f.x0).fold(f64::INFINITY, f64::min);
                let cx1 = c.iter().map(|f| f.x1).fold(f64::NEG_INFINITY, f64::max);
                let cw = cx1 - cx0;
                // 仅当子区域占父宽 >= 55%（如 1/3 侧栏旁的 2/3 嵌套多栏主区）且宽 >= 26*med 时再细分，
                // 避免在普通双栏小字词典/报纸的单个窄栏内部沿词间“文字河流”误劈成两半
                if depth < 1 && c.len() >= 12 && cw >= 0.55 * parent_width && cw >= 26.0 * med {
                    segment_region(c, depth + 1, out);
                } else {
                    out.push(merge_rows(c));
                }
            }
        }
    };
    for f in frags {
        match column_of(&f) {
            Some(c) => {
                if !spanning.is_empty() {
                    out.push(merge_rows(std::mem::take(&mut spanning)));
                }
                band[c].push(f);
            }
            None => {
                flush_cols(&mut band, out);
                spanning.push(f);
            }
        }
    }
    if !spanning.is_empty() {
        out.push(merge_rows(spanning));
    }
    flush_cols(&mut band, out);
}

/// 把片段按基线临时聚成水平行（输入已按 y 降序排好）。
fn group_y_rows(frags: Vec<Line>) -> Vec<Vec<Line>> {
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for f in frags {
        match rows.last_mut() {
            Some(r)
                if !f.raw && !r[0].raw && (r[0].y - f.y).abs() < 0.35 * r[0].size.max(f.size) =>
            {
                r.push(f)
            }
            _ => rows.push(vec![f]),
        }
    }
    for r in &mut rows {
        r.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    }
    rows
}

/// 一行是不是真正通排（跨栏）的行：
/// - `raw` 块（宽表格/跨栏图区，宽度 >= 0.58 * width）是通排；窄的 raw 块留在栏内；
/// - 文本行如果总宽 >= 0.62 * width，且中部 `[left + 0.15*w, right - 0.15*w]` 内没有任何
///   宽度 >= `min_col_gap` 的内部水平空隙（即不是两栏并排拼成的），才是真正的通排行！
fn is_spanning_row(row: &[Line], left: f64, width: f64, min_col_gap: f64) -> bool {
    if row.is_empty() {
        return false;
    }
    let rx0 = row.iter().map(|f| f.x0).fold(f64::INFINITY, f64::min);
    let rx1 = row.iter().map(|f| f.x1).fold(f64::NEG_INFINITY, f64::max);
    let rw = rx1 - rx0;
    if row.iter().any(|f| f.raw) {
        return rw >= 0.58 * width;
    }
    if rw < 0.62 * width {
        return false;
    }
    // 合并相邻间隔 <= min_col_gap 的区间，看是否存在单段连续覆盖 >= 0.58 * width
    let mut max_run = 0.0f64;
    let mut cur_x0 = row[0].x0;
    let mut cur_x1 = row[0].x1;
    for f in &row[1..] {
        if f.x0 - cur_x1 <= min_col_gap {
            cur_x1 = cur_x1.max(f.x1);
        } else {
            max_run = max_run.max(cur_x1 - cur_x0);
            cur_x0 = f.x0;
            cur_x1 = f.x1;
        }
    }
    max_run = max_run.max(cur_x1 - cur_x0);
    // 也检查中部区域是否出现大空隙
    let mid_lo = left + 0.15 * width;
    let mid_hi = left + 0.85 * width;
    let has_mid_gap = row.windows(2).any(|w| {
        let g0 = w[0].x1;
        let g1 = w[1].x0;
        g1 - g0 >= min_col_gap && g0 >= mid_lo && g1 <= mid_hi
    });
    max_run >= 0.58 * width && !has_mid_gap
}

/// 在一个水平带内部，按显著垂直空隙（> 2.6 倍典型行距）做 Y-Cut，防止上 2 栏下 3 栏或通排摘要+双栏互相干扰。
fn split_by_large_y_gaps(
    rows: Vec<Vec<Line>>,
    left: f64,
    width: f64,
    med: f64,
    existing_gs: Option<&[(f64, f64)]>,
) -> Vec<Vec<Vec<Line>>> {
    if rows.len() < 8 {
        return vec![rows];
    }
    let row_y = |r: &Vec<Line>| r[0].y;
    let mut diffs: Vec<f64> = rows
        .windows(2)
        .map(|w| row_y(&w[0]) - row_y(&w[1]))
        .filter(|&d| d > 0.4 * med && d < 3.0 * med)
        .collect();
    if diffs.len() < 4 {
        return vec![rows];
    }
    diffs.sort_by(f64::total_cmp);
    let lead = diffs[diffs.len() / 2].max(med);
    let cut_gap = (2.6 * lead).max(3.2 * med);

    let mut split_pts: Vec<usize> = Vec::new();
    for i in 1..rows.len() {
        let gap = row_y(&rows[i - 1]) - row_y(&rows[i]);
        if gap >= cut_gap {
            split_pts.push(i);
        }
    }
    if split_pts.is_empty() {
        return vec![rows];
    }
    let whole_gs: Vec<(f64, f64)> = match existing_gs {
        Some(g) => g.to_vec(),
        None => {
            let all_flat: Vec<Line> = rows.iter().flatten().cloned().collect();
            local_gutters(&all_flat)
        }
    };

    let mut out = Vec::new();
    let mut start = 0;
    for pt in split_pts.into_iter().chain(std::iter::once(rows.len())) {
        if pt > start {
            out.push(rows[start..pt].to_vec());
        }
        start = pt;
    }
    let min_col_gap = (0.68 * med).max(3.5);
    // 验证是否真的发现了新的多栏结构或独立的通排摘要/标题带
    let differs = out.iter().any(|sub| {
        if sub.len() < 4 {
            return false;
        }
        let flat: Vec<Line> = sub.iter().flatten().cloned().collect();
        let g = local_gutters(&flat);
        if whole_gs.is_empty() {
            !g.is_empty()
        } else {
            let span_band = g.is_empty()
                && sub
                    .iter()
                    .filter(|r| is_spanning_row(r, left, width, min_col_gap))
                    .count()
                    * 2
                    >= sub.len();
            // 子带出现任意与全局栏隙明显错位（>14pt）的新栏隙（如上 2 栏下 3 栏、或上 3 栏下 2 栏）
            let new_gutter = g.iter().any(|a| {
                let am = 0.5 * (a.0 + a.1);
                whole_gs
                    .iter()
                    .all(|b| (am - 0.5 * (b.0 + b.1)).abs() > 14.0)
            });
            span_band || new_gutter
        }
    });
    if differs {
        out
    } else {
        vec![out.into_iter().flatten().collect()]
    }
}

/// 在单个水平带内检测局部栏间空白，返回 `[g0, g1]` 列表。
fn local_gutters(frags: &[Line]) -> Vec<(f64, f64)> {
    let refs: Vec<&Line> = frags.iter().collect();
    gutters_of(&refs)
}

/// 找一组片段的栏间空白（供 `table::extract` 和 `columns` 共用）。
/// 结合局部严格零交叉扫描 + 放宽重叠带验证，支持 2/3/4 栏且防止单栏公式/编号误拆。
pub(crate) fn gutters_of(frags: &[&Line]) -> Vec<(f64, f64)> {
    let left = frags.iter().map(|f| f.x0).fold(f64::INFINITY, f64::min);
    let right = frags.iter().map(|f| f.x1).fold(f64::NEG_INFINITY, f64::max);
    let width = right - left;
    if width.is_nan() || width <= 1.0 || frags.len() < 6 {
        return Vec::new();
    }
    // 原样输出的块（表格、图内文字）和过宽的片段不当候选
    let cands: Vec<&Line> = frags
        .iter()
        .copied()
        .filter(|f| f.x1 - f.x0 < 0.56 * width && !f.raw)
        .collect();
    if cands.len() < 6 {
        return Vec::new();
    }
    let mut sizes: Vec<f64> = cands.iter().map(|f| f.size).collect();
    sizes.sort_by(f64::total_cmp);
    let med = sizes[sizes.len() / 2];
    let min_gap = (0.55 * med).max(3.5);

    // 1pt 精度的覆盖直方图，宽页面按比例缩放
    let bins = (width.ceil() as usize).clamp(1, 4000);
    let scale = bins as f64 / width;
    let mut cover = vec![0usize; bins + 1];
    for f in &cands {
        let a = (((f.x0 - left) * scale).floor().max(0.0) as usize).min(bins);
        let b = (((f.x1 - left) * scale).ceil().max(0.0) as usize).min(bins);
        for c in &mut cover[a..b.max(a)] {
            *c += 1;
        }
    }
    let tol = (cands.len() / 20).max(1);
    let out = scan(frags, &cands, &cover, tol, left, scale, min_gap, None);
    if !out.is_empty() {
        return out;
    }
    // 放宽扫描：只看两边正文字号行并排的高度带，那里没有行跨过空白就算栏间空白
    // 同时对 3 栏/4 栏放宽 `wide` 阈值（由 0.25 放宽到 0.16，支持 3-4 栏报纸/词典）
    scan(
        frags,
        &cands,
        &cover,
        (cands.len() / 4).max(1),
        left,
        scale,
        min_gap,
        Some((tol, med)),
    )
}

#[allow(clippy::too_many_arguments)]
fn scan(
    all_frags: &[&Line],
    cands: &[&Line],
    cover: &[usize],
    tol: usize,
    left: f64,
    scale: f64,
    min_gap: f64,
    band_tol: Option<(usize, f64)>,
) -> Vec<(f64, f64)> {
    let bins = cover.len() - 1;
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &c) in cover.iter().enumerate() {
        let empty = i < bins && c <= tol;
        match (empty, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                start = None;
                let (g0, g1) = (left + s as f64 / scale, left + i as f64 / scale);
                if s == 0 || i >= bins || g1 - g0 < min_gap {
                    continue;
                }
                let lhs: Vec<&&Line> = cands.iter().filter(|f| f.x1 <= g0 + 0.5).collect();
                let rhs: Vec<&&Line> = cands.iter().filter(|f| f.x0 >= g1 - 0.5).collect();
                let need = (cands.len() / 12).max(3);
                if lhs.len() < need || rhs.len() < need {
                    continue;
                }
                // 统计验证 1：两侧必须有垂直高度重叠，且各自有足够独立的基线行数与非零实质文字
                // （防止单栏页面右对齐公式编号 `(1)` 或页边短标签把单栏正文误拆成两栏）
                let y_range = |v: &[&&Line]| {
                    v.iter()
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), f| {
                            (lo.min(f.y), hi.max(f.y))
                        })
                };
                let ((ly0, ly1), (ry0, ry1)) = (y_range(&lhs), y_range(&rhs));
                let (ov_lo, ov_hi) = (ly0.max(ry0), ly1.min(ry1));
                if ov_lo >= ov_hi {
                    continue;
                }
                let side_chars = |v: &[&&Line]| -> usize {
                    v.iter()
                        .map(|f| f.text.chars().filter(|c| !c.is_whitespace()).count())
                        .sum()
                };
                if side_chars(&lhs) < 15 || side_chars(&rhs) < 15 {
                    continue;
                }
                // 防单栏两端对齐页（如参考文献）的少数短碎片在大量通排正文行之间误拼出伪栏隙
                let full_crossing = all_frags
                    .iter()
                    .filter(|f| {
                        !f.raw && f.x0 < g0 - 0.5 && f.x1 > g1 + 0.5 && f.y >= ov_lo && f.y <= ov_hi
                    })
                    .count();
                if full_crossing >= lhs.len().min(rhs.len()) {
                    continue;
                }
                if let Some((band_tol, med)) = band_tol {
                    let full = bins as f64 / scale;
                    let wide = |v: &[&&Line]| {
                        let mut w: Vec<f64> = v
                            .iter()
                            .filter(|f| (f.size - med).abs() < 0.25 * med.max(4.0))
                            .map(|f| f.x1 - f.x0)
                            .collect();
                        w.sort_by(f64::total_cmp);
                        w.get(w.len() / 2).is_some_and(|&m| m >= 0.16 * full)
                    };
                    if !wide(&lhs) || !wide(&rhs) {
                        continue;
                    }
                    let span = |v: &[&&Line]| {
                        v.iter()
                            .filter(|f| (f.size - med).abs() < 0.25 * med.max(4.0))
                            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), f| {
                                (lo.min(f.y), hi.max(f.y))
                            })
                    };
                    let ((l0, l1), (r0, r1)) = (span(&lhs), span(&rhs));
                    let (lo, hi) = (l0.max(r0), l1.min(r1));
                    let crossing = all_frags
                        .iter()
                        .filter(|f| {
                            !f.raw && f.x0 < g0 - 0.5 && f.x1 > g1 + 0.5 && f.y >= lo && f.y <= hi
                        })
                        .count();
                    if lo >= hi || crossing > band_tol {
                        continue;
                    }
                } else {
                    // 严格扫描下防右对齐公式编号 `(1)`/`[2]` 或有序列表编号 `1.` 误拆单栏；
                    // 允许稿件页边连续行号（纯整数 `387`、`388`...）与正文分栏，避免行号混入句中
                    let looks_like_eq_tags = |v: &[&&Line]| {
                        let tags = v
                            .iter()
                            .filter(|f| {
                                let t = f.text.trim();
                                t.len() <= 6
                                    && ((t.starts_with('(') && t.ends_with(')'))
                                        || (t.starts_with('[') && t.ends_with(']'))
                                        || (t.contains('.')
                                            && t.chars().all(|c| c.is_ascii_digit() || c == '.')))
                            })
                            .count();
                        tags * 5 >= v.len() * 4
                    };
                    if looks_like_eq_tags(&lhs) || looks_like_eq_tags(&rhs) {
                        continue;
                    }
                }
                out.push((g0, g1));
            }
            _ => {}
        }
    }
    out
}

fn median_size(frags: &[Line]) -> f64 {
    let mut sizes: Vec<f64> = frags.iter().map(|f| f.size).collect();
    sizes.sort_by(f64::total_cmp);
    sizes.get(sizes.len() / 2).copied().unwrap_or(10.0).max(1.0)
}

/// 把同一栏里基线相同的片段合成一行（片段已按 y 降序排好）。
pub(super) fn merge_rows(frags: Vec<Line>) -> Vec<Line> {
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for f in frags {
        match rows.last_mut() {
            Some(r)
                if !f.raw
                    && !r[0].raw
                    && r[0].group == f.group
                    && (r[0].y - f.y).abs() < 0.35 * r[0].size.max(f.size) =>
            {
                r.push(f)
            }
            _ => rows.push(vec![f]),
        }
    }
    rows.into_iter()
        .filter_map(|mut r| {
            r.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            let mut it = r.into_iter();
            let mut line = it.next()?;
            for f in it {
                let cjk = line.text.chars().last().is_some_and(is_cjk)
                    && f.text.chars().next().is_some_and(is_cjk)
                    && f.x0 - line.x1 < CJK_SPACE_GAP * f.size;
                if !cjk {
                    line.text.push(' ');
                }
                line.text.push_str(&f.text);
                line.x1 = line.x1.max(f.x1);
                if f.text.chars().count() > line.text.chars().count() / 2 {
                    line.size = line.size.max(f.size);
                }
                line.bold &= f.bold;
            }
            Some(line)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x0: f64, x1: f64, y: f64, size: f64) -> Line {
        Line {
            text: text.into(),
            x0,
            x1,
            y,
            size,
            bold: false,
            raw: false,
            mcid: None,
            artifact: false,
            hint: None,
            group: None,
            keep: false,
            turned: false,
        }
    }

    #[test]
    fn spanning_abstract_above_two_columns_segments_cleanly() {
        let mut frags = Vec::new();
        // Full-width abstract split into two tightly spaced fragments per line (like justified text)
        for i in 0..5 {
            let y = 700.0 - 12.0 * i as f64;
            frags.push(line(
                "Abstract text spanning the first half",
                50.0,
                290.0,
                y,
                10.0,
            ));
            frags.push(line(
                "and continuing across the center line.",
                294.0,
                550.0,
                y,
                10.0,
            ));
        }
        // Two-column body below
        for i in 0..8 {
            let y = 600.0 - 12.0 * i as f64;
            frags.push(line(
                &format!("Left column body line {i}"),
                50.0,
                280.0,
                y,
                10.0,
            ));
            frags.push(line(
                &format!("Right column body line {i}"),
                315.0,
                550.0,
                y,
                10.0,
            ));
        }
        let bs = blocks(frags);
        assert_eq!(bs.len(), 3, "abstract + left col + right col: {bs:?}");
        assert!(bs[0][0].text.starts_with("Abstract text"));
        assert!(bs[1].iter().all(|l| l.text.starts_with("Left column")));
        assert!(bs[2].iter().all(|l| l.text.starts_with("Right column")));
    }

    #[test]
    fn equation_numbers_on_right_do_not_split_single_column() {
        let mut frags = Vec::new();
        for i in 0..6 {
            let y = 600.0 - 18.0 * i as f64;
            frags.push(line("x + y = z formula text here", 80.0, 300.0, y, 10.0));
            frags.push(line(&format!("({})", i + 1), 500.0, 520.0, y, 10.0));
        }
        let bs = blocks(frags);
        assert_eq!(bs.len(), 1, "must stay single column");
    }

    #[test]
    fn single_column_with_justified_gaps_stays_one_block() {
        let mut frags = Vec::new();
        for i in 0..8 {
            let y = 700.0 - 12.0 * i as f64;
            if i % 2 == 0 {
                frags.push(line(
                    "Full width bibliography entry line here",
                    100.0,
                    520.0,
                    y,
                    10.0,
                ));
            } else {
                // Stretched justified line with >7pt internal gap
                frags.push(line("Cerne, v.", 100.0, 260.0, y, 10.0));
                frags.push(line("3, n. 1, 1997.", 275.0, 520.0, y, 10.0));
            }
        }
        let bs = blocks(frags);
        assert_eq!(bs.len(), 1, "single column must not shatter: {bs:?}");
    }

    #[test]
    fn two_columns_above_three_columns_both_split_cleanly() {
        let mut frags = Vec::new();
        // Top half: 2 columns at [57..311] and [324..578]
        for i in 0..10 {
            let y = 700.0 - 11.0 * i as f64;
            frags.push(line(&format!("TopLeft line {i}"), 57.0, 311.0, y, 8.8));
            frags.push(line(&format!("TopRight line {i}"), 324.0, 578.0, y, 8.8));
        }
        // Bottom half (after large Y gap): 3 columns at [57..221], [235..400], [413..578]
        for i in 0..14 {
            let y = 400.0 - 8.5 * i as f64;
            frags.push(line(&format!("BotCol1 line {i}"), 57.0, 221.0, y, 6.8));
            frags.push(line(&format!("BotCol2 line {i}"), 235.0, 400.0, y, 6.8));
            frags.push(line(&format!("BotCol3 line {i}"), 413.0, 578.0, y, 6.8));
        }
        let bs = blocks(frags);
        assert_eq!(bs.len(), 5, "2 top cols + 3 bottom cols: {bs:?}");
        assert!(bs[0].iter().all(|l| l.text.starts_with("TopLeft")));
        assert!(bs[1].iter().all(|l| l.text.starts_with("TopRight")));
        assert!(bs[2].iter().all(|l| l.text.starts_with("BotCol1")));
        assert!(bs[3].iter().all(|l| l.text.starts_with("BotCol2")));
        assert!(bs[4].iter().all(|l| l.text.starts_with("BotCol3")));
    }

    #[test]
    fn tiny_text_narrow_gutter_and_margin_line_numbers() {
        // Tiny 5.5pt encyclopedia with 3.8pt gutter
        let mut frags = Vec::new();
        for i in 0..12 {
            let y = 550.0 - 7.0 * i as f64;
            frags.push(line(
                &format!("Left tiny text line {i}"),
                15.0,
                204.0,
                y,
                5.5,
            ));
            frags.push(line(
                &format!("Right tiny text line {i}"),
                208.0,
                397.0,
                y,
                5.5,
            ));
        }
        let bs = blocks(frags);
        assert_eq!(bs.len(), 2, "must split narrow tiny-text gutter: {bs:?}");

        // Two-column proof with left/right margin line numbers (387..396, 450..459)
        let mut lined = Vec::new();
        for i in 0..10 {
            let y = 600.0 - 11.0 * i as f64;
            lined.push(line(&format!("{}", 387 + i), 30.0, 45.0, y, 8.0));
            lined.push(line(
                &format!("Left body sentence line {i}"),
                60.0,
                280.0,
                y,
                9.0,
            ));
            lined.push(line(
                &format!("Right body sentence line {i}"),
                305.0,
                525.0,
                y,
                9.0,
            ));
            lined.push(line(&format!("{}", 450 + i), 540.0, 555.0, y, 8.0));
        }
        let lbs = blocks(lined);
        assert_eq!(
            lbs.len(),
            4,
            "margin line numbers must stay separate from both columns: {lbs:?}"
        );
        assert!(lbs[1].iter().all(|l| l.text.starts_with("Left body")));
        assert!(lbs[2].iter().all(|l| l.text.starts_with("Right body")));
    }
}
