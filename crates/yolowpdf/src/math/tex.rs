//! TeX 语法树构建与公式排版逻辑（含 DRACULAE 2D 上下标解缠、TeX 算子识别、组合符号与 KaTeX 语法守护）。

use super::footnote;
use super::symbols::{
    ACCENTS, FUNCS, OPERATORS, is_combining, is_func_or_op, is_greek, is_math_symbol, push_char,
};
use crate::font::MathKind;
use crate::interp::Glyph;

/// 一个字形和它规范化后的文字。
pub(crate) struct Item<'a> {
    pub(crate) g: &'a Glyph,
    pub(crate) text: crate::short::Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Word {
    Text,
    Glue,
    Math,
}

/// 公式内部常见的短自然语言限定词（出现在未闭合括号内或两段公式之间时用 `\text{...}` 桥接）。
fn is_math_qualifier(s: &str) -> bool {
    matches!(
        s,
        "iff"
            | "if"
            | "for"
            | "and"
            | "or"
            | "when"
            | "where"
            | "with"
            | "a.a."
            | "a.e."
            | "s.t."
            | "a.s."
            | "i.e."
            | "w.r.t."
            | "const"
            | "otherwise"
            | "all"
    )
}

pub(crate) fn classify(word: &[Item<'_>]) -> Word {
    let chars: Vec<char> = word
        .iter()
        .flat_map(|i| i.text.chars())
        .filter(|c| !c.is_whitespace())
        .collect();
    if chars.is_empty() {
        return Word::Glue;
    }
    // 裸 `$`（货币符号）绝不进入数学模式，避免破坏外层 `$...$` 定界符
    if chars.contains(&'$') {
        return Word::Text;
    }
    if word.iter().any(|i| i.g.math == MathKind::Prebuilt) {
        return Word::Math;
    }
    let math_font = word
        .iter()
        .any(|i| i.g.math != MathKind::Text && !i.text.trim().is_empty());
    let symbol = chars.iter().any(|&c| is_math_symbol(c));
    let short_greek = chars.len() <= 3 && chars.iter().any(|&c| is_greek(c));
    if math_font || symbol || short_greek {
        return Word::Math;
    }
    if chars
        .iter()
        .all(|&c| c.is_ascii_digit() || "+-=<>()[]{}|/,.:;!'*−∶".contains(c))
    {
        return Word::Glue;
    }
    // 纯文本的标准数学函数/算子名（如 `log`, `sin`, `lim`, `inf`, `Hom`, `det`）设为 Glue：
    // 普通句子里孤立出现时保持正文不变，紧邻数学表达式时自动并入公式！
    let trimmed: String = chars.iter().collect();
    let alpha = trimmed.trim_matches(|c: char| "()[]{}|/,.:;".contains(c));
    if !alpha.is_empty() && alpha.chars().all(|c| c.is_ascii_alphabetic()) && is_func_or_op(alpha) {
        return Word::Glue;
    }
    Word::Text
}

pub(crate) fn evidence(item: &Item<'_>) -> bool {
    let c = item.text.trim().chars().next();
    match item.g.math {
        MathKind::Text => false,
        MathKind::Prebuilt => !item.text.trim().is_empty(),
        MathKind::Symbol => c.is_some_and(|c| {
            c.is_alphanumeric() || is_math_symbol(c) || "=+<>|()[]{}′'".contains(c)
        }),
        _ => c.is_some(),
    }
}

pub(crate) fn punct_only(word: &[Item<'_>]) -> bool {
    word.iter()
        .all(|i| i.text.trim().chars().all(|c| ",.;:".contains(c)))
}

pub(crate) fn push_word(out: &mut String, w: &str) {
    if w.is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
    out.push_str(w);
}

pub(crate) fn plain(word: &[Item<'_>]) -> String {
    word.iter().map(|i| i.text.trim_start()).collect()
}

pub(crate) fn plain_items(items: &[&Item<'_>]) -> String {
    items.iter().map(|i| i.text.trim_start()).collect()
}

pub(crate) struct Atom {
    pub(crate) tex: String,
    pub(crate) x0: f64,
    pub(crate) x1: f64,
    pub(crate) y: f64,
    pub(crate) size: f64,
    pub(crate) fixed: bool,
    pub(crate) is_dot: bool,
    pub(crate) is_cdot: bool,
}

fn negated_symbol(c: char) -> Option<char> {
    match c {
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
    }
}

pub(crate) fn tex_of(items: &[&Item<'_>], base_y: f64, base_size: f64) -> String {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut accents: Vec<(usize, &str, f64)> = Vec::new();
    let mut k = 0;
    while k < items.len() {
        let item = items[k];
        k += 1;
        let t = item.text.trim();
        if t.is_empty() {
            continue;
        }
        let g = item.g;

        // 2D 引擎预构建好的完整 LaTeX 片段（`\frac`, `\sqrt`, `\sum_{...}^{...}`, `\overline`）原样嵌入
        if g.math == MathKind::Prebuilt {
            atoms.push(Atom {
                tex: format!("{t} "),
                x0: g.x,
                x1: g.x_end.max(g.x),
                y: g.y,
                size: g.size,
                fixed: true,
                is_dot: false,
                is_cdot: false,
            });
            continue;
        }

        // TeX 组合否定符号（`\u{0338}` 叠加在下一个符号上，如 `\not` + `=` → `≠`）和 `\mapsto`（`↦` + `→`）
        if t == "\u{0338}"
            && k < items.len()
            && let Some(next_c) = items[k].text.trim().chars().next()
            && items[k].text.trim().chars().count() == 1
            && let Some(comp) = negated_symbol(next_c)
        {
            let ng = items[k].g;
            k += 1;
            let mut tex = String::new();
            push_char(comp, ng.math, &mut tex);
            atoms.push(Atom {
                tex,
                x0: g.x.min(ng.x),
                x1: g.x_end.max(ng.x_end).max(g.x),
                y: ng.y,
                size: ng.size,
                fixed: false,
                is_dot: false,
                is_cdot: false,
            });
            continue;
        }
        if t == "↦" && k < items.len() && items[k].text.trim() == "→" {
            let ng = items[k].g;
            k += 1;
            atoms.push(Atom {
                tex: "\\mapsto ".to_string(),
                x0: g.x.min(ng.x),
                x1: g.x_end.max(ng.x_end).max(g.x),
                y: ng.y,
                size: ng.size,
                fixed: false,
                is_dot: false,
                is_cdot: false,
            });
            continue;
        }

        // 识别连续直体字母组成的标准函数名/算子名（以及 `lim inf` → `\liminf`、`lim sup` → `\limsup`）
        if g.math == MathKind::Text && t.chars().all(|c| c.is_ascii_alphabetic()) {
            let mut j = k;
            let mut name = t.to_string();
            let mut last_x1 = g.x_end.max(g.x);
            while j < items.len()
                && items[j].g.math == MathKind::Text
                && (items[j].g.y - g.y).abs() < 0.12 * g.size
                && items[j].g.x - last_x1 <= 0.20 * g.size
                && !items[j].text.trim().is_empty()
                && items[j]
                    .text
                    .trim()
                    .chars()
                    .all(|c| c.is_ascii_alphabetic())
            {
                name.push_str(items[j].text.trim());
                last_x1 = last_x1.max(items[j].g.x_end.max(items[j].g.x));
                j += 1;
            }
            // 处理 `lim inf` / `lim sup` / `arg min` / `arg max` 中间带薄空格的双词算子
            if name == "lim" && j < items.len() && items[j].g.math == MathKind::Text {
                let mut m = j;
                let mut w2 = String::new();
                let mut lx = last_x1;
                while m < items.len()
                    && items[m].g.math == MathKind::Text
                    && (items[m].g.y - g.y).abs() < 0.12 * g.size
                    && (m == j && items[m].g.x - lx <= 0.65 * g.size
                        || m > j && items[m].g.x - lx <= 0.20 * g.size)
                    && !items[m].text.trim().is_empty()
                    && items[m]
                        .text
                        .trim()
                        .chars()
                        .all(|c| c.is_ascii_alphabetic())
                {
                    w2.push_str(items[m].text.trim());
                    lx = lx.max(items[m].g.x_end.max(items[m].g.x));
                    m += 1;
                }
                if w2 == "inf" || w2 == "sup" {
                    atoms.push(Atom {
                        tex: format!("\\lim{w2} "),
                        x0: g.x,
                        x1: lx,
                        y: g.y,
                        size: g.size,
                        fixed: false,
                        is_dot: false,
                        is_cdot: false,
                    });
                    k = m;
                    continue;
                }
            }
            if FUNCS.contains(&name.as_str()) {
                atoms.push(Atom {
                    tex: format!("\\{name} "),
                    x0: g.x,
                    x1: last_x1,
                    y: g.y,
                    size: g.size,
                    fixed: false,
                    is_dot: false,
                    is_cdot: false,
                });
                k = j;
                continue;
            }
            if OPERATORS.contains(&name.as_str()) {
                atoms.push(Atom {
                    tex: format!("\\operatorname{{{name}}} "),
                    x0: g.x,
                    x1: last_x1,
                    y: g.y,
                    size: g.size,
                    fixed: false,
                    is_dot: false,
                    is_cdot: false,
                });
                k = j;
                continue;
            }
            // 短自然语言限定词（如 `iff`, `for`, `where`, `const`）直体输出 `\text{...}`
            if name.len() >= 2 && is_math_qualifier(&name) {
                atoms.push(Atom {
                    tex: format!("\\text{{{name}}} "),
                    x0: g.x,
                    x1: last_x1,
                    y: g.y,
                    size: g.size,
                    fixed: false,
                    is_dot: false,
                    is_cdot: false,
                });
                k = j;
                continue;
            }
        }
        let mut chars = t.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && (g.math != MathKind::Extension || is_combining(c))
            && let Some((_, cmd)) = ACCENTS.iter().find(|(a, _)| *a == c)
        {
            accents.push((atoms.len(), cmd, (g.x + g.x_end) / 2.0));
            continue;
        }
        let mut tex = String::new();
        for c in t.chars() {
            push_char(c, g.math, &mut tex);
        }
        let is_dot = t == ".";
        let is_cdot = t == "·" || t == "⋅";
        atoms.push(Atom {
            tex,
            x0: g.x,
            x1: g.x_end.max(g.x),
            y: g.y,
            size: g.size,
            fixed: g.math == MathKind::Extension,
            is_dot,
            is_cdot,
        });
    }
    // 重音附着：优先中心落在目标原子水平范围内（放宽半字号容差，兼容宽重音 `\tilde{a}`、`\hat{x}`）
    for (at, cmd, cx) in accents.into_iter().rev() {
        let pick = [at, at.wrapping_sub(1)]
            .into_iter()
            .filter(|&k| k < atoms.len())
            .find(|&k| cx >= atoms[k].x0 - 0.5 && cx <= atoms[k].x1 + 0.5)
            .or_else(|| {
                [at, at.wrapping_sub(1)]
                    .into_iter()
                    .filter(|&k| k < atoms.len())
                    .min_by(|&a, &b| {
                        let da = (cx - 0.5 * (atoms[a].x0 + atoms[a].x1)).abs();
                        let db = (cx - 0.5 * (atoms[b].x0 + atoms[b].x1)).abs();
                        da.total_cmp(&db)
                    })
                    .filter(|&k| (cx - 0.5 * (atoms[k].x0 + atoms[k].x1)).abs() <= 0.6 * base_size)
            });
        if let Some(k) = pick {
            atoms[k].tex = format!("{cmd}{{{}}}", atoms[k].tex.trim());
        }
    }
    // 折叠连续三个句点 `. . .` → `\ldots`，连续三个居中点 `· · ·` → `\cdots`
    collapse_ellipsis(&mut atoms);

    let mut out = String::new();
    build(&atoms, base_y, base_size, &mut out);
    sanitize_tex(&out)
}

fn collapse_ellipsis(atoms: &mut Vec<Atom>) {
    if atoms.len() < 3 {
        return;
    }
    let mut i = 0;
    while i + 2 < atoms.len() {
        let same_y = (atoms[i].y - atoms[i + 1].y).abs() < 0.25 * atoms[i].size
            && (atoms[i + 1].y - atoms[i + 2].y).abs() < 0.25 * atoms[i].size;
        let close_x = (atoms[i + 1].x0 - atoms[i].x1) < 0.8 * atoms[i].size
            && (atoms[i + 2].x0 - atoms[i + 1].x1) < 0.8 * atoms[i].size;
        if same_y && close_x && atoms[i].is_dot && atoms[i + 1].is_dot && atoms[i + 2].is_dot {
            let x1 = atoms[i + 2].x1;
            atoms[i].tex = "\\ldots ".to_string();
            atoms[i].x1 = x1;
            atoms[i].is_dot = false;
            atoms.drain(i + 1..i + 3);
        } else if same_y
            && close_x
            && atoms[i].is_cdot
            && atoms[i + 1].is_cdot
            && atoms[i + 2].is_cdot
        {
            let x1 = atoms[i + 2].x1;
            atoms[i].tex = "\\cdots ".to_string();
            atoms[i].x1 = x1;
            atoms[i].is_cdot = false;
            atoms.drain(i + 1..i + 3);
        } else {
            i += 1;
        }
    }
}

pub(crate) fn level(a: &Atom, base_y: f64, base_size: f64) -> i32 {
    if a.fixed || a.size > 0.85 * base_size {
        return 0;
    }
    let dy = a.y - base_y;
    if dy > 0.12 * base_size {
        1
    } else if dy < -0.08 * base_size {
        -1
    } else {
        0
    }
}

/// 检查一段同基线原子是否只是导数撇号 `'`/`''`/`'''`。
fn is_prime_tex(tex: &str) -> bool {
    let t = tex.trim();
    !t.is_empty() && t.bytes().all(|b| b == b'\'')
}

pub(crate) fn build(atoms: &[Atom], base_y: f64, base_size: f64, out: &mut String) {
    let mut i = 0;
    let mut have_base = false;
    let (mut has_sup, mut has_sub) = (false, false);
    while i < atoms.len() {
        let lvl = level(&atoms[i], base_y, base_size);
        if lvl == 0 {
            if is_prime_tex(&atoms[i].tex) {
                // 导数撇号属于上标修饰，绝不能重置 `has_sub`（否则 `v_{c}'_{1}` 触发 KaTeX Double Subscript）
                if !have_base || has_sup {
                    out.push_str("{}");
                    has_sub = false;
                }
                out.push_str(atoms[i].tex.trim());
                have_base = true;
                has_sup = true;
            } else {
                out.push_str(&atoms[i].tex);
                have_base = true;
                let t = atoms[i].tex.trim();
                let prebuilt_scripted = atoms[i].fixed
                    && (t.starts_with("\\sum")
                        || t.starts_with("\\prod")
                        || t.starts_with("\\coprod")
                        || t.starts_with("\\int")
                        || t.starts_with("\\iint")
                        || t.starts_with("\\oint")
                        || t.starts_with("\\big")
                        || t.starts_with("\\lim")
                        || t.starts_with("\\sup")
                        || t.starts_with("\\inf")
                        || t.starts_with("\\max")
                        || t.starts_with("\\min"));
                has_sup = t.ends_with('\'') || (prebuilt_scripted && t.contains("^{"));
                has_sub = prebuilt_scripted && t.contains("_{");
            }
            i += 1;
            continue;
        }

        // 收集当前同层级连续组；若后紧跟一个水平重叠的异层级组并被同层级组紧密续接（如 `d_{x}^{(n)}`
        // 中 `(` 在 x=5.0、`x` 在 x=5.5、`n)` 在 x=7.5），按 DRACULAE 2D 区域解缠为同一基元的 `_{sub}^{sup}`
        let mut j = i + 1;
        while j < atoms.len() && level(&atoms[j], base_y, base_size) == lvl {
            j += 1;
        }
        // 检查是否存在被异层级打断但水平紧邻的同层级续接（`A(lvl) + B(-lvl) + C(lvl)`）
        let mut k = j;
        while k < atoms.len() && level(&atoms[k], base_y, base_size) == -lvl {
            k += 1;
        }
        let mut m = k;
        while m < atoms.len() && level(&atoms[m], base_y, base_size) == lvl {
            m += 1;
        }
        let interleaved = k > j
            && m > k
            && atoms[j].x0 < atoms[j - 1].x1
            && (atoms[k].x0 - atoms[j - 1].x1) <= 0.18 * atoms[j - 1].size.max(atoms[k].size);

        if interleaved {
            // 解缠：同一基元同时拥有重叠的上下标（先继续吸纳后续紧邻交错段）
            let mut end = m;
            while end < atoms.len() && level(&atoms[end], base_y, base_size) != 0 {
                let nl = level(&atoms[end], base_y, base_size);
                let prev_same = atoms[i..end]
                    .iter()
                    .rfind(|a| level(a, base_y, base_size) == nl);
                if prev_same
                    .is_some_and(|p| atoms[end].x0 - p.x1 <= 0.18 * p.size.max(atoms[end].size))
                {
                    end += 1;
                } else {
                    break;
                }
            }
            let sup_atoms: Vec<Atom> = atoms[i..end]
                .iter()
                .filter(|a| level(a, base_y, base_size) > 0)
                .map(|a| Atom {
                    tex: a.tex.clone(),
                    x0: a.x0,
                    x1: a.x1,
                    y: a.y,
                    size: a.size,
                    fixed: a.fixed,
                    is_dot: a.is_dot,
                    is_cdot: a.is_cdot,
                })
                .collect();
            let sub_atoms: Vec<Atom> = atoms[i..end]
                .iter()
                .filter(|a| level(a, base_y, base_size) < 0)
                .map(|a| Atom {
                    tex: a.tex.clone(),
                    x0: a.x0,
                    x1: a.x1,
                    y: a.y,
                    size: a.size,
                    fixed: a.fixed,
                    is_dot: a.is_dot,
                    is_cdot: a.is_cdot,
                })
                .collect();
            if !have_base || has_sub || has_sup {
                out.push_str("{}");
                have_base = true;
                has_sub = false;
                has_sup = false;
            }
            if !sub_atoms.is_empty() {
                emit_script(
                    &sub_atoms,
                    -1,
                    base_y,
                    &mut have_base,
                    &mut has_sup,
                    &mut has_sub,
                    out,
                );
            }
            if !sup_atoms.is_empty() {
                emit_script(
                    &sup_atoms,
                    1,
                    base_y,
                    &mut have_base,
                    &mut has_sup,
                    &mut has_sub,
                    out,
                );
            }
            i = end;
            continue;
        }

        let group = &atoms[i..j];
        emit_script(
            group,
            lvl,
            base_y,
            &mut have_base,
            &mut has_sup,
            &mut has_sub,
            out,
        );
        i = j;
    }
}

fn emit_script(
    group: &[Atom],
    lvl: i32,
    base_y: f64,
    have_base: &mut bool,
    has_sup: &mut bool,
    has_sub: &mut bool,
    out: &mut String,
) {
    if lvl > 0 && group.iter().all(|a| is_prime_tex(&a.tex)) && !*has_sup {
        if !*have_base {
            out.push_str("{}");
            *have_base = true;
        }
        for a in group {
            out.push_str(a.tex.trim());
        }
        *has_sup = true;
        return;
    }
    let repeated = if lvl > 0 { *has_sup } else { *has_sub };
    if !*have_base || repeated {
        out.push_str("{}");
        *have_base = true;
        *has_sup = false;
        *has_sub = false;
    }
    if lvl > 0 {
        *has_sup = true;
    } else {
        *has_sub = true;
    }
    let inner = group
        .iter()
        .max_by(|a, b| {
            a.size
                .total_cmp(&b.size)
                .then((b.y - base_y).abs().total_cmp(&(a.y - base_y).abs()))
        })
        .map_or((group[0].y, group[0].size), |a| (a.y, a.size));
    let mut sub = String::new();
    build(group, inner.0, inner.1, &mut sub);
    out.push_str(if lvl > 0 { "^{" } else { "_{" });
    out.push_str(sub.trim());
    out.push('}');
}

/// 清理空白并保证 `{}` 括号配平与无悬空 `^`/`_`，防止 KaTeX 抛出语法错误。
fn sanitize_tex(raw: &str) -> String {
    let mut s = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        return s;
    }
    // 检查未转义 `{` 和 `}` 的嵌套深度
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    let mut bad_close = false;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth < 0 {
                bad_close = true;
                break;
            }
        }
        i += 1;
    }
    if bad_close {
        // 移除导致负深度的多余未转义 `}`
        let mut fixed = String::with_capacity(s.len());
        let mut d = 0i32;
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                fixed.push(c);
                if let Some(n) = chars.next() {
                    fixed.push(n);
                }
                continue;
            }
            if c == '{' {
                d += 1;
                fixed.push(c);
            } else if c == '}' {
                if d > 0 {
                    d -= 1;
                    fixed.push(c);
                }
            } else {
                fixed.push(c);
            }
        }
        while d > 0 {
            fixed.push('}');
            d -= 1;
        }
        s = fixed;
    } else if depth > 0 {
        for _ in 0..depth {
            s.push('}');
        }
    }
    s
}

/// 统计词内未配平的开定界符数（`{`, `(`, `[` 减 `}`, `)`, `]`）。
fn delim_delta(word: &[Item<'_>]) -> i32 {
    let mut d = 0i32;
    for item in word {
        for c in item.text.chars() {
            match c {
                '{' | '(' | '[' => d += 1,
                '}' | ')' | ']' => d -= 1,
                _ => {}
            }
        }
    }
    d
}

/// 一行的文字。没有公式时返回 None，调用方照普通文字拼。
pub(crate) fn line_text(words: &[Vec<Item<'_>>], base_y: f64, base_size: f64) -> Option<String> {
    let mut kinds: Vec<Word> = words.iter().map(|w| classify(w)).collect();
    // 词尾上标的脚注标记（`Vaswani∗`）是普通文字，不是公式
    let mut footnote = vec![false; words.len()];
    for i in 0..words.len() {
        if kinds[i] == Word::Math {
            let after_text = i > 0 && kinds[i - 1] == Word::Text && !footnote[i - 1];
            if footnote::is_marker_word(&words[i], base_y, base_size, after_text) {
                kinds[i] = Word::Text;
                footnote[i] = true;
            }
        }
    }
    let word_text = |i: usize| {
        if footnote[i] {
            footnote::plain(&words[i])
        } else {
            plain(&words[i])
        }
    };
    if !kinds.contains(&Word::Math) {
        if !footnote.contains(&true) {
            return None;
        }
        let mut out = String::new();
        for i in 0..words.len() {
            push_word(&mut out, &word_text(i));
        }
        return Some(out);
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if kinds[i] != Word::Math {
            i += 1;
            continue;
        }
        let mut start = i;
        while start > 0 && kinds[start - 1] == Word::Glue {
            start -= 1;
        }
        let mut end = i + 1;
        let mut open_delims: i32 = words[start..end].iter().map(|w| delim_delta(w)).sum();
        loop {
            let mut j = end;
            while j < words.len() && kinds[j] == Word::Glue {
                j += 1;
            }
            if j < words.len() && kinds[j] == Word::Math {
                for w in &words[end..=j] {
                    open_delims += delim_delta(w);
                }
                end = j + 1;
                continue;
            }
            // 未闭合括号内或两段公式之间的短限定词（如 `a.a.`, `for`, `iff`, `if`, `const`）桥接进公式
            if j < words.len() && kinds[j] == Word::Text && !footnote[j] {
                let raw = plain(&words[j]);
                let raw_t = raw.trim().trim_end_matches([',', ';', ':']);
                let qual = is_math_qualifier(raw_t);
                let short_inside_brace = open_delims > 0 && raw_t.len() <= 8;
                if qual || short_inside_brace {
                    let mut m = j + 1;
                    while m < words.len() && kinds[m] == Word::Glue {
                        m += 1;
                    }
                    // 若紧跟另一段数学，或者本身是 `const` 且前面有 `+`/`=` 算符，则并入
                    if (m < words.len() && kinds[m] == Word::Math) || raw_t == "const" {
                        let new_end = if m < words.len() && kinds[m] == Word::Math {
                            m + 1
                        } else {
                            j + 1
                        };
                        for w in &words[end..new_end] {
                            open_delims += delim_delta(w);
                        }
                        end = new_end;
                        continue;
                    }
                }
            }
            end = j;
            break;
        }
        while start < end && punct_only(&words[start]) {
            start += 1;
        }
        while end > start && punct_only(&words[end - 1]) {
            end -= 1;
        }
        if words[start..end].iter().flatten().any(evidence) {
            spans.push((start, end));
        }
        i = end.max(i + 1);
    }
    if spans.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut w = 0;
    for (start, end) in spans {
        while w < start {
            push_word(&mut out, &word_text(w));
            w += 1;
        }
        let items: Vec<&Item> = words[start..end].iter().flatten().collect();
        let mut cut = items.len();
        // 剥离公式末尾的句子标点，但保留连续省略号 `. . .`（否则会破坏末尾 `\ldots`）
        while cut > 0 {
            let t = items[cut - 1].text.trim();
            if t == "."
                && cut >= 3
                && items[cut - 2].text.trim() == "."
                && items[cut - 3].text.trim() == "."
            {
                break;
            }
            if matches!(t, "," | "." | ";" | ":") {
                cut -= 1;
            } else {
                break;
            }
        }
        let tail: String = items[cut..].iter().map(|i| i.text.trim()).collect();
        // 对整段公式统一建树（保留词间空隙），避免跨词重置上下标状态和拆散 `lim inf` / `...`
        let mut parts: Vec<String> = Vec::new();
        let mut taken = 0;
        let mut span_refs: Vec<&Item> = Vec::with_capacity(cut);
        // 检测是否有需要按词切开保持空格的普通单词限定词
        let has_word_break_qual = words[start..end].iter().any(|wd| {
            let p = plain(wd);
            is_math_qualifier(p.trim())
        });
        if !has_word_break_qual
            && words[start..end].len() > 1
            && words[start..end].windows(2).any(|pair| {
                let a_last = pair[0].last();
                let b_first = pair[1].first();
                match (a_last, b_first) {
                    (Some(a), Some(b)) => {
                        let p0 = plain(&pair[0]);
                        let p1 = plain(&pair[1]);
                        let is_lim_pair =
                            p0.ends_with("lim") && (p1.starts_with("inf") || p1.starts_with("sup"));
                        let is_dot_pair = a.text.trim() == "." && b.text.trim() == ".";
                        let is_cdot_pair = matches!(a.text.trim(), "·" | "⋅")
                            && matches!(b.text.trim(), "·" | "⋅");
                        let is_script_attach = level(
                            &Atom {
                                tex: String::new(),
                                x0: b.g.x,
                                x1: b.g.x_end,
                                y: b.g.y,
                                size: b.g.size,
                                fixed: b.g.math == MathKind::Extension,
                                is_dot: false,
                                is_cdot: false,
                            },
                            base_y,
                            base_size,
                        ) != 0;
                        let is_neg = a.text.trim() == "\u{0338}";
                        is_lim_pair || is_dot_pair || is_cdot_pair || is_script_attach || is_neg
                    }
                    _ => false,
                }
            })
        {
            for it in &items[..cut] {
                span_refs.push(*it);
            }
            let tex = tex_of(&span_refs, base_y, base_size);
            if !tex.is_empty() {
                parts.push(tex);
            }
        } else {
            for word in &words[start..end] {
                let n = word.len().min(cut.saturating_sub(taken));
                let refs: Vec<&Item> = word[..n].iter().collect();
                taken += word.len();
                let tex = tex_of(&refs, base_y, base_size);
                if !tex.is_empty() {
                    parts.push(tex);
                }
            }
        }
        let tex = sanitize_tex(&parts.join(" "));
        if tex.trim().is_empty() {
            push_word(&mut out, &plain_items(&items));
        } else {
            push_word(&mut out, &format!("${}${tail}", tex.trim()));
        }
        w = end;
    }
    while w < words.len() {
        push_word(&mut out, &word_text(w));
        w += 1;
    }
    Some(out)
}
