//! 基于二态维特比动态规划（Viterbi / HMM, Rabiner 1989; Garain & Chaudhuri 2005）的行内数学区间最优切分。
//!
//! 将一行上的词序列 $w_0, \dots, w_{n-1}$ 建模为隐马尔可夫二态解码问题（`Text` vs `Math`）：
//! - 发射对数似然 $E(w_t, s)$ 综合字形字体类别（`MathKind`）、粗体单字母向量（`\mathbf{T}`）、
//!   直体希腊字母（`Ω`, `Δ`）、算符/数字/定界符、标准函数名与自然语言散文词惩罚；
//! - 转移对数似然 $T(s_{t-1} \to s_t)$ 惩罚在公式内部碎片化切换（避免 `$T$ $\simeq$ $N$` 或 `Ω_1 $= [0,2\pi]$` 撕裂），
//!   同时借助全局最优路径避免把英文长句误吸入数学模式。

use super::symbols::is_func_or_op;
use super::tex::{Item, Word, evidence, plain, punct_only};
use crate::font::MathKind;

/// 公式内部常见的短自然语言限定词（出现在两段公式之间或未闭合括号内时允许桥接）。
pub(crate) fn is_math_qualifier(s: &str) -> bool {
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

/// 判断一个 `Text` 词是不是像数学里的单字母变量/粗体向量（如 `\mathbf{T}`, `N`, `A`, `B`, `f`），
/// 而不是英语冠词/代词（`a`, `I`）或普通多字母英文词。
fn is_math_like_single_var(word: &[Item<'_>]) -> bool {
    let raw = plain(word);
    let clean = raw.trim_matches(|c: char| "()[]{},.;:!".contains(c));
    let mut cs = clean.chars();
    let (Some(ch), None) = (cs.next(), cs.next()) else {
        return false;
    };
    if !ch.is_ascii_alphabetic() {
        return false;
    }
    let has_bold = word.iter().any(|it| it.g.bold);
    if has_bold {
        return true;
    }
    // 非英语单字母冠词/代词（排除孤立 `a`、`A`、`I`），仅在紧邻算符/数学时由转移吸收
    !matches!(ch, 'a' | 'A' | 'I')
}

/// 判断词是否包含强数学二元关系/运算符（`=`, `\simeq`, `\leq`, `\in`, `\to`, `+`, `−` 等）。
fn has_rel_or_op(word: &[Item<'_>]) -> bool {
    word.iter().any(|it| {
        it.g.math == MathKind::Prebuilt
            || it.text.chars().any(|c| {
                matches!(
                    c,
                    '=' | '<'
                        | '>'
                        | '+'
                        | '−'
                        | '≤'
                        | '≥'
                        | '≠'
                        | '≈'
                        | '≡'
                        | '≃'
                        | '∼'
                        | '∈'
                        | '∉'
                        | '⊂'
                        | '⊆'
                        | '→'
                        | '←'
                        | '↔'
                        | '⇒'
                        | '⇔'
                        | '↦'
                        | '↪'
                        | '×'
                        | '·'
                        | '⋅'
                        | '±'
                        | '∓'
                        | '⊗'
                        | '⊕'
                        | '∩'
                        | '∪'
                        | '⊥'
                        | '∥'
                        | '∶'
                        | '≔'
                )
            })
    })
}

/// 统计词内未配平的开定界符数（`{`, `(`, `[` 减 `}`, `)`, `]`）以及单独的大括号 `{` 净余额。
fn delim_delta(word: &[Item<'_>]) -> (i32, i32) {
    let mut all = 0i32;
    let mut brace = 0i32;
    for item in word {
        for c in item.text.chars() {
            match c {
                '{' => {
                    all += 1;
                    brace += 1;
                }
                '}' => {
                    all -= 1;
                    brace -= 1;
                }
                '(' | '[' => all += 1,
                ')' | ']' => all -= 1,
                _ => {}
            }
        }
    }
    (all, brace)
}

/// 识别正文字体中的常见物理单位/英文复合词缀（如 `μM`、`μm`、`μL`、`μg`、`α-helix`、`90%`），
/// 防止同行存在其他公式时把正文单位误吸入 `$...$`。
fn is_plain_unit_or_prose_greek(trimmed: &str) -> bool {
    let clean = trimmed.trim_matches(|c: char| "()[]{},.;:!".contains(c));
    if clean.contains('%') {
        return true;
    }
    if matches!(
        clean,
        "μm" | "μM"
            | "μL"
            | "μl"
            | "μg"
            | "μs"
            | "μV"
            | "μA"
            | "μW"
            | "μN"
            | "μmol"
            | "µm"
            | "µM"
            | "µL"
            | "µl"
            | "µg"
            | "µs"
            | "µV"
            | "µA"
            | "µW"
            | "µmol"
    ) {
        return true;
    }
    // 形如 `α-helix`、`β-sheet`、`γ-ray` 的普通英文复合词
    let ascii_letters = clean.chars().filter(|c| c.is_ascii_alphabetic()).count();
    clean.contains('-') && ascii_letters >= 3
}

/// 判断词是否包含普通非粗体英文散文词（连续 >= 2 个 ASCII 字母且非数学函数/限定词），
/// 用于阻止未配平圆括号前瞻把 `($x$ is fast)` 吞进公式，同时放行 `t+0)`、`t))`、`\mathbf{Fin})` 等真实参数。
fn has_plain_english_word(word: &[Item<'_>], trimmed: &str) -> bool {
    if word
        .iter()
        .any(|it| it.g.bold || it.g.math != MathKind::Text)
    {
        return false;
    }
    let mut run = String::new();
    let check = |r: &str| r.len() >= 2 && !is_func_or_op(r) && !is_math_qualifier(r);
    for c in trimmed.chars() {
        if c.is_ascii_alphabetic() {
            run.push(c);
        } else {
            if check(&run) {
                return true;
            }
            run.clear();
        }
    }
    check(&run)
}

/// 用二态 Viterbi DP 计算一行上的数学区间 `[start, end)` 列表。
pub(crate) fn segment_math_spans(
    words: &[Vec<Item<'_>>],
    kinds: &[Word],
    footnote: &[bool],
) -> Vec<(usize, usize)> {
    let n = words.len();
    if n == 0 || !kinds.contains(&Word::Math) {
        return Vec::new();
    }

    // 1) 为每个词计算发射分数 (score_text, score_math)
    let mut e_text = vec![0.0f64; n];
    let mut e_math = vec![0.0f64; n];

    for i in 0..n {
        if footnote[i] {
            e_text[i] = 100.0;
            e_math[i] = -100.0;
            continue;
        }
        let raw = plain(&words[i]);
        let trimmed = raw.trim();
        // 裸货币 `$` 硬约束留在正文
        if trimmed.contains('$') {
            e_text[i] = 100.0;
            e_math[i] = -100.0;
            continue;
        }
        match kinds[i] {
            Word::Math => {
                // 强数学词：数学字体、TeX 大写希腊字母或 Prebuilt 2D 表达式
                let strong = words[i].iter().any(evidence);
                if strong {
                    e_math[i] = 8.0;
                    e_text[i] = -4.0;
                } else {
                    // 正文字体里的物理单位/百分数比较符（如 `250 μM`、`≥ 90%`、`α-helix`）强制留正文，
                    // 非 TeX PDF 正文字体中的常规希腊变量/数学符号（如 `π_ii(t_0)`、`η(t_0)`）仍保持数学倾向
                    let next_has_pct = i + 1 < n && plain(&words[i + 1]).contains('%');
                    if is_plain_unit_or_prose_greek(trimmed) || next_has_pct {
                        e_text[i] = 4.5;
                        e_math[i] = -4.5;
                    } else {
                        e_math[i] = 3.5;
                        e_text[i] = -4.0;
                    }
                }
            }
            Word::Glue => {
                if punct_only(&words[i]) {
                    e_text[i] = 0.4;
                    e_math[i] = 0.0;
                } else if has_rel_or_op(&words[i]) {
                    // 运算符/等号/比较符倾向留在数学块内
                    e_text[i] = -0.3;
                    e_math[i] = 1.6;
                } else {
                    // 纯数字、括号或标准函数名（sin/log/det）：取决于上下文状态
                    let alpha = trimmed.trim_matches(|c: char| "()[]{}|/,.:;".contains(c));
                    if !alpha.is_empty()
                        && alpha.chars().all(|c| c.is_ascii_alphabetic())
                        && is_func_or_op(alpha)
                    {
                        e_text[i] = 0.2;
                        e_math[i] = 0.8;
                    } else {
                        e_text[i] = 0.2;
                        e_math[i] = 0.6;
                    }
                }
            }
            Word::Text => {
                let raw_t = trimmed.trim_end_matches([',', ';', ':']);
                // 检查单字母变量/粗体向量是否紧邻强算符/数学词（如 `T \simeq N`、`f : R -> R`）
                let adj_op = (i > 0 && has_rel_or_op(&words[i - 1]))
                    || (i + 1 < n && has_rel_or_op(&words[i + 1]));
                let adj_bold_math = words[i].iter().any(|it| it.g.bold)
                    && raw_t.chars().count() == 1
                    && raw_t.chars().all(|c| c.is_ascii_alphabetic())
                    && ((i > 0 && kinds[i - 1] != Word::Text)
                        || (i + 1 < n && kinds[i + 1] != Word::Text));

                if adj_bold_math || (adj_op && is_math_like_single_var(&words[i])) {
                    e_text[i] = -0.5;
                    e_math[i] = 2.4;
                } else if is_math_qualifier(raw_t) {
                    // 数学限定词（for, if, iff, and, or, where, a.e., const）：
                    // 仅当两侧均有数学（或 `const` 紧接算符）时通过转移惩罚自然桥接，不在普通句尾吸入
                    let prev_math = i > 0 && kinds[i - 1] != Word::Text;
                    let next_math = i + 1 < n && kinds[i + 1] != Word::Text;
                    if prev_math && next_math {
                        e_text[i] = 1.0;
                        e_math[i] = 0.4;
                    } else if raw_t == "const" && prev_math {
                        e_text[i] = 0.0;
                        e_math[i] = 1.2;
                    } else {
                        e_text[i] = 6.0;
                        e_math[i] = -6.0;
                    }
                } else if words[i].iter().any(evidence) && !is_plain_unit_or_prose_greek(trimmed) {
                    e_text[i] = -2.0;
                    e_math[i] = 4.0;
                } else {
                    // 普通自然语言英文词（含 `α-helix`、`90%` 等）：强烈倾向 Text，防止误吞散文
                    let word_len = raw_t.chars().filter(|c| c.is_alphabetic()).count();
                    let pen = if word_len >= 4 { 12.0 } else { 6.5 };
                    e_text[i] = pen;
                    e_math[i] = -pen;
                }
            }
        }
    }

    // 2) 二态 Viterbi 动态规划（状态 0 = Text，状态 1 = Math）
    // 切换代价 SW = 1.8：
    // - 单个 Glue/限定词夹在两段 Math 之间时，保持 Math 省去 2*SW = 3.6 的切换代价，自动桥接；
    // - 连续普通散文词（e_text - e_math >= 13.0 >> 3.6）必然切回 Text。
    const SW: f64 = 1.8;
    let mut dp = vec![[f64::NEG_INFINITY; 2]; n];
    let mut back = vec![[0usize; 2]; n];

    dp[0][0] = e_text[0];
    dp[0][1] = e_math[0] - 0.3; // 轻微先验偏置 Text，避免孤立 Glue 开头进入 Math

    for i in 1..n {
        // 如果前一个词以句末标点 `.` `;` 结尾且不是省略号，降低跨句桥接倾向
        let prev_raw = plain(&words[i - 1]);
        let prev_t = prev_raw.trim();
        let sentence_break = (prev_t.ends_with('.') && !prev_t.ends_with(".."))
            || prev_t.ends_with(';')
            || prev_t.ends_with('?');
        let keep_math_pen = if sentence_break { 3.2 } else { 0.0 };

        for cur in 0..2 {
            let em = if cur == 0 { e_text[i] } else { e_math[i] };
            let mut best_v = f64::NEG_INFINITY;
            let mut best_p = 0usize;
            for (prev, &prev_dp) in dp[i - 1].iter().enumerate() {
                let trans = if prev == cur {
                    if cur == 1 { -keep_math_pen } else { 0.0 }
                } else {
                    -SW
                };
                let v = prev_dp + trans + em;
                if v > best_v {
                    best_v = v;
                    best_p = prev;
                }
            }
            dp[i][cur] = best_v;
            back[i][cur] = best_p;
        }
    }

    // 回溯最优状态序列
    let mut states = vec![0usize; n];
    states[n - 1] = if dp[n - 1][1] > dp[n - 1][0] { 1 } else { 0 };
    for i in (1..n).rev() {
        states[i - 1] = back[i][states[i]];
    }

    // 3) 括号配平修复：
    // - 集合推导花括号 `{ ... }` 允许桥接最多 3 个短自然语言条件词直至 `}`（如 `\{ x : x is even \}`）；
    // - 圆/方括号 `( ... )` / `[ ... ]` 只桥接到非正文（Math/Glue）的闭合词，禁止把 `($x$ is fast)` 吞成数学！
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if states[i] == 0 {
            i += 1;
            continue;
        }
        let mut start = i;
        let mut end = i + 1;
        while end < n && states[end] == 1 {
            end += 1;
        }
        let (mut open_d, mut open_b) = words[start..end].iter().fold((0i32, 0i32), |(a, b), w| {
            let (da, db) = delim_delta(w);
            (a + da, b + db)
        });
        while open_d > 0 && end < n {
            let look_lim = (end + 3).min(n);
            let mut found = None;
            let mut acc_d = open_d;
            let mut acc_b = open_b;
            for j in end..look_lim {
                if footnote[j] || plain(&words[j]).contains('$') {
                    break;
                }
                let raw_t = plain(&words[j]);
                let raw_trimmed = raw_t.trim();
                if states[j] == 0 && raw_trimmed.chars().count() > 10 {
                    break;
                }
                let (da, db) = delim_delta(&words[j]);
                acc_d += da;
                acc_b += db;
                let is_prose_stop = is_plain_unit_or_prose_greek(raw_trimmed)
                    || (open_b <= 0
                        && kinds[j] == Word::Text
                        && has_plain_english_word(&words[j], raw_trimmed));
                if is_prose_stop {
                    break;
                }
                let closed = if open_b > 0 {
                    // 未闭合花括号：允许短词直到闭合 `}`（但禁止吞入正文字体物理单位如 `μM`）
                    acc_b <= 0 && acc_d <= 0
                } else {
                    // 未闭合 `(`/`[`：允许 `t+0)`、`t))`、`\mathbf{Fin})` 等参数闭合，排除正文散文与物理单位
                    acc_d <= 0
                };
                if closed {
                    found = Some(j + 1);
                    break;
                }
            }
            if let Some(mut new_end) = found {
                while new_end < n && states[new_end] == 1 {
                    new_end += 1;
                }
                let (na, nb) = words[start..new_end]
                    .iter()
                    .fold((0i32, 0i32), |(a, b), w| {
                        let (da, db) = delim_delta(w);
                        (a + da, b + db)
                    });
                open_d = na;
                open_b = nb;
                end = new_end;
            } else {
                break;
            }
        }
        // 剥离尾部未闭合的纯 Glue 开括号词（如 `$\Delta > 0$ (250 μM)` 中误吸入的 `(250`）
        while end > start
            && kinds[end - 1] == Word::Glue
            && !has_rel_or_op(&words[end - 1])
            && delim_delta(&words[end - 1]).0 > 0
        {
            end -= 1;
        }
        while start < end && punct_only(&words[start]) {
            start += 1;
        }
        while end > start && punct_only(&words[end - 1]) {
            end -= 1;
        }
        // 至少包含实质数学证据（数学字体、TeX 大写希腊字母或 Prebuilt 结构）
        if start < end && words[start..end].iter().flatten().any(evidence) {
            spans.push((start, end));
        }
        i = end.max(i + 1);
    }
    spans
}
