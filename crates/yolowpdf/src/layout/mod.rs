//! 版面重组：字形 → 行片段 → 分栏阅读顺序 → 段落 → Markdown。

use std::collections::HashMap;

use crate::interp::Glyph;

mod columns;
mod furniture;
mod headings;
mod render;

pub(crate) use columns::{blocks, gutters_of};
pub(crate) use furniture::remove_furniture;
#[cfg(test)]
use furniture::{furniture_key, is_page_number};
#[cfg(test)]
use render::merge_math;
pub(crate) use render::{Mode, Para, join_text, paragraphs, render};

/// 同一基线上连续的一段文字。分栏以前叫"片段"，合并同基线片段后就是"行"。
#[derive(Debug, Clone)]
pub(crate) struct Line {
    pub(crate) text: String,
    pub(crate) x0: f64,
    pub(crate) x1: f64,
    pub(crate) y: f64,
    pub(crate) size: f64,
    pub(crate) bold: bool,
    /// 原样输出的内容（表格的 Markdown），单独成段，不参与合并。
    pub(crate) raw: bool,
    /// 多数字符所在标记内容的 MCID。
    pub(crate) mcid: Option<i32>,
    /// 多数字符在 /Artifact 里。
    pub(crate) artifact: bool,
    /// 写入方给出的标题层级（来自结构树）；None 表示没有提示。
    pub(crate) hint: Option<u8>,
    /// 标题提示来自哪个结构元素：同一元素的行才能合成一段。
    pub(crate) group: Option<u32>,
    /// 写入方声明过的标题行（对上了这一页的书签）：不当页眉页脚删。
    pub(crate) keep: bool,
    /// 由少数方向的旋转文字拼成（竖写的坐标轴标题、转过的表头）：参加图区认领，不参加表格识别（见 orient.rs）。
    pub(crate) turned: bool,
}

/// 片段内超过这个间距（相对字号）就拆开，拆多了没关系，分栏后会按基线合回去。
const SPLIT_GAP: f64 = 0.45;
const SPACE_GAP: f64 = 0.15;
const CJK_SPACE_GAP: f64 = 0.5;

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FDF | 0x3000..=0x30FF | 0x3100..=0x312F | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF
        | 0x20000..=0x2FFFF)
}

fn normalize_char(c: char, out: &mut String) {
    match c {
        '\u{FB00}' => out.push_str("ff"),
        '\u{FB01}' => out.push_str("fi"),
        '\u{FB02}' => out.push_str("fl"),
        '\u{FB03}' => out.push_str("ffi"),
        '\u{FB04}' => out.push_str("ffl"),
        '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{3000}' | '\t' => out.push(' '),
        '\u{200B}'..='\u{200D}' | '\u{FEFF}' | '\u{FFFD}' => {}
        c if c.is_control() => {}
        c => out.push(crate::encoding::unify_radical(c)),
    }
}

/// TeX 用两个字形拼出来的符号合成一个：否定斜杠叠在后一个符号上（̸= → ≠），`\mapsto` 是短竖加箭头。
fn compose_tex(text: &str) -> String {
    if !text.contains(['\u{0338}', '↦']) {
        return text.to_string();
    }
    let negated = |c: char| match c {
        '=' => Some('≠'),
        '∈' => Some('∉'),
        '∋' => Some('∌'),
        '<' => Some('≮'),
        '>' => Some('≯'),
        '≤' => Some('≰'),
        '≥' => Some('≱'),
        '⊂' => Some('⊄'),
        '⊃' => Some('⊅'),
        '⊆' => Some('⊈'),
        '⊇' => Some('⊉'),
        '∼' => Some('≁'),
        '≃' => Some('≄'),
        '≈' => Some('≉'),
        '≡' => Some('≢'),
        '∃' => Some('∄'),
        '|' | '∣' => Some('∤'),
        '∥' => Some('∦'),
        _ => None,
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\u{0338}'
            && let Some(n) = next.and_then(negated)
        {
            out.push(n);
            i += 2;
            continue;
        }
        if c == '↦' && next == Some('→') {
            out.push('↦');
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// 把一页的字形聚合成行片段。`heading_of` 给出 MCID 所属的标题元素（结构树）：
/// 同一条基线上属于不同标题元素（或一边是标题一边不是）的字形分成不同片段。
pub(crate) fn fragments(
    glyphs: &[Glyph],
    heading_of: Option<&HashMap<i32, (u8, u32)>>,
) -> Vec<Line> {
    let group = |g: &Glyph| {
        heading_of
            .and_then(|h| g.mcid.and_then(|m| h.get(&m)))
            .map(|&(_, e)| e)
    };
    let mut order: Vec<usize> = (0..glyphs.len()).collect();
    order.sort_by(|&a, &b| {
        glyphs[b]
            .y
            .total_cmp(&glyphs[a].y)
            .then(glyphs[a].x.total_cmp(&glyphs[b].x))
    });

    // 先按基线初步聚带，再按水平空隙（栏间空白 > 2.2*size 或跨 0.75*size 空白的显著字号跳变）拆分成局部水平段。
    // 这彻底解耦双栏/多栏相位错位页面（如 IEEE 双栏论文中左栏正文 y_1 恰好撞上右栏上标 y_2 + 3.6pt），
    // 防止另一栏的正文行劫持本栏的数学上下标行导致全栏上标脱离为孤立段落。
    struct Row {
        idx: Vec<usize>,
        y: f64,
        size: f64,
        x0: f64,
        x1: f64,
    }
    let mut bands: Vec<(f64, f64, Vec<usize>)> = Vec::new();
    for i in order {
        let g = &glyphs[i];
        match bands.last_mut() {
            Some((by, bsz, ids))
                if (*by - g.y).abs() <= 0.15 * bsz.min(g.size).max(1.0)
                    || ((*by - g.y).abs() <= 0.15 * bsz.max(g.size)
                        && (*bsz / g.size.max(0.1)).max(g.size / bsz.max(0.1)) <= 1.6) =>
            {
                ids.push(i);
                if ids.len() <= 3 {
                    *bsz = bsz.max(g.size);
                }
            }
            _ => bands.push((g.y, g.size, vec![i])),
        }
    }
    let mut rows: Vec<Row> = Vec::with_capacity(bands.len());
    for (_, _, mut ids) in bands {
        ids.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
        let mut start = 0usize;
        let mut prev_end = glyphs[ids[0]].x_end.max(glyphs[ids[0]].x);
        let mut seg_sz = glyphs[ids[0]].size;
        for pos in 1..ids.len() {
            let g = &glyphs[ids[pos]];
            let gap = g.x - prev_end;
            let sz_max = seg_sz.max(g.size).max(1.0);
            // 仅在真正宽于栏间空白（> 2.2*size）处横向拆带，防止把同栏行内公式按微小字距切碎
            if gap > 2.2 * sz_max {
                let slice = ids[start..pos].to_vec();
                let (x0, x1, y, size) = summarize_row(&slice, glyphs);
                rows.push(Row {
                    idx: slice,
                    y,
                    size,
                    x0,
                    x1,
                });
                start = pos;
                prev_end = g.x_end.max(g.x);
                seg_sz = g.size;
            } else {
                prev_end = prev_end.max(g.x_end.max(g.x));
                seg_sz = seg_sz.max(g.size);
            }
        }
        let slice = ids[start..].to_vec();
        let (x0, x1, y, size) = summarize_row(&slice, glyphs);
        rows.push(Row {
            idx: slice,
            y,
            size,
            x0,
            x1,
        });
    }
    rows.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x0.total_cmp(&b.x0)));

    // 首字下沉（Drop Cap）识别与归位：孤立单个大号大写字母位于 2–4 行缩进正文左侧时，
    // 记录其归属到最顶上那一行缩进正文的开头，并从原位置移除。
    let mut drop_cap_prefix: HashMap<usize, (usize, bool)> = HashMap::new();
    let mut body_sizes: Vec<f64> = rows
        .iter()
        .filter(|r| r.idx.len() >= 4)
        .map(|r| r.size)
        .collect();
    body_sizes.sort_by(f64::total_cmp);
    let page_body = body_sizes
        .get(body_sizes.len() / 2)
        .copied()
        .unwrap_or(10.0);
    for i in 0..rows.len() {
        if rows[i].idx.len() != 1 {
            continue;
        }
        let gi = rows[i].idx[0];
        let dg = &glyphs[gi];
        let dt = dg.text.trim();
        let mut dcs = dt.chars();
        let (Some(dc), None) = (dcs.next(), dcs.next()) else {
            continue;
        };
        if !dc.is_uppercase()
            || dg.math != crate::font::MathKind::Text
            || dg.size < 1.75 * page_body
        {
            continue;
        }
        let d_x1 = dg.x_end.max(dg.x + 0.5 * dg.size);
        let d_top = dg.y + 0.78 * dg.size;
        let d_bot = dg.y - 0.15 * dg.size;
        // 找右侧紧邻且纵向落在首字下沉高度范围内的 2–4 行正文
        let mut beside: Vec<usize> = Vec::new();
        for (j, h) in rows.iter().enumerate() {
            if j == i || h.idx.len() < 3 || h.size > 0.65 * dg.size {
                continue;
            }
            let h_mid = h.y + 0.35 * h.size;
            if h_mid <= d_top + 0.6 * h.size
                && h_mid >= d_bot - 0.5 * h.size
                && h.x0 >= d_x1 - 0.45 * h.size
                && h.x0 <= d_x1 + 2.2 * h.size
            {
                beside.push(j);
            }
        }
        if beside.len() >= 2 && beside.len() <= 5 {
            // 取最靠上的一行作为宿主行
            if let Some(&top_row) = beside
                .iter()
                .max_by(|&&a, &&b| rows[a].y.total_cmp(&rows[b].y))
            {
                // 判断是否需要在首字下沉与宿主行首词之间加空格（如 `A sweet` / `I have` vs `Do you` / `F’dan`）
                let mut host_ids = rows[top_row].idx.clone();
                host_ids.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
                let first_text: String = host_ids
                    .iter()
                    .take(8)
                    .map(|&k| glyphs[k].text.as_str())
                    .collect();
                let first_word = first_text
                    .trim_start()
                    .split(|c: char| !c.is_alphabetic())
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                let standalone_article = matches!(dc, 'A' | 'I')
                    && matches!(
                        first_word.as_str(),
                        "sweet"
                            | "new"
                            | "good"
                            | "great"
                            | "small"
                            | "long"
                            | "short"
                            | "few"
                            | "man"
                            | "woman"
                            | "day"
                            | "year"
                            | "time"
                            | "way"
                            | "have"
                            | "had"
                            | "was"
                            | "am"
                            | "will"
                            | "would"
                            | "can"
                            | "could"
                            | "did"
                            | "do"
                            | "don"
                            | "know"
                            | "think"
                            | "want"
                            | "need"
                    );
                if let std::collections::hash_map::Entry::Vacant(e) = drop_cap_prefix.entry(top_row)
                {
                    e.insert((gi, standalone_article));
                    rows[i].idx.clear();
                }
            }
        }
    }

    for i in 0..rows.len() {
        let r = &rows[i];
        if r.idx.is_empty() || drop_cap_prefix.contains_key(&i) {
            continue;
        }
        let r_has_math = r
            .idx
            .iter()
            .any(|&k| glyphs[k].math != crate::font::MathKind::Text);
        let mut best: Option<(usize, f64)> = None;
        // rows 已按 y 降序排好：在上下 1.5*body 垂直邻域内搜索所有水平重叠的宿主行（多栏分拆后仍 O(1) 均摊）
        let y_win = (1.25 * page_body).max(18.0);
        let lo = rows.partition_point(|h| h.y > r.y + y_win);
        let hi = rows.partition_point(|h| h.y >= r.y - y_win);
        for (j, h) in rows.iter().enumerate().take(hi).skip(lo) {
            if j == i || h.idx.is_empty() {
                continue;
            }
            let dy = (h.y - r.y).abs();
            let h_has_math = h
                .idx
                .iter()
                .any(|&k| glyphs[k].math != crate::font::MathKind::Text);
            let count_ok = h.idx.len() >= r.idx.len() * 2
                || ((r_has_math || h_has_math) && r.size <= h.size * 0.86 && r.idx.len() <= 24);
            let dy_lim = if h_has_math || r_has_math {
                0.75 * h.size
            } else {
                0.6 * h.size
            };
            if h.size >= r.size * 1.1
                && count_ok
                && dy < dy_lim
                && r.x0 <= h.x1 + 0.6 * h.size
                && r.x1 >= h.x0 - 0.6 * h.size
                && best.is_none_or(|(_, d)| dy < d)
            {
                // 水平重叠守护：如果 r 中有 >= 2 个字形横向直接叠在 h 的同字号字形正上方/正下方
                // （如堆叠的两行小字 `FRIDAY` 压在 `4 JANUARY 2019` 上方），绝不是上下标，禁止合并！
                let overlaps = r
                    .idx
                    .iter()
                    .filter(|&&ri| {
                        let rg = &glyphs[ri];
                        let rcx = 0.5 * (rg.x + rg.x_end.max(rg.x));
                        h.idx.iter().any(|&hi| {
                            let hg = &glyphs[hi];
                            rcx >= hg.x && rcx <= hg.x_end.max(hg.x)
                        })
                    })
                    .count();
                if !r_has_math && !h_has_math && overlaps >= 2 {
                    continue;
                }
                best = Some((j, dy));
            }
        }
        if let Some((j, _)) = best {
            let moved = std::mem::take(&mut rows[i].idx);
            rows[j].idx.extend(moved);
        }
    }
    let mut out = Vec::new();
    for (row_idx, row) in rows.into_iter().enumerate() {
        let mut c = row.idx;
        if c.is_empty() {
            continue;
        }
        c.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
        let prefix = drop_cap_prefix.get(&row_idx).copied();
        let mut first_frag = true;
        let mut cur: Vec<&Glyph> = Vec::new();
        let mut prev_end = f64::NEG_INFINITY;
        for &i in &c {
            let g = &glyphs[i];
            if let Some(p) = cur.last() {
                // 重复叠印（假粗体）只留一份
                if p.text == g.text
                    && (p.x - g.x).abs() < 0.2 * g.size
                    && (p.y - g.y).abs() < 0.2 * g.size
                {
                    continue;
                }
                let gap = g.x - prev_end;
                let is_tex_math = |m: crate::font::MathKind| {
                    !matches!(
                        m,
                        crate::font::MathKind::Text | crate::font::MathKind::Symbol
                    )
                };
                let split_k = if is_tex_math(g.math) || is_tex_math(p.math) {
                    0.90
                } else {
                    SPLIT_GAP
                };
                if gap > split_k * g.size.max(p.size)
                    || (heading_of.is_some() && group(p) != group(g))
                {
                    if let Some(mut line) = make_line(&cur) {
                        if first_frag && let Some((di, space)) = prefix {
                            prepend_drop_cap(&mut line, &glyphs[di], space);
                            first_frag = false;
                        }
                        out.push(line);
                    }
                    cur.clear();
                    prev_end = f64::NEG_INFINITY;
                }
            }
            prev_end = prev_end.max(g.x_end).max(g.x);
            cur.push(g);
        }
        if let Some(mut line) = make_line(&cur) {
            if first_frag && let Some((di, space)) = prefix {
                prepend_drop_cap(&mut line, &glyphs[di], space);
            }
            out.push(line);
        }
    }
    out
}

fn summarize_row(ids: &[usize], glyphs: &[Glyph]) -> (f64, f64, f64, f64) {
    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let y = glyphs[ids[0]].y;
    let size = if ids.len() >= 2 {
        let mut szs: Vec<f64> = ids
            .iter()
            .map(|&k| {
                let g = &glyphs[k];
                x0 = x0.min(g.x);
                x1 = x1.max(g.x_end.max(g.x));
                g.size
            })
            .collect();
        szs.sort_by(f64::total_cmp);
        szs[szs.len() / 2]
    } else {
        let g = &glyphs[ids[0]];
        x0 = g.x;
        x1 = g.x_end.max(g.x);
        g.size
    };
    (x0, x1, y, size)
}

fn prepend_drop_cap(line: &mut Line, dg: &Glyph, add_space: bool) {
    let ch = dg.text.trim();
    if ch.is_empty() {
        return;
    }
    let sep = if add_space { " " } else { "" };
    line.text = format!("{ch}{sep}{}", line.text);
    line.x0 = line.x0.min(dg.x);
}

/// 基于 O'Gorman (1993) Docstrum 与稳健分位数的自适应词间距阈值估计：
/// - 数学行保持标准 `0.15*size`（避免把 TeX 斜体修正如 `P(z)` 误拆成 `P (z)`）；
/// - 仅在检测到一致的宽字距（Tracking）标题（单字对 15% 分位 >= 0.06*size）时抬升阈值，
///   避免把字母拆散，同时不在 OCR 抖动行上过度抬升导致词粘连。
fn adaptive_space_gap(glyphs: &[&Glyph]) -> f64 {
    if glyphs.iter().any(|g| g.math != crate::font::MathKind::Text) {
        return SPACE_GAP;
    }
    let mut single_gaps: Vec<f64> = Vec::new();
    let mut prev: Option<&Glyph> = None;
    let mut prev_end = f64::NEG_INFINITY;
    for &g in glyphs {
        let t = g.text.trim();
        if t.is_empty() {
            prev = None;
            continue;
        }
        if let Some(p) = prev {
            let s = g.size.max(p.size);
            if s > 0.5 && p.text.trim().chars().count() == 1 && t.chars().count() == 1 {
                let p_cjk = p.text.chars().any(is_cjk);
                let g_cjk = t.chars().any(is_cjk);
                if !p_cjk && !g_cjk {
                    let r = (g.x - prev_end) / s;
                    if (-0.05..0.40).contains(&r) {
                        single_gaps.push(r);
                    }
                }
            }
        }
        prev_end = prev_end.max(g.x_end).max(g.x);
        prev = Some(g);
    }
    if single_gaps.len() >= 10 {
        single_gaps.sort_by(f64::total_cmp);
        let p15 = single_gaps[(single_gaps.len() * 15) / 100];
        if p15 >= 0.06 {
            return (p15 + 0.10).clamp(SPACE_GAP, 0.23);
        }
    }
    SPACE_GAP
}

fn make_line(glyphs: &[&Glyph]) -> Option<Line> {
    let latin_space_gap = adaptive_space_gap(glyphs);
    // 按空格切词；词内文字和原来逐字拼接一致
    let mut words: Vec<Vec<crate::math::Item>> = Vec::new();
    let mut pending_space = false;
    let mut prev: Option<&Glyph> = None;
    let mut prev_end = f64::NEG_INFINITY;
    let mut bold_chars = 0usize;
    let mut all_chars = 0usize;
    // 一行里的字号、MCID 通常只有一两种：小数组线性找，不建哈希表
    let mut sizes: Vec<(i64, (usize, f64, f64))> = Vec::new();
    let mut mcids: Vec<(i32, usize)> = Vec::new();
    let mut artifact_chars = 0usize;
    let mut last_char: Option<char> = None;
    // 规范化后的字，每个字形复用同一个缓冲区
    let mut t = String::new();
    for g in glyphs {
        t.clear();
        if g.math == crate::font::MathKind::Prebuilt {
            t.push_str(&g.text);
        } else {
            for c in g.text.chars() {
                normalize_char(c, &mut t);
            }
        }
        if t.trim().is_empty() {
            if !t.is_empty() {
                pending_space = true;
            }
            continue;
        }
        let mut new_word = words.is_empty();
        if let Some(p) = prev {
            let gap = g.x - prev_end;
            let cjk = last_char.is_some_and(is_cjk) && t.chars().next().is_some_and(is_cjk);
            // 紧凑多字符 TJ 词块之间（如报纸窄栏 `-135` 负调距）兼容到 0.125*size
            let tight_word_pair = !cjk
                && p.math == crate::font::MathKind::Text
                && g.math == crate::font::MathKind::Text
                && p.text.trim().chars().count() >= 2
                && t.chars().count() >= 2
                && last_char.is_some_and(char::is_alphanumeric)
                && t.chars().next().is_some_and(char::is_alphanumeric);
            let limit = if cjk {
                CJK_SPACE_GAP
            } else if tight_word_pair {
                latin_space_gap.min(0.125)
            } else {
                latin_space_gap
            };
            if pending_space || gap > limit * g.size.max(p.size) {
                new_word = true;
            }
            // 防止长英文普通词（如 `frequency`）与紧邻无空隙的希腊字母/数学符号粘成同一词而被吸入 `$...$`
            if !new_word
                && p.math == crate::font::MathKind::Text
                && g.math != crate::font::MathKind::Text
                && (g.y - p.y).abs() <= 0.15 * p.size
                && let Some(last_w) = words.last()
                && last_w
                    .iter()
                    .all(|it| it.g.math == crate::font::MathKind::Text)
            {
                let cur_word: String = last_w.iter().map(|it| it.text.as_str()).collect();
                if cur_word.len() >= 4
                    && cur_word.chars().all(|c| c.is_ascii_alphabetic())
                    && !crate::math::symbols::is_func_or_op(&cur_word)
                {
                    new_word = true;
                }
            }
        }
        pending_space = false;
        let n = t.chars().count();
        all_chars += n;
        if g.bold {
            bold_chars += n;
        }
        if let Some(m) = g.mcid {
            match mcids.iter_mut().find(|(k, _)| *k == m) {
                Some((_, c)) => *c += n,
                None => mcids.push((m, n)),
            }
        }
        if g.artifact {
            artifact_chars += n;
        }
        let key = (g.size * 10.0).round() as i64;
        let e = match sizes.iter().position(|(k, _)| *k == key) {
            Some(i) => &mut sizes[i].1,
            None => {
                sizes.push((key, (0, g.size, g.y)));
                &mut sizes.last_mut().expect("just pushed").1
            }
        };
        e.0 += n;
        last_char = t.chars().last().or(last_char);
        let item = crate::math::Item {
            g,
            text: t.trim_start().into(),
        };
        match words.last_mut() {
            Some(w) if !new_word => w.push(item),
            _ => words.push(vec![item]),
        }
        prev_end = prev_end.max(g.x_end).max(g.x);
        prev = Some(g);
    }
    // 字号取字符数最多的那个，基线也跟着它
    let (_, size, y) = sizes
        .into_iter()
        .map(|(_, v)| v)
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))?;
    let text = crate::math::line_text(&words, y, size).unwrap_or_else(|| {
        let mut text = String::new();
        for w in &words {
            if !text.is_empty() && !text.ends_with(' ') {
                text.push(' ');
            }
            for i in w {
                text.push_str(&i.text);
            }
        }
        text
    });
    let text = compose_tex(text.trim_end());
    if text.is_empty() {
        return None;
    }
    let first = glyphs.iter().find(|g| !g.text.trim().is_empty())?;
    let last_end = glyphs
        .iter()
        .map(|g| g.x_end.max(g.x))
        .fold(f64::NEG_INFINITY, f64::max);
    Some(Line {
        text,
        x0: first.x,
        x1: last_end,
        y,
        size,
        bold: bold_chars * 10 >= all_chars * 7,
        raw: false,
        mcid: mcids
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(m, _)| m),
        artifact: artifact_chars * 2 > all_chars,
        hint: None,
        group: None,
        keep: false,
        turned: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_formulas_merge() {
        assert_eq!(
            merge_math("with $0 \\leq k \\leq$ $2^{N}$. Then $a$ and $b$"),
            "with $0 \\leq k \\leq 2^{N}$. Then $a$ and $b$"
        );
        assert_eq!(
            merge_math("$S$ = $(P, L)$ and $N \\geq$ 0"),
            "$S = (P, L)$ and $N \\geq 0$"
        );
        assert_eq!(merge_math("log $|z|$"), "$\\log |z|$");
        // 货币金额绝不能被当作公式合并或破坏空格
        let currency = "Example. Income of $164,700. Deductions total $20,000. Gifts of $12,000. The other $8,000 ($20,000 – $12,000) of deductions.";
        assert_eq!(merge_math(currency), currency);
        // 货币与真实公式混排时仅合并真实公式
        assert_eq!(
            merge_math("Cost is $5 and $x$ + $y$ = $z$"),
            "Cost is $5 and $x + y = z$"
        );
        // 裸花括号绝不并入公式破坏配平
        assert_eq!(merge_math("$a$ { $b$"), "$a$ { $b$");
    }

    #[test]
    fn hyphen_join() {
        let mut a = "an extra-".to_string();
        join_text(&mut a, "ordinary day");
        assert_eq!(a, "an extraordinary day");
        let mut a = "state-of-the-".to_string();
        join_text(&mut a, "art");
        assert_eq!(a, "state-of-the-art");
        let mut a = "中文".to_string();
        join_text(&mut a, "句子");
        assert_eq!(a, "中文句子");
        let mut a = "co\u{2010}".to_string();
        join_text(&mut a, "operate");
        assert_eq!(a, "cooperate");
        let mut a = "Version 2-".to_string();
        join_text(&mut a, "Beta");
        assert_eq!(a, "Version 2- Beta");
    }

    #[test]
    fn furniture_keys() {
        assert_eq!(furniture_key("Page 12 of 30"), "page # of #");
        assert!(is_page_number(&furniture_key("- 7 -")));
        assert!(is_page_number(&furniture_key("xiv")));
        assert!(!is_page_number(&furniture_key("Chapter 3")));
    }
}
