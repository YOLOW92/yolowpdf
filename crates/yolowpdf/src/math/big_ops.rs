//! 2D 大型算子上下限重构引擎（`\sum`, `\prod`, `\int`, `\bigcup`, `\lim`, `\liminf`, `\limsup`, `\sup`, `\inf`, `\max`, `\min`）。
//!
//! 兼容 Knuth `cmex10.tfm` 悬挂基线与常规基线，通过空间桶检索正上方/正下方孤立小字号集群，
//! 并校验左右 2.5*body 邻域无全字号正文，避免误抓相邻正文行里的普通上下标。

use crate::font::MathKind;
use crate::geom::Glyph;
use crate::math::spatial::{YBuckets, looks_like_math_expr, looks_like_prose, render_subexpr};

/// 大型算子上下限（`\sum_{sub}^{sup}`、`\prod`、`\int`、`\lim_{sub}` 等）的 2D 聚合。
pub(crate) fn reconstruct_big_ops(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    body: f64,
) {
    let n_orig = glyphs.len();
    let mut qbuf: Vec<usize> = Vec::new();

    // A) 符号型大型算子（\sum, \prod, \int, \bigcup, ...）
    for i in 0..n_orig {
        if used[i] {
            continue;
        }
        let t = glyphs[i].text.trim();
        let mut cs = t.chars();
        let (Some(ch), None) = (cs.next(), cs.next()) else {
            continue;
        };
        let Some(cmd) = big_op_cmd(ch) else {
            continue;
        };
        let op = glyphs[i].clone();
        let op_x0 = op.x;
        let op_x1 = op.x_end.max(op.x + 0.6 * op.size);
        let op_sz = op.size.max(body);
        let is_cmex = op.math == MathKind::Extension;
        let is_int = matches!(ch, '∫' | '∬' | '∮');

        // Knuth cmex10.tfm：大型算子基线在字形顶部（向下延伸约 1.2–1.6*size）；非 CMEX 基线在底部
        let (top_y, bot_y, eq_y) = if is_cmex {
            (
                op.y + 0.20 * op_sz,
                op.y - 1.30 * op_sz,
                op.y - 0.45 * op_sz,
            )
        } else {
            (op.y + 0.85 * op_sz, op.y - 0.20 * op_sz, op.y)
        };
        let x_left = if is_int {
            op_x0 + 0.10 * op_sz
        } else {
            op_x0 - 0.85 * op_sz
        };
        let x_right = if is_int {
            op_x1 + 1.45 * op_sz
        } else {
            op_x1 + 0.90 * op_sz
        };

        let upper = collect_limit_cluster(
            glyphs,
            used,
            buckets,
            &[i],
            x_left,
            x_right,
            top_y - 0.15 * op_sz,
            top_y + if is_int { 0.90 } else { 0.78 } * op_sz,
            op_sz,
            true,
            &mut qbuf,
        );
        let lower = collect_limit_cluster(
            glyphs,
            used,
            buckets,
            &[i],
            x_left,
            x_right,
            bot_y - 0.92 * op_sz,
            bot_y + 0.10 * op_sz,
            op_sz,
            false,
            &mut qbuf,
        );
        if upper.is_empty() && lower.is_empty() {
            continue;
        }
        let mut tex = String::from(cmd);
        let mut x0 = op_x0;
        let mut x1 = op_x1;
        if !lower.is_empty() {
            let (lt, _, _) = render_subexpr(glyphs, &lower, body * 0.7);
            if !lt.is_empty() {
                tex.push_str("_{");
                tex.push_str(&lt);
                tex.push('}');
                for &k in &lower {
                    x0 = x0.min(glyphs[k].x);
                    x1 = x1.max(glyphs[k].x_end.max(glyphs[k].x));
                    used[k] = true;
                }
            }
        }
        if !upper.is_empty() {
            let (ut, _, _) = render_subexpr(glyphs, &upper, body * 0.7);
            if !ut.is_empty() {
                tex.push_str("^{");
                tex.push_str(&ut);
                tex.push('}');
                for &k in &upper {
                    x0 = x0.min(glyphs[k].x);
                    x1 = x1.max(glyphs[k].x_end.max(glyphs[k].x));
                    used[k] = true;
                }
            }
        }
        if tex == cmd {
            continue;
        }
        used[i] = true;
        // 寻找紧邻右侧同方程的基线，若找到则以它为等式基线（精确对齐同基线主表达式）
        let aligned_y = nearby_equation_baseline(glyphs, used, buckets, x1, eq_y, body, &mut qbuf)
            .unwrap_or(eq_y);
        let new_idx = glyphs.len();
        glyphs.push(Glyph {
            text: tex.into(),
            x: x0,
            y: aligned_y,
            x_end: x1,
            size: op_sz,
            bold: false,
            math: MathKind::Prebuilt,
            mcid: op.mcid,
            artifact: op.artifact,
        });
        used.push(false);
        buckets.push(new_idx, aligned_y);
    }

    // B) 文本型极限算子（\lim, \liminf, \limsup, \sup, \inf, \max, \min）的正下方下标
    reconstruct_text_limit_ops(glyphs, used, buckets, body, &mut qbuf);
}

fn big_op_cmd(c: char) -> Option<&'static str> {
    match c {
        '∑' => Some("\\sum"),
        '∏' => Some("\\prod"),
        '∐' => Some("\\coprod"),
        '∫' => Some("\\int"),
        '∬' => Some("\\iint"),
        '∮' => Some("\\oint"),
        '⋃' => Some("\\bigcup"),
        '⋂' => Some("\\bigcap"),
        '⋀' => Some("\\bigwedge"),
        '⋁' => Some("\\bigvee"),
        '⨁' => Some("\\bigoplus"),
        '⨂' => Some("\\bigotimes"),
        '⨀' => Some("\\bigodot"),
        '⨄' => Some("\\biguplus"),
        '⨆' => Some("\\bigsqcup"),
        _ => None,
    }
}

/// 在 `[x_left, x_right] × [y_lo, y_hi]` 内找小字号上下限种子字形，并沿同基线左右连通扩展。
#[allow(clippy::too_many_arguments)]
fn collect_limit_cluster(
    glyphs: &[Glyph],
    used: &[bool],
    buckets: &YBuckets,
    op_ids: &[usize],
    x_left: f64,
    x_right: f64,
    y_lo: f64,
    y_hi: f64,
    body: f64,
    is_upper: bool,
    qbuf: &mut Vec<usize>,
) -> Vec<usize> {
    buckets.query(y_lo, y_hi, qbuf);
    let mut seeds: Vec<usize> = qbuf
        .iter()
        .copied()
        .filter(|&k| {
            if used[k] || op_ids.contains(&k) {
                return false;
            }
            let g = &glyphs[k];
            if g.text.trim().is_empty() || g.size > 0.88 * body {
                return false;
            }
            // 不抓大算子本身或孤立根号符号
            if matches!(g.text.trim(), "√" | "∑" | "∏" | "∫") {
                return false;
            }
            let cx = 0.5 * (g.x + g.x_end.max(g.x));
            g.y >= y_lo && g.y <= y_hi && cx >= x_left && cx <= x_right
        })
        .collect();
    if seeds.is_empty() {
        return Vec::new();
    }
    // 选离算子最近的那个基线层作为极限主基线
    if is_upper {
        seeds.sort_by(|&a, &b| glyphs[a].y.total_cmp(&glyphs[b].y));
    } else {
        seeds.sort_by(|&a, &b| glyphs[b].y.total_cmp(&glyphs[a].y));
    }
    let lim_y = glyphs[seeds[0]].y;
    seeds.retain(|&k| (glyphs[k].y - lim_y).abs() <= 0.45 * body);
    if seeds.is_empty() {
        return Vec::new();
    }

    // 沿同基线左右连通扩展（例如 `1 \le k \le N` 比 `\sum` 略宽），但遇到明显空隙即停（防止串到邻近算子）
    buckets.query(lim_y - 0.40 * body, lim_y + 0.40 * body, qbuf);
    let mut row: Vec<usize> = qbuf
        .iter()
        .copied()
        .filter(|&k| {
            !used[k]
                && !op_ids.contains(&k)
                && !glyphs[k].text.trim().is_empty()
                && glyphs[k].size <= 0.88 * body
                && (glyphs[k].y - lim_y).abs() <= 0.38 * body
        })
        .collect();
    row.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
    let mut cluster = seeds;
    let max_gap = 0.65 * body;
    let mut changed = true;
    while changed {
        changed = false;
        let (mut cx0, mut cx1) = (f64::INFINITY, f64::NEG_INFINITY);
        for &k in &cluster {
            cx0 = cx0.min(glyphs[k].x);
            cx1 = cx1.max(glyphs[k].x_end.max(glyphs[k].x));
        }
        for &k in &row {
            if cluster.contains(&k) {
                continue;
            }
            let g = &glyphs[k];
            let gx1 = g.x_end.max(g.x);
            if (g.x <= cx1 + max_gap && gx1 >= cx0 - max_gap) && (cx1 - cx0) < 12.0 * body {
                cluster.push(k);
                changed = true;
            }
        }
    }
    if looks_like_prose(glyphs, &cluster, body) {
        return Vec::new();
    }
    // 关键几何不变量：真行间算子的上下限处于算子上下方的独立空白带内，同一基线左右 0.9*body 内
    // 不应出现正常字号（>= 0.90*body）的普通正文 Text 字形（防止误抓相邻正文行里的普通上下标，
    // 同时允许右侧紧邻 `\frac{a}{b}` 等数学子式与上下限处于相近 y 高度）
    let (mut lx0, mut lx1) = (f64::INFINITY, f64::NEG_INFINITY);
    for &k in &cluster {
        lx0 = lx0.min(glyphs[k].x);
        lx1 = lx1.max(glyphs[k].x_end.max(glyphs[k].x));
    }
    if (lx1 - lx0) > (x_right - x_left).max(body * 2.0) * 3.5 + 2.0 * body {
        return Vec::new();
    }
    buckets.query(lim_y - 0.35 * body, lim_y + 0.35 * body, qbuf);
    let has_neighbor_fullsize = qbuf.iter().any(|&k| {
        if used[k] || op_ids.contains(&k) || cluster.contains(&k) {
            return false;
        }
        let g = &glyphs[k];
        if g.math != MathKind::Text || g.text.trim().is_empty() || g.size < 0.92 * body {
            return false;
        }
        let gx1 = g.x_end.max(g.x);
        (g.y - lim_y).abs() <= 0.35 * body && gx1 >= lx0 - 0.90 * body && g.x <= lx1 + 0.90 * body
    });
    if has_neighbor_fullsize {
        return Vec::new();
    }
    cluster
}

fn nearby_equation_baseline(
    glyphs: &[Glyph],
    used: &[bool],
    buckets: &YBuckets,
    op_x1: f64,
    approx_y: f64,
    body: f64,
    qbuf: &mut Vec<usize>,
) -> Option<f64> {
    buckets.query(approx_y - 0.60 * body, approx_y + 0.60 * body, qbuf);
    let mut best: Option<(f64, f64)> = None;
    for &k in qbuf.iter() {
        if used[k] {
            continue;
        }
        let g = &glyphs[k];
        let t = g.text.trim();
        if t.is_empty() || g.size < 0.85 * body {
            continue;
        }
        // 只对齐到右侧紧邻的数学表达式字形（禁止对齐到相邻正文行里的英文词如 `weighted`）
        let is_math_anchor = g.math != MathKind::Text
            || t.chars()
                .all(|c| c.is_ascii_digit() || "+-=<>()[]|".contains(c));
        if !is_math_anchor {
            continue;
        }
        let dx = g.x - op_x1;
        if dx >= -0.25 * body
            && dx <= 3.0 * body
            && (g.y - approx_y).abs() <= 0.60 * body
            && best.is_none_or(|(d, _)| dx.abs() < d)
        {
            best = Some((dx.abs(), g.y));
        }
    }
    best.map(|(_, y)| y)
}

/// 文本型极限算子（`lim`, `lim inf`, `lim sup`, `sup`, `inf`, `max`, `min`）聚合其正下方下标。
fn reconstruct_text_limit_ops(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    body: f64,
    qbuf: &mut Vec<usize>,
) {
    let mut text_ids: Vec<usize> = (0..glyphs.len())
        .filter(|&i| {
            !used[i]
                && glyphs[i].math == MathKind::Text
                && glyphs[i].size >= 0.80 * body
                && !glyphs[i].text.trim().is_empty()
                && glyphs[i]
                    .text
                    .trim()
                    .chars()
                    .all(|c| c.is_ascii_alphabetic())
        })
        .collect();
    text_ids.sort_by(|&a, &b| {
        glyphs[b]
            .y
            .total_cmp(&glyphs[a].y)
            .then(glyphs[a].x.total_cmp(&glyphs[b].x))
    });

    let mut words: Vec<(String, Vec<usize>, f64, f64, f64, f64)> = Vec::new();
    let mut i = 0;
    while i < text_ids.len() {
        let mut ids = vec![text_ids[i]];
        let g0 = &glyphs[text_ids[i]];
        let mut s = g0.text.trim().to_string();
        let (x0, mut x1, y, sz) = (g0.x, g0.x_end.max(g0.x), g0.y, g0.size);
        let mut j = i + 1;
        while j < text_ids.len() {
            let gj = &glyphs[text_ids[j]];
            if (gj.y - y).abs() <= 0.15 * sz && gj.x - x1 <= 0.18 * sz && gj.x >= x1 - 0.15 * sz {
                s.push_str(gj.text.trim());
                x1 = x1.max(gj.x_end.max(gj.x));
                ids.push(text_ids[j]);
                j += 1;
            } else {
                break;
            }
        }
        words.push((s, ids, x0, x1, y, sz));
        i = j;
    }

    let mut w = 0;
    while w < words.len() {
        let (ref name, ref ids, x0, x1, y, sz) = words[w];
        let (cmd, op_ids, ox0, ox1, step) = if name == "lim"
            && let Some((next_name, next_ids, nx0, nx1, ny, _)) = words.get(w + 1)
            && (ny - y).abs() <= 0.15 * sz
            && *nx0 - x1 <= 0.65 * sz
            && matches!(next_name.as_str(), "inf" | "sup")
        {
            let mut all = ids.clone();
            all.extend_from_slice(next_ids);
            let cmd = if next_name == "inf" {
                "\\liminf"
            } else {
                "\\limsup"
            };
            (cmd, all, x0, *nx1, 2)
        } else {
            let cmd = match name.as_str() {
                "lim" => "\\lim",
                "liminf" => "\\liminf",
                "limsup" => "\\limsup",
                "sup" => "\\sup",
                "inf" => "\\inf",
                "max" => "\\max",
                "min" => "\\min",
                _ => {
                    w += 1;
                    continue;
                }
            };
            (cmd, ids.clone(), x0, x1, 1)
        };

        if op_ids.iter().any(|&k| used[k]) {
            w += step;
            continue;
        }
        let lower = collect_limit_cluster(
            glyphs,
            used,
            buckets,
            &op_ids,
            ox0 - 0.55 * body,
            ox1 + 0.55 * body,
            y - 1.35 * body,
            y - 0.22 * body,
            body,
            false,
            qbuf,
        );
        if lower.is_empty() || !looks_like_math_expr(glyphs, &lower) {
            w += step;
            continue;
        }
        let (lt, _, _) = render_subexpr(glyphs, &lower, body * 0.7);
        if lt.is_empty() {
            w += step;
            continue;
        }
        let mcid = glyphs[op_ids[0]].mcid;
        let artifact = glyphs[op_ids[0]].artifact;
        let mut bx0 = ox0;
        let mut bx1 = ox1;
        for &k in &op_ids {
            used[k] = true;
        }
        for &k in &lower {
            bx0 = bx0.min(glyphs[k].x);
            bx1 = bx1.max(glyphs[k].x_end.max(glyphs[k].x));
            used[k] = true;
        }
        let tex = format!("{cmd}_{{{lt}}}");
        let new_idx = glyphs.len();
        glyphs.push(Glyph {
            text: tex.into(),
            x: bx0,
            y,
            x_end: bx1,
            size: sz,
            bold: false,
            math: MathKind::Prebuilt,
            mcid,
            artifact,
        });
        used.push(false);
        buckets.push(new_idx, y);
        w += step;
    }
}
