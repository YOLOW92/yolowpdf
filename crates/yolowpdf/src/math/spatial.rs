//! 2D 几何数学结构重构引擎（参考 Anderson 1968、Zanibbi DRACULAE 2002、Suzuki INFTY 2003）。
//!
//! 在逐行切分之前，直接在页面二维坐标 `(Glyph, Rule)` 上自底向上重构：
//! 1. 根号 `\sqrt[n]{...}`（`\surd` 字形 + 顶横线 vinculum）；
//! 2. 分式 `\frac{...}{...}`（分数线横线 + 上下分子分母孤立集群，按线宽升序支持任意嵌套）；
//! 3. 上划线 `\overline{...}`（数学符号正上方的短横线）；
//! 4. 大型算子上下限 `\sum_{...}^{...}`、`\prod`、`\int`、`\lim_{...}`、`\liminf_{...}` 等
//!    （兼容 TeX `cmex10.tfm` 悬挂基线与常规基线）。
//!
//! 重构出的 2D 结构折叠为单个 `MathKind::Prebuilt` 合成字形放回等式基线，同时移除已被公式消费的横线，
//! 使后续行聚合、分栏与表格检测零侵入兼容。

use crate::font::MathKind;
use crate::geom::{Glyph, Rule};
use crate::math::symbols::{is_func_or_op, is_greek, is_math_symbol};
use crate::math::tex::{Item, tex_of};

/// 单页参与 2D 数学分析的候选横线数上限（防止复杂矢量图带来额外开销）。
const MAX_MATH_RULES: usize = 2000;
/// 空间桶高度（pt）。
const BUCKET_H: f64 = 8.0;

/// 在 `layout::fragments` 之前重构页面上的 2D 数学结构。
pub(crate) fn reconstruct(
    glyphs: &mut Vec<Glyph>,
    rules: &mut Vec<Rule>,
    page_width: f64,
    page_height: f64,
) {
    if glyphs.len() < 2 || !has_math_evidence(glyphs) {
        return;
    }
    let body = median_body_size(glyphs);
    if !(2.0..=72.0).contains(&body) {
        return;
    }

    let mut used = vec![false; glyphs.len()];
    let mut rule_used = vec![false; rules.len()];
    let mut buckets = YBuckets::new(glyphs, page_height);

    // Pass 1：按横线宽度升序，自底向上重构根号、分式、上划线（自然支持嵌套分式与分式内根号）
    if !rules.is_empty() {
        reconstruct_vinculums(
            glyphs,
            rules,
            &mut used,
            &mut rule_used,
            &mut buckets,
            body,
            page_width,
        );
    }

    // Pass 2：大型算子（\sum/\prod/\int/\bigcup/...）与极限算子（\lim/\liminf/\limsup/\sup/\inf/\max/\min）上下限
    super::big_ops::reconstruct_big_ops(glyphs, &mut used, &mut buckets, body);

    // 清理被 2D 结构消费掉的字形和公式横线
    if used.iter().any(|&u| u) {
        let mut it = used.into_iter();
        glyphs.retain(|_| !it.next().unwrap_or(false));
    }
    if rule_used.iter().any(|&u| u) {
        let mut it = rule_used.into_iter();
        rules.retain(|_| !it.next().unwrap_or(false));
    }
}

fn has_math_evidence(glyphs: &[Glyph]) -> bool {
    glyphs.iter().any(|g| {
        g.math != MathKind::Text
            || g.text
                .chars()
                .any(|c| is_math_symbol(c) || is_greek(c) || c == '√')
    })
}

fn median_body_size(glyphs: &[Glyph]) -> f64 {
    let mut sizes: Vec<f64> = glyphs
        .iter()
        .filter(|g| !g.text.trim().is_empty() && g.size.is_finite() && g.size > 0.5)
        .map(|g| g.size)
        .collect();
    if sizes.is_empty() {
        return 10.0;
    }
    sizes.sort_by(f64::total_cmp);
    sizes[sizes.len() / 2].clamp(4.0, 36.0)
}

/// 简单的按 y 坐标分桶索引：把 [y_lo, y_hi] 范围查询降到 O(1) 桶访问。
pub(crate) struct YBuckets {
    bins: Vec<Vec<usize>>,
    offset: i32,
}

impl YBuckets {
    fn new(glyphs: &[Glyph], page_height: f64) -> Self {
        let max_bin = ((page_height.max(100.0) / BUCKET_H).ceil() as usize).min(2048) + 8;
        let offset = 4i32;
        let mut b = YBuckets {
            bins: vec![Vec::new(); max_bin + offset as usize + 1],
            offset,
        };
        for (i, g) in glyphs.iter().enumerate() {
            b.push(i, g.y);
        }
        b
    }

    #[inline]
    fn slot(&self, y: f64) -> usize {
        if !y.is_finite() {
            return 0;
        }
        let k = ((y / BUCKET_H).floor() as i32).saturating_add(self.offset);
        k.clamp(0, self.bins.len() as i32 - 1) as usize
    }

    pub(crate) fn push(&mut self, idx: usize, y: f64) {
        if !y.is_finite() {
            return;
        }
        let s = self.slot(y);
        self.bins[s].push(idx);
    }

    pub(crate) fn query(&self, y_lo: f64, y_hi: f64, out: &mut Vec<usize>) {
        out.clear();
        if !y_lo.is_finite() || !y_hi.is_finite() || y_lo > y_hi {
            return;
        }
        let a = self.slot(y_lo);
        let b = self.slot(y_hi);
        for bin in &self.bins[a..=b] {
            out.extend_from_slice(bin);
        }
    }
}

/// 把横线按宽度升序排列，排除属于表格/图框（端点与竖线相交）的横线，依次尝试：
/// 1) 根号顶线 `\sqrt`；2) 分数线 `\frac`；3) 短上划线 `\overline`。
fn reconstruct_vinculums(
    glyphs: &mut Vec<Glyph>,
    rules: &[Rule],
    used: &mut Vec<bool>,
    rule_used: &mut [bool],
    buckets: &mut YBuckets,
    body: f64,
    page_width: f64,
) {
    // 收集竖线（用于排除表格网格与方框边框），按 x 排序以便二分查找
    let mut vrules: Vec<(f64, f64, f64)> = rules
        .iter()
        .filter(|r| !r.horizontal() && (r.y1 - r.y0).abs() >= 3.0)
        .map(|r| (r.x0, r.y0.min(r.y1), r.y0.max(r.y1)))
        .collect();
    vrules.sort_by(|a, b| a.0.total_cmp(&b.0));

    let touches_vrule = |x: f64, y: f64| -> bool {
        let lo = vrules.partition_point(|v| v.0 < x - 3.0);
        for &(vx, vy0, vy1) in &vrules[lo..] {
            if vx > x + 3.0 {
                break;
            }
            if y >= vy0 - 3.0 && y <= vy1 + 3.0 {
                return true;
            }
        }
        false
    };

    // 识别表格横线（绝不当分数线消费）：
    // 1) 三线表（Booktabs）成组对齐的长横线：同一 [x0, x1]（±6pt）出现 >= 2 条且宽度 >= 0.18*page_width；
    // 2) 分段画出的共线表格横线：同一 y（±2pt）上出现 >= 2 条紧邻横线（间距 <= 24pt）且总跨度 >= 0.20*page_width，
    //    或同一 y 上出现 >= 3 条分段横线（表头/表底逐列划线）。
    let h_all: Vec<(f64, f64, f64)> = rules
        .iter()
        .filter(|r| r.horizontal())
        .map(|r| ((r.y0 + r.y1) * 0.5, r.x0, r.x1))
        .collect();
    let is_table_rule = |r: &Rule| -> bool {
        let w = r.x1 - r.x0;
        let ry = (r.y0 + r.y1) * 0.5;
        if w >= 0.18 * page_width
            && h_all
                .iter()
                .filter(|&&(_, x0, x1)| {
                    (x1 - x0) >= 0.18 * page_width
                        && (x0 - r.x0).abs() < 6.0
                        && (x1 - r.x1).abs() < 6.0
                })
                .count()
                >= 2
        {
            return true;
        }
        // 共线分段表格横线检测：相邻段间距 <= 3pt（与 table::lattice::JOIN 一致，避免误伤并排分式 `\frac{a}{b}+\frac{c}{d}`）
        let mut same_y: Vec<(f64, f64)> = h_all
            .iter()
            .filter(|&&(y, _, _)| (y - ry).abs() <= 2.0)
            .map(|&(_, x0, x1)| (x0, x1))
            .collect();
        if same_y.len() >= 2 {
            same_y.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut seg_x0 = same_y[0].0;
            let mut seg_x1 = same_y[0].1;
            let mut seg_cnt = 1usize;
            for &(x0, x1) in &same_y[1..] {
                if x0 <= seg_x1 + 3.0 {
                    seg_x1 = seg_x1.max(x1);
                    seg_cnt += 1;
                } else {
                    if seg_cnt >= 2
                        && r.x0 >= seg_x0 - 1.0
                        && r.x1 <= seg_x1 + 1.0
                        && seg_x1 - seg_x0 >= 0.22 * page_width
                    {
                        return true;
                    }
                    seg_x0 = x0;
                    seg_x1 = x1;
                    seg_cnt = 1;
                }
            }
            if seg_cnt >= 2
                && r.x0 >= seg_x0 - 1.0
                && r.x1 <= seg_x1 + 1.0
                && seg_x1 - seg_x0 >= 0.22 * page_width
            {
                return true;
            }
        }
        false
    };

    let mut cands: Vec<(usize, f64)> = rules
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            if !r.horizontal() {
                return None;
            }
            let w = r.x1 - r.x0;
            if w < 2.0 || w > 0.72 * page_width {
                return None;
            }
            let y = (r.y0 + r.y1) * 0.5;
            if touches_vrule(r.x0, y) || touches_vrule(r.x1, y) || is_table_rule(r) {
                return None;
            }
            Some((i, w))
        })
        .collect();
    if cands.is_empty() {
        return;
    }
    cands.sort_by(|a, b| a.1.total_cmp(&b.1));
    if cands.len() > MAX_MATH_RULES {
        cands.truncate(MAX_MATH_RULES);
    }

    let mut qbuf: Vec<usize> = Vec::new();
    for (ri, w) in cands {
        let r = rules[ri];
        let y_bar = (r.y0 + r.y1) * 0.5;

        // 1. 尝试根号 `\sqrt` / `\sqrt[n]`
        if try_radical(glyphs, used, buckets, r, y_bar, body, &mut qbuf) {
            rule_used[ri] = true;
            continue;
        }

        // 2. 尝试分式 `\frac{num}{den}`
        if try_fraction(
            glyphs, used, buckets, r, y_bar, w, body, page_width, &mut qbuf,
        ) {
            rule_used[ri] = true;
            continue;
        }

        // 3. 尝试短上划线 `\overline{...}`
        if w <= 14.0 * body && try_overline(glyphs, used, buckets, r, y_bar, body, &mut qbuf) {
            rule_used[ri] = true;
        }
    }
}

fn try_radical(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    r: Rule,
    y_bar: f64,
    body: f64,
    qbuf: &mut Vec<usize>,
) -> bool {
    buckets.query(y_bar - 3.2 * body, y_bar + 0.6 * body, qbuf);
    // 找紧邻顶线左端的 `√` 字形
    let mut best_surd: Option<(usize, f64)> = None;
    for &i in qbuf.iter() {
        if used[i] {
            continue;
        }
        let g = &glyphs[i];
        if g.text.trim() != "√" {
            continue;
        }
        let dx = (g.x_end - r.x0).abs().min((g.x - r.x0).abs());
        let dy = y_bar - g.y;
        let is_ext = g.math == MathKind::Extension;
        let min_dy = if is_ext { -0.25 * body } else { 0.25 * body };
        let max_h = (3.0 * g.size.max(body)).max(24.0);
        if g.x <= r.x0 + 0.4 * body
            && g.x_end >= r.x0 - 0.8 * body
            && dx <= 0.85 * g.size.max(body)
            && dy >= min_dy
            && dy <= max_h
            && best_surd.is_none_or(|(_, d)| dx < d)
        {
            best_surd = Some((i, dx));
        }
    }
    let Some((si, _)) = best_surd else {
        return false;
    };
    let surd = glyphs[si].clone();
    let rad_bot = if surd.math == MathKind::Extension {
        surd.y - 2.4 * surd.size.max(body)
    } else {
        surd.y - 0.65 * surd.size.max(body)
    };

    // 收集顶线正下方的被开方数字形
    buckets.query(rad_bot, y_bar - 0.02 * body, qbuf);
    let mut rad: Vec<usize> = Vec::new();
    for &i in qbuf.iter() {
        if used[i] || i == si {
            continue;
        }
        let g = &glyphs[i];
        if g.text.trim().is_empty() {
            continue;
        }
        let cx = 0.5 * (g.x + g.x_end.max(g.x));
        if cx >= r.x0 - 0.25 * body
            && cx <= r.x1 + 0.35 * body
            && g.y >= rad_bot
            && g.y < y_bar - 0.02 * body
        {
            rad.push(i);
        }
    }
    if rad.is_empty() {
        return false;
    }

    // 可选根次（如三次根号 `\sqrt[3]{x}`）：位于 `√` 左上方的小字号字形
    buckets.query(surd.y + 0.30 * body, y_bar + 0.35 * body, qbuf);
    let mut root_idx: Vec<usize> = Vec::new();
    let surd_w = (surd.x_end - surd.x).max(0.4 * body);
    for &i in qbuf.iter() {
        if used[i] || i == si || rad.contains(&i) {
            continue;
        }
        let g = &glyphs[i];
        if g.text.trim().is_empty() || g.size > 0.82 * body {
            continue;
        }
        let cx = 0.5 * (g.x + g.x_end.max(g.x));
        if cx >= surd.x - 0.6 * body
            && cx <= surd.x + 0.55 * surd_w
            && g.y >= surd.y + 0.30 * body
            && g.y <= y_bar + 0.30 * body
        {
            root_idx.push(i);
        }
    }

    let (rad_tex, base_y, base_size) = render_subexpr(glyphs, &rad, body);
    if rad_tex.is_empty() {
        return false;
    }
    let tex = if root_idx.is_empty() {
        format!("\\sqrt{{{rad_tex}}}")
    } else {
        let (idx_tex, _, _) = render_subexpr(glyphs, &root_idx, body * 0.7);
        if idx_tex.is_empty() {
            format!("\\sqrt{{{rad_tex}}}")
        } else {
            format!("\\sqrt[{idx_tex}]{{{rad_tex}}}")
        }
    };

    used[si] = true;
    for &i in &rad {
        used[i] = true;
    }
    for &i in &root_idx {
        used[i] = true;
    }
    let x0 = root_idx.iter().map(|&i| glyphs[i].x).fold(surd.x, f64::min);
    let new_idx = glyphs.len();
    glyphs.push(Glyph {
        text: tex.into(),
        x: x0,
        y: base_y,
        x_end: r.x1.max(x0 + 0.5 * body),
        size: base_size,
        bold: false,
        math: MathKind::Prebuilt,
        mcid: surd.mcid,
        artifact: surd.artifact,
    });
    used.push(false);
    buckets.push(new_idx, base_y);
    true
}

#[allow(clippy::too_many_arguments)]
fn try_fraction(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    r: Rule,
    y_bar: f64,
    w: f64,
    body: f64,
    page_width: f64,
    qbuf: &mut Vec<usize>,
) -> bool {
    let x_pad = 0.40 * body;
    // 在分数线上下 2.2*body 范围内先取候选，再隔离紧贴分数线的最近一层
    buckets.query(y_bar + 0.03 * body, y_bar + 2.3 * body, qbuf);
    let mut above_raw: Vec<usize> = qbuf
        .iter()
        .copied()
        .filter(|&i| {
            if used[i] {
                return false;
            }
            let g = &glyphs[i];
            if g.text.trim().is_empty() {
                return false;
            }
            let x1 = g.x_end.max(g.x);
            let cx = 0.5 * (g.x + x1);
            g.y > y_bar + 0.03 * body
                && g.y <= y_bar + 2.3 * body
                && cx >= r.x0 - x_pad
                && cx <= r.x1 + x_pad
        })
        .collect();
    if above_raw.is_empty() {
        return false;
    }

    buckets.query(y_bar - 2.3 * body, y_bar - 0.03 * body, qbuf);
    let mut below_raw: Vec<usize> = qbuf
        .iter()
        .copied()
        .filter(|&i| {
            if used[i] {
                return false;
            }
            let g = &glyphs[i];
            if g.text.trim().is_empty() {
                return false;
            }
            let x1 = g.x_end.max(g.x);
            let cx = 0.5 * (g.x + x1);
            g.y < y_bar - 0.03 * body
                && g.y >= y_bar - 2.3 * body
                && cx >= r.x0 - x_pad
                && cx <= r.x1 + x_pad
        })
        .collect();
    if below_raw.is_empty() {
        return false;
    }

    // 隔离紧贴分数线的单层分子（按 y 升序从下往上聚，遇到明显垂直空隙即停）
    above_raw.sort_by(|&a, &b| glyphs[a].y.total_cmp(&glyphs[b].y));
    let near_above = glyphs[above_raw[0]].y;
    if near_above - y_bar > 1.30 * body {
        return false;
    }
    let mut num = Vec::new();
    let mut top_y = near_above;
    for &i in &above_raw {
        let gy = glyphs[i].y;
        if gy - top_y > 0.62 * body || gy - near_above > 1.65 * body {
            break;
        }
        top_y = top_y.max(gy);
        num.push(i);
    }

    // 隔离紧贴分数线的单层分母（按 y 降序从上往下聚，遇到明显垂直空隙即停）
    below_raw.sort_by(|&a, &b| glyphs[b].y.total_cmp(&glyphs[a].y));
    let near_below = glyphs[below_raw[0]].y;
    if y_bar - near_below > 1.60 * body {
        return false;
    }
    let mut den = Vec::new();
    let mut bot_y = near_below;
    for &i in &below_raw {
        let gy = glyphs[i].y;
        if bot_y - gy > 0.62 * body || near_below - gy > 1.65 * body {
            break;
        }
        bot_y = bot_y.min(gy);
        den.push(i);
    }

    if num.is_empty() || den.is_empty() {
        return false;
    }

    // 边界与正文防误判校验：
    // 1) 分子/分母不能大幅伸出分数线两端
    let span = |ids: &[usize]| -> (f64, f64) {
        ids.iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &i| {
                (a.min(glyphs[i].x), b.max(glyphs[i].x_end.max(glyphs[i].x)))
            })
    };
    let (nx0, nx1) = span(&num);
    let (dx0, dx1) = span(&den);
    if nx0 < r.x0 - 0.85 * body
        || nx1 > r.x1 + 0.85 * body
        || dx0 < r.x0 - 0.85 * body
        || dx1 > r.x1 + 0.85 * body
    {
        return false;
    }

    // 2) 分子或分母不能只是某行连续正文穿过短横线的中间截断（左右紧贴同基线字形）
    if has_tight_horizontal_neighbor(glyphs, used, buckets, &num, r.x0, r.x1, body, qbuf)
        || has_tight_horizontal_neighbor(glyphs, used, buckets, &den, r.x0, r.x1, body, qbuf)
    {
        return false;
    }

    // 3) 不能是长正文句子或跨多列的表格行
    if looks_like_prose(glyphs, &num, body) || looks_like_prose(glyphs, &den, body) {
        return false;
    }
    if w > 0.18 * page_width
        && (has_wide_internal_gap(glyphs, &num, body) || has_wide_internal_gap(glyphs, &den, body))
    {
        return false;
    }
    if w > 0.35 * page_width && !group_has_math(glyphs, &num) && !group_has_math(glyphs, &den) {
        return false;
    }
    // 至少一侧有数学特征（数学字体/符号/希腊字母/预构建块/数字/单字母变量），
    // 且若分子分母均无真正 TeX 数学字体，拒绝首字母大写的表头词/普通英文词（如 `Element / O`, `Weight % / 56.53`）
    if !looks_like_math_expr(glyphs, &num) && !looks_like_math_expr(glyphs, &den) {
        return false;
    }
    let has_tex_font = |ids: &[usize]| {
        ids.iter()
            .any(|&i| !matches!(glyphs[i].math, MathKind::Text | MathKind::Symbol))
    };
    if !has_tex_font(&num)
        && !has_tex_font(&den)
        && (has_table_header_word(glyphs, &num, body) || has_table_header_word(glyphs, &den, body))
    {
        return false;
    }

    let (num_tex, _, num_sz) = render_subexpr(glyphs, &num, body);
    let (den_tex, _, den_sz) = render_subexpr(glyphs, &den, body);
    if num_tex.is_empty() || den_tex.is_empty() {
        return false;
    }

    let mcid = glyphs[num[0]].mcid;
    let artifact = glyphs[num[0]].artifact;
    for &i in &num {
        used[i] = true;
    }
    for &i in &den {
        used[i] = true;
    }

    // 根据 Knuth TeXbook Appendix G：分数线位于数学轴（基线上方约 0.25*size），反推等式基线
    let eq_size = num_sz.max(den_sz).max(body * 0.85).min(body * 1.25);
    let eq_y = y_bar - 0.25 * eq_size;
    let tex = format!("\\frac{{{num_tex}}}{{{den_tex}}}");
    let new_idx = glyphs.len();
    glyphs.push(Glyph {
        text: tex.into(),
        x: r.x0.min(nx0).min(dx0),
        y: eq_y,
        x_end: r.x1.max(nx1).max(dx1),
        size: eq_size,
        bold: false,
        math: MathKind::Prebuilt,
        mcid,
        artifact,
    });
    used.push(false);
    buckets.push(new_idx, eq_y);
    true
}

fn try_overline(
    glyphs: &mut Vec<Glyph>,
    used: &mut Vec<bool>,
    buckets: &mut YBuckets,
    r: Rule,
    y_bar: f64,
    body: f64,
    qbuf: &mut Vec<usize>,
) -> bool {
    let x_pad = 0.35 * body;
    buckets.query(y_bar - 1.20 * body, y_bar - 0.35 * body, qbuf);
    let mut under: Vec<usize> = qbuf
        .iter()
        .copied()
        .filter(|&i| {
            if used[i] {
                return false;
            }
            let g = &glyphs[i];
            if g.text.trim().is_empty() {
                return false;
            }
            let x1 = g.x_end.max(g.x);
            let cx = 0.5 * (g.x + x1);
            let dy = y_bar - g.y;
            dy >= 0.42 * g.size.max(body * 0.6)
                && dy <= 1.20 * g.size.max(body)
                && cx >= r.x0 - x_pad
                && cx <= r.x1 + x_pad
        })
        .collect();
    if under.is_empty() {
        return false;
    }
    // 只取同一基线层（离横线最近的数学符号）
    under.sort_by(|&a, &b| glyphs[b].y.total_cmp(&glyphs[a].y));
    let top_y = glyphs[under[0]].y;
    under.retain(|&i| (top_y - glyphs[i].y).abs() <= 0.45 * body);
    if !group_has_math(glyphs, &under) || looks_like_prose(glyphs, &under, body) {
        return false;
    }
    let (ux0, ux1) = under
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &i| {
            (a.min(glyphs[i].x), b.max(glyphs[i].x_end.max(glyphs[i].x)))
        });
    // 宽度要大致吻合（防止长下划线误作单个变量的 overline）
    if (r.x1 - r.x0) > (ux1 - ux0).max(body) * 1.9 + body * 0.5 {
        return false;
    }

    let (inner_tex, base_y, base_size) = render_subexpr(glyphs, &under, body);
    if inner_tex.is_empty() {
        return false;
    }
    let mcid = glyphs[under[0]].mcid;
    let artifact = glyphs[under[0]].artifact;
    for &i in &under {
        used[i] = true;
    }
    let tex = format!("\\overline{{{inner_tex}}}");
    let new_idx = glyphs.len();
    glyphs.push(Glyph {
        text: tex.into(),
        x: ux0.min(r.x0),
        y: base_y,
        x_end: ux1.max(r.x1),
        size: base_size,
        bold: false,
        math: MathKind::Prebuilt,
        mcid,
        artifact,
    });
    used.push(false);
    buckets.push(new_idx, base_y);
    true
}

/// 检查 `ids` 所在基线上是否紧邻分数线外的同基线字形（若紧贴，则横线只是普通行/表头下划线，非分式）。
#[allow(clippy::too_many_arguments)]
fn has_tight_horizontal_neighbor(
    glyphs: &[Glyph],
    used: &[bool],
    buckets: &YBuckets,
    ids: &[usize],
    rx0: f64,
    rx1: f64,
    body: f64,
    qbuf: &mut Vec<usize>,
) -> bool {
    let base_y = ids.iter().map(|&i| glyphs[i].y).sum::<f64>() / ids.len() as f64;
    let (mut gx0, mut gx1) = (f64::INFINITY, f64::NEG_INFINITY);
    for &i in ids {
        gx0 = gx0.min(glyphs[i].x);
        gx1 = gx1.max(glyphs[i].x_end.max(glyphs[i].x));
    }
    buckets.query(base_y - 0.25 * body, base_y + 0.25 * body, qbuf);
    for &i in qbuf.iter() {
        if used[i] || ids.contains(&i) {
            continue;
        }
        let g = &glyphs[i];
        if g.text.trim().is_empty() || (g.y - base_y).abs() > 0.22 * body {
            continue;
        }
        let x1 = g.x_end.max(g.x);
        // 左侧或右侧极近（小于一个普通字母空格宽）且超出横线外
        if (x1 <= gx0 + 0.1 * body && gx0 - x1 < 0.35 * body && g.x < rx0 - 0.5 * body)
            || (g.x >= gx1 - 0.1 * body && g.x - gx1 < 0.35 * body && x1 > rx1 + 0.5 * body)
        {
            return true;
        }
    }
    false
}

/// 检查字形集内部是否有大于 2.2*body 的水平空白（多列表格行，非分式）。
fn has_wide_internal_gap(glyphs: &[Glyph], ids: &[usize], body: f64) -> bool {
    if ids.len() < 2 {
        return false;
    }
    let mut sorted = ids.to_vec();
    sorted.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
    let mut prev_x1 = glyphs[sorted[0]].x_end.max(glyphs[sorted[0]].x);
    for &i in &sorted[1..] {
        let g = &glyphs[i];
        if g.x - prev_x1 > 2.2 * body {
            return true;
        }
        prev_x1 = prev_x1.max(g.x_end.max(g.x));
    }
    false
}

/// 检查无 TeX 数学字体的字形集是否含首字母大写的表头词（如 `Element`, `Total`, `Weight`, `Name`, `No`）。
fn has_table_header_word(glyphs: &[Glyph], ids: &[usize], body: f64) -> bool {
    let mut sorted = ids.to_vec();
    sorted.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
    let mut cur = String::new();
    let mut prev_x1 = f64::NEG_INFINITY;
    let is_hdr = |w: &str| {
        let mut cs = w.chars();
        let Some(first) = cs.next() else {
            return false;
        };
        let rest: Vec<char> = cs.collect();
        first.is_uppercase()
            && rest.len() >= 2
            && rest.iter().all(|c| c.is_lowercase())
            && !is_func_or_op(w)
    };
    for &i in &sorted {
        let g = &glyphs[i];
        let t = g.text.trim();
        if t.is_empty() {
            if is_hdr(&cur) {
                return true;
            }
            cur.clear();
            continue;
        }
        if prev_x1.is_finite() && g.x - prev_x1 > 0.18 * body {
            if is_hdr(&cur) {
                return true;
            }
            cur.clear();
        }
        cur.push_str(t);
        prev_x1 = prev_x1.max(g.x_end.max(g.x));
    }
    is_hdr(&cur)
}

/// 用 `tex::tex_of` 渲染一个局部 2D 数学子表达式（如分子、分母、被开方数、上下限）。
pub(crate) fn render_subexpr(
    glyphs: &[Glyph],
    ids: &[usize],
    fallback_size: f64,
) -> (String, f64, f64) {
    if ids.is_empty() {
        return (String::new(), 0.0, fallback_size);
    }
    let mut sorted = ids.to_vec();
    sorted.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
    // 确定该子表达式的主字号与主基线（取字符最多的最大字号档）
    let mut best_sz = 0.0f64;
    let mut best_y = glyphs[sorted[0]].y;
    let mut sizes: Vec<(i64, usize, f64, f64)> = Vec::new();
    for &i in &sorted {
        let g = &glyphs[i];
        let k = (g.size * 10.0).round() as i64;
        let n = g.text.chars().count().max(1);
        match sizes.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 += n,
            None => sizes.push((k, n, g.size, g.y)),
        }
    }
    if let Some((_, _, sz, y)) = sizes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)))
    {
        best_sz = sz;
        best_y = y;
    }
    if best_sz <= 0.1 {
        best_sz = fallback_size;
    }

    let items: Vec<Item<'_>> = sorted
        .iter()
        .map(|&i| Item {
            g: &glyphs[i],
            text: glyphs[i].text.clone(),
        })
        .collect();
    let refs: Vec<&Item<'_>> = items.iter().collect();
    let tex = tex_of(&refs, best_y, best_sz);
    (tex, best_y, best_sz)
}

fn group_has_math(glyphs: &[Glyph], ids: &[usize]) -> bool {
    ids.iter().any(|&i| {
        let g = &glyphs[i];
        g.math != MathKind::Text
            || g.text
                .chars()
                .any(|c| is_math_symbol(c) || is_greek(c) || "+-=<>()[]{}".contains(c))
    })
}

pub(crate) fn looks_like_math_expr(glyphs: &[Glyph], ids: &[usize]) -> bool {
    if group_has_math(glyphs, ids) {
        return true;
    }
    // 纯数字或单字母短变量（如 `\frac{1}{2}`, `\frac{d}{dx}`）也视为数学子式
    let total_chars: usize = ids
        .iter()
        .map(|&i| glyphs[i].text.trim().chars().count())
        .sum();
    total_chars <= 4
        && ids.iter().any(|&i| {
            glyphs[i]
                .text
                .chars()
                .any(|c| c.is_ascii_digit() || c.is_ascii_alphabetic())
        })
}

/// 检查字形集是不是多词英文/自然语言正文（防止把表头/正文横线误认为分式）。
pub(crate) fn looks_like_prose(glyphs: &[Glyph], ids: &[usize], body: f64) -> bool {
    let mut sorted = ids.to_vec();
    sorted.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
    let mut prose_words = 0usize;
    let mut cur = String::new();
    let mut prev_x1 = f64::NEG_INFINITY;
    let mut cur_all_text = true;
    let flush = |w: &mut String, all_text: &mut bool, count: &mut usize| {
        if *all_text {
            for part in w.split('-') {
                if part.chars().count() >= 3
                    && part.chars().all(|c| c.is_alphabetic())
                    && !is_func_or_op(part)
                {
                    *count += 1;
                }
            }
        }
        w.clear();
        *all_text = true;
    };
    for &i in &sorted {
        let g = &glyphs[i];
        let t = g.text.trim();
        if t.is_empty() {
            flush(&mut cur, &mut cur_all_text, &mut prose_words);
            continue;
        }
        if prev_x1.is_finite() && g.x - prev_x1 > 0.18 * body {
            flush(&mut cur, &mut cur_all_text, &mut prose_words);
        }
        if g.math != MathKind::Text {
            cur_all_text = false;
        }
        cur.push_str(t);
        prev_x1 = prev_x1.max(g.x_end.max(g.x));
    }
    flush(&mut cur, &mut cur_all_text, &mut prose_words);
    prose_words >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(text: &str, x: f64, y: f64, size: f64, math: MathKind) -> Glyph {
        Glyph {
            text: text.into(),
            x,
            y,
            x_end: x + 0.55 * size * text.chars().count().max(1) as f64,
            size,
            bold: false,
            math,
            mcid: None,
            artifact: false,
        }
    }

    #[test]
    fn reconstructs_fraction_and_nested_fraction() {
        // \frac{d}{d\epsilon}
        let mut glyphs = vec![
            g("d", 102.0, 580.0, 10.0, MathKind::Italic),
            g("d", 100.0, 565.0, 10.0, MathKind::Italic),
            g("ϵ", 106.0, 565.0, 10.0, MathKind::Italic),
            g("J", 116.0, 572.0, 10.0, MathKind::Italic),
        ];
        let mut rules = vec![Rule {
            x0: 99.5,
            y0: 574.8,
            x1: 112.5,
            y1: 574.8,
        }];
        reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(rules.is_empty(), "fraction bar rule is consumed");
        assert!(
            glyphs
                .iter()
                .any(|x| x.math == MathKind::Prebuilt && x.text == "\\frac{d}{d\\epsilon}"),
            "got: {glyphs:?}"
        );
    }

    #[test]
    fn reconstructs_radical_with_index() {
        // \sqrt[3]{x+1}
        let mut glyphs = vec![
            g("3", 98.0, 507.0, 6.0, MathKind::Text),
            g("√", 100.0, 500.0, 10.0, MathKind::Symbol),
            g("x", 107.0, 500.0, 10.0, MathKind::Italic),
            g("+", 113.0, 500.0, 10.0, MathKind::Text),
            g("1", 120.0, 500.0, 10.0, MathKind::Text),
        ];
        let mut rules = vec![Rule {
            x0: 106.0,
            y0: 509.5,
            x1: 127.0,
            y1: 509.5,
        }];
        reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(rules.is_empty());
        assert!(
            glyphs
                .iter()
                .any(|x| x.math == MathKind::Prebuilt && x.text == "\\sqrt[3]{x+1}"),
            "got: {glyphs:?}"
        );
    }

    #[test]
    fn reconstructs_sum_with_cmex_hanging_limits() {
        // \sum_{k=1}^{K} with CMEX hanging baseline
        let mut glyphs = vec![
            g("K", 104.0, 508.0, 7.0, MathKind::Italic),
            g("∑", 100.0, 505.0, 10.0, MathKind::Extension),
            g("k", 99.0, 487.0, 7.0, MathKind::Italic),
            g("=", 103.5, 487.0, 7.0, MathKind::Text),
            g("1", 108.0, 487.0, 7.0, MathKind::Text),
            g("x", 118.0, 500.0, 10.0, MathKind::Italic),
        ];
        let mut rules = Vec::new();
        reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(
            glyphs
                .iter()
                .any(|x| x.math == MathKind::Prebuilt && x.text == "\\sum_{k=1}^{K}"),
            "got: {glyphs:?}"
        );
    }
}
