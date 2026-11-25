//! 数学公式解析与 LaTeX 重构引擎。
//! 包含数学字体识别、TeX 命令映射、上下标多级分层树与行内/行间公式渲染。

pub(crate) mod big_ops;
pub(crate) mod footnote;
pub(crate) mod matrix;
pub(crate) mod segment;
pub(crate) mod spatial;
pub(crate) mod symbols;
pub(crate) mod tex;

pub(crate) use tex::{Item, line_text};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::MathKind;
    use crate::interp::Glyph;
    use symbols::math_alnum;

    fn glyph(text: &str, x: f64, y: f64, size: f64, math: MathKind) -> Glyph {
        Glyph {
            text: text.into(),
            x,
            y,
            x_end: x + 0.5 * size * text.chars().count() as f64,
            size,
            bold: false,
            math,
            mcid: None,
            artifact: false,
        }
    }

    fn line(words: &[&[Glyph]]) -> Option<String> {
        let words: Vec<Vec<Item>> = words
            .iter()
            .map(|w| {
                w.iter()
                    .map(|g| Item {
                        g,
                        text: g.text.clone(),
                    })
                    .collect()
            })
            .collect();
        line_text(&words, 100.0, 10.0)
    }

    use MathKind::{Italic, Symbol, Text};

    #[test]
    fn superscript_and_relations() {
        let w = [
            vec![glyph("for", 0.0, 100.0, 10.0, Text)],
            vec![glyph("0", 20.0, 100.0, 10.0, Text)],
            vec![glyph("≤", 30.0, 100.0, 10.0, Symbol)],
            vec![glyph("k", 40.0, 100.0, 10.0, Italic)],
            vec![glyph("≤", 50.0, 100.0, 10.0, Symbol)],
            vec![
                glyph("2", 60.0, 100.0, 10.0, Text),
                glyph("N", 65.0, 103.6, 7.0, Italic),
            ],
            vec![
                glyph("−", 72.0, 100.0, 10.0, Symbol),
                glyph("1", 78.0, 100.0, 10.0, Text),
                glyph(",", 83.0, 100.0, 10.0, Italic),
            ],
            vec![glyph("we", 90.0, 100.0, 10.0, Text)],
        ];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "for $0 \\leq k \\leq 2^{N} -1$, we");
    }

    #[test]
    fn subscripts_nested_and_fonts() {
        let w = [vec![
            glyph("x", 0.0, 100.0, 10.0, Italic),
            glyph("i", 5.0, 98.5, 7.0, Italic),
            glyph("j", 8.0, 97.5, 5.0, Italic),
            glyph("∈", 15.0, 100.0, 10.0, Symbol),
            glyph("R", 25.0, 100.0, 10.0, MathKind::Blackboard),
            glyph("F", 32.0, 100.0, 10.0, Symbol),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(
            line(&refs).unwrap(),
            "$x_{i_{j}}\\in \\mathbb{R}\\mathcal{F}$"
        );
    }

    #[test]
    fn plain_text_is_untouched() {
        let w = [
            vec![glyph("Price", 0.0, 100.0, 10.0, Text)],
            vec![glyph("→", 30.0, 100.0, 10.0, Text)],
            vec![glyph("2019", 40.0, 100.0, 10.0, Text)],
        ];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs), None);
    }

    #[test]
    fn primes_and_accents() {
        let w = [vec![
            glyph("f", 0.0, 100.0, 10.0, Italic),
            glyph("′", 5.0, 103.0, 7.0, Symbol),
            glyph("ˆ", 10.0, 100.0, 10.0, Text),
            glyph("x", 10.0, 100.0, 10.0, Italic),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$f'\\hat{x}$");
    }

    #[test]
    fn function_names() {
        let w = [vec![
            glyph("c", 0.0, 100.0, 10.0, Text),
            glyph("o", 5.0, 100.0, 10.0, Text),
            glyph("s", 10.0, 100.0, 10.0, Text),
            glyph("θ", 16.0, 100.0, 10.0, Italic),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$\\cos \\theta$");
    }

    #[test]
    fn repeated_scripts_get_a_new_base() {
        let w = [vec![
            glyph("t", 0.0, 100.0, 10.0, Italic),
            glyph("2", 5.0, 103.6, 7.0, Text),
            glyph("1", 8.0, 98.5, 7.0, Text),
            glyph("n", 11.0, 103.6, 7.0, Italic),
            glyph("S", 20.0, 100.0, 10.0, Italic),
            glyph("i", 25.0, 98.5, 7.0, Italic),
            glyph("′", 28.0, 103.0, 7.0, Symbol),
            glyph("j", 31.0, 98.5, 7.0, Italic),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$t^{2}_{1}{}^{n}S_{i}'{}_{j}$");
    }

    /// 作者名后面上标的星号、剑号是脚注引用（arXiv 1706.03762 的作者行），不是公式。
    #[test]
    fn footnote_markers_after_names_are_not_math() {
        let w = [
            vec![glyph("Ashish", 0.0, 100.0, 10.0, Text)],
            vec![
                glyph("Vaswani", 35.0, 100.0, 10.0, Text),
                glyph("∗", 70.0, 103.6, 7.0, Symbol),
            ],
            vec![glyph("Noam", 80.0, 100.0, 10.0, Text)],
            vec![
                glyph("Shazeer", 105.0, 100.0, 10.0, Text),
                glyph("†", 140.0, 103.6, 7.0, Symbol),
            ],
            // 标记和名字之间有空格
            vec![glyph("Niki", 150.0, 100.0, 10.0, Text)],
            vec![glyph("Parmar", 175.0, 100.0, 10.0, Text)],
            vec![glyph("∗", 210.0, 103.6, 7.0, Symbol)],
        ];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(
            line(&refs).unwrap(),
            "Ashish Vaswani* Noam Shazeer† Niki Parmar *"
        );
    }

    /// 同样的符号在基线上、和公式字体的变量连在一起时仍是公式。
    #[test]
    fn baseline_asterisk_and_math_superscripts_stay_math() {
        let w = [vec![
            glyph("x", 0.0, 100.0, 10.0, Italic),
            glyph("∗", 5.0, 100.0, 10.0, Symbol),
            glyph("y", 12.0, 100.0, 10.0, Italic),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$x\\ast y$");
        let w = [vec![
            glyph("f", 0.0, 100.0, 10.0, Italic),
            glyph("∗", 5.0, 103.6, 7.0, Symbol),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$f^{\\ast}$");
        // 单独的上标数字跟在文字后面可能是指数，照旧
        let w = [
            vec![glyph("size", 0.0, 100.0, 10.0, Text)],
            vec![glyph("2", 25.0, 103.6, 7.0, Symbol)],
        ];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert!(line(&refs).unwrap().contains("^{2}"));
    }

    #[test]
    fn math_alphanumerics() {
        assert_eq!(math_alnum('𝑥'), Some((None, 'x')));
        assert_eq!(math_alnum('𝔽'), Some((Some("\\mathbb"), 'F')));
        assert_eq!(math_alnum('𝟏'), Some((Some("\\mathbf"), '1')));
    }

    #[test]
    fn disentangles_overlapping_sub_and_sup() {
        // d_x^{(n)}: '(' at x=5..7.5, 'x' at x=5.5..9.0, 'n)' at x=7.5..12.5
        let mut lp = glyph("(", 5.0, 103.6, 7.0, Text);
        lp.x_end = 7.5;
        let mut sub_x = glyph("x", 5.5, 98.5, 7.0, Italic);
        sub_x.x_end = 9.0;
        let mut sup_n = glyph("n)", 7.5, 103.6, 7.0, Italic);
        sup_n.x_end = 12.5;
        let w = [vec![glyph("d", 0.0, 100.0, 10.0, Italic), lp, sub_x, sup_n]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$d_{x}^{(n)}$");
    }

    #[test]
    fn baseline_prime_does_not_cause_double_subscript() {
        // v_{c}'{}_{1}: prime has size=10 (lvl 0) and must preserve has_sub
        let w = [vec![
            glyph("v", 0.0, 100.0, 10.0, Italic),
            glyph("c", 5.0, 98.5, 7.0, Italic),
            glyph("′", 9.0, 100.0, 10.0, Symbol),
            glyph("1", 13.0, 98.5, 7.0, Text),
        ]];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$v_{c}'{}_{1}$");
    }

    #[test]
    fn ellipsis_and_negation_and_split_liminf() {
        let w = [
            vec![glyph("lim", 0.0, 100.0, 10.0, Text)],
            vec![
                glyph("inf", 18.0, 100.0, 10.0, Text),
                glyph("x", 35.0, 100.0, 10.0, Italic),
                glyph("\u{0338}", 42.0, 100.0, 10.0, Symbol),
                glyph("=", 42.0, 100.0, 10.0, Text),
                glyph(".", 50.0, 100.0, 10.0, Text),
                glyph(".", 54.0, 100.0, 10.0, Text),
                glyph(".", 58.0, 100.0, 10.0, Text),
            ],
        ];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$\\liminf x\\neq \\ldots$");
    }

    #[test]
    fn spatial_handles_extreme_coords_and_multiglyph_lim() {
        // Extreme y = 1e20 must not overflow i32 in YBuckets::slot
        let mut extreme = vec![
            glyph("x", 10.0, 1e20, 10.0, Italic),
            glyph("y", 20.0, -1e20, 10.0, Italic),
            glyph("z", 30.0, f64::NAN, 10.0, Italic),
        ];
        let mut rules = vec![crate::geom::Rule {
            x0: 10.0,
            y0: 1e20,
            x1: 30.0,
            y1: 1e20,
        }];
        spatial::reconstruct(&mut extreme, &mut rules, 600.0, 800.0);

        // Multi-glyph "l"+"i"+"m" with close subscript (y - 0.28*body) must not self-reject
        let mk = |t: &str, x: f64, x_end: f64, y: f64, sz: f64, m: MathKind| Glyph {
            text: t.into(),
            x,
            y,
            x_end,
            size: sz,
            bold: false,
            math: m,
            mcid: None,
            artifact: false,
        };
        let mut glyphs = vec![
            mk("l", 100.0, 103.0, 500.0, 10.0, Text),
            mk("i", 103.0, 106.0, 500.0, 10.0, Text),
            mk("m", 106.0, 114.0, 500.0, 10.0, Text),
            mk("n", 101.0, 105.0, 497.2, 7.0, Italic),
            mk("→", 105.5, 110.0, 497.2, 7.0, Symbol),
            mk("∞", 110.5, 115.0, 497.2, 7.0, Symbol),
        ];
        let mut no_rules = Vec::new();
        spatial::reconstruct(&mut glyphs, &mut no_rules, 600.0, 800.0);
        assert!(
            glyphs
                .iter()
                .any(|g| g.math == MathKind::Prebuilt && g.text == "\\lim_{n\\to \\infty}"),
            "multi-glyph lim must attach close subscript: {glyphs:?}"
        );
    }

    #[test]
    fn bold_math_uppercase_greek_and_plain_units_stay_clean() {
        // Bold upright vectors flanking math relation: \mathbf{T} \simeq \mathbf{N}
        let mut t_bold = glyph("T", 0.0, 100.0, 10.0, Text);
        t_bold.bold = true;
        let mut n_bold = glyph("N", 25.0, 100.0, 10.0, Text);
        n_bold.bold = true;
        let w = [
            vec![t_bold],
            vec![glyph("≃", 12.0, 100.0, 10.0, Symbol)],
            vec![n_bold],
        ];
        let refs: Vec<&[Glyph]> = w.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&refs).unwrap(), "$\\mathbf{T} \\simeq \\mathbf{N}$");

        // Plain prose units (`250 μM`, `≥ 90%`) in Text font must NOT turn into LaTeX math
        let u1 = [
            vec![glyph("250", 0.0, 100.0, 10.0, Text)],
            vec![
                glyph("μ", 20.0, 100.0, 10.0, Text),
                glyph("M", 26.0, 100.0, 10.0, Text),
            ],
        ];
        let r1: Vec<&[Glyph]> = u1.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&r1), None);

        let u2 = [
            vec![glyph("≥", 0.0, 100.0, 10.0, Text)],
            vec![glyph("90%", 12.0, 100.0, 10.0, Text)],
        ];
        let r2: Vec<&[Glyph]> = u2.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&r2), None);
    }

    #[test]
    fn knuth_appendix_g_fraction_and_radical_exclude_flanking_parens() {
        // (1 + \frac{2}{3}): closing ')' sits at equation baseline y=100.0 right beside bar x1=114.0
        let mk = |t: &str, x: f64, x_end: f64, y: f64, sz: f64, m: MathKind| Glyph {
            text: t.into(),
            x,
            y,
            x_end,
            size: sz,
            bold: false,
            math: m,
            mcid: None,
            artifact: false,
        };
        let mut glyphs = vec![
            mk("(", 90.0, 94.0, 100.0, 10.0, Text),
            mk("1", 94.5, 99.0, 100.0, 10.0, Text),
            mk("+", 99.5, 104.5, 100.0, 10.0, Symbol),
            mk("2", 107.0, 111.0, 104.5, 7.0, Text),
            mk("3", 107.0, 111.0, 95.5, 7.0, Text),
            // Placed inside r.x1 + 0.12*body so horizontal check passes and Knuth ascender guard must reject it
            mk(")", 112.8, 116.2, 100.0, 10.0, Text),
        ];
        let mut rules = vec![crate::geom::Rule {
            x0: 105.0,
            y0: 102.5,
            x1: 113.5,
            y1: 102.5,
        }];
        spatial::reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(
            glyphs
                .iter()
                .any(|g| g.math == MathKind::Prebuilt && g.text == "\\frac{2}{3}"),
            "flanking ')' must not be swallowed into denominator: {glyphs:?}"
        );
        assert!(
            glyphs.iter().any(|g| g.text == ")"),
            "closing ')' must remain intact: {glyphs:?}"
        );

        // (0.098, \sqrt{0.098}): comma before surd and closing ')' after bar stay outside \sqrt
        let mut rg = vec![
            mk(",", 97.0, 99.5, 100.0, 10.0, Text),
            mk("√", 100.0, 107.0, 100.0, 10.0, Symbol),
            mk("0", 107.5, 112.0, 100.0, 10.0, Text),
            mk("9", 112.0, 116.5, 100.0, 10.0, Text),
            mk(")", 117.5, 121.0, 100.0, 10.0, Text),
        ];
        let mut rr = vec![crate::geom::Rule {
            x0: 107.0,
            y0: 109.2,
            x1: 116.8,
            y1: 109.2,
        }];
        spatial::reconstruct(&mut rg, &mut rr, 600.0, 800.0);
        assert!(
            rg.iter()
                .any(|g| g.math == MathKind::Prebuilt && g.text == "\\sqrt{09}"),
            "radical must not swallow ',' or ')': {rg:?}"
        );
        assert!(rg.iter().any(|g| g.text == ",") && rg.iter().any(|g| g.text == ")"));
    }

    #[test]
    fn reconstructs_fraction_radical_and_cmex_sum() {
        let g = |text: &str, x: f64, y: f64, size: f64, math: MathKind| Glyph {
            text: text.into(),
            x,
            y,
            x_end: x + 0.55 * size * text.chars().count().max(1) as f64,
            size,
            bold: false,
            math,
            mcid: None,
            artifact: false,
        };
        // \frac{d}{d\epsilon}
        let mut glyphs = vec![
            g("d", 102.0, 580.0, 10.0, Italic),
            g("d", 100.0, 565.0, 10.0, Italic),
            g("ϵ", 106.0, 565.0, 10.0, Italic),
            g("J", 116.0, 572.0, 10.0, Italic),
        ];
        let mut rules = vec![crate::geom::Rule {
            x0: 99.5,
            y0: 574.8,
            x1: 112.5,
            y1: 574.8,
        }];
        spatial::reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(rules.is_empty(), "fraction bar rule is consumed");
        assert!(
            glyphs
                .iter()
                .any(|x| x.math == MathKind::Prebuilt && x.text == "\\frac{d}{d\\epsilon}"),
            "got: {glyphs:?}"
        );

        // \sqrt[3]{x+1}
        let mut rg = vec![
            g("3", 98.0, 507.0, 6.0, Text),
            g("√", 100.0, 500.0, 10.0, Symbol),
            g("x", 107.0, 500.0, 10.0, Italic),
            g("+", 113.0, 500.0, 10.0, Text),
            g("1", 120.0, 500.0, 10.0, Text),
        ];
        let mut rr = vec![crate::geom::Rule {
            x0: 106.0,
            y0: 509.5,
            x1: 127.0,
            y1: 509.5,
        }];
        spatial::reconstruct(&mut rg, &mut rr, 600.0, 800.0);
        assert!(rr.is_empty());
        assert!(
            rg.iter()
                .any(|x| x.math == MathKind::Prebuilt && x.text == "\\sqrt[3]{x+1}"),
            "got: {rg:?}"
        );

        // \sum_{k=1}^{K} with CMEX hanging baseline
        let mut sg = vec![
            g("K", 104.0, 508.0, 7.0, Italic),
            g("∑", 100.0, 505.0, 10.0, MathKind::Extension),
            g("k", 99.0, 487.0, 7.0, Italic),
            g("=", 103.5, 487.0, 7.0, Text),
            g("1", 108.0, 487.0, 7.0, Text),
            g("x", 118.0, 500.0, 10.0, Italic),
        ];
        let mut sr = Vec::new();
        spatial::reconstruct(&mut sg, &mut sr, 600.0, 800.0);
        assert!(
            sg.iter()
                .any(|x| x.math == MathKind::Prebuilt && x.text == "\\sum_{k=1}^{K}"),
            "got: {sg:?}"
        );
    }

    #[test]
    fn adjacent_fractions_and_cmex_delim_beside_numerator_both_reconstruct() {
        let mk = |t: &str, x: f64, x_end: f64, y: f64, sz: f64, m: MathKind| Glyph {
            text: t.into(),
            x,
            y,
            x_end,
            size: sz,
            bold: false,
            math: m,
            mcid: None,
            artifact: false,
        };
        // 相邻分式 \frac{1}{2}\frac{d}{dt}（复现 2503.04467_pg7）
        let mut glyphs = vec![
            mk("1", 198.0, 203.0, 632.0, 10.0, Text),
            mk("2", 198.0, 203.0, 618.5, 10.0, Text),
            mk("d", 207.2, 212.4, 632.0, 10.0, Italic),
            mk("d", 205.4, 210.6, 618.5, 10.0, Italic),
            mk("t", 210.6, 214.2, 618.5, 10.0, Italic),
        ];
        let mut rules = vec![
            crate::geom::Rule {
                x0: 198.0,
                y0: 627.8,
                x1: 203.0,
                y1: 627.8,
            },
            crate::geom::Rule {
                x0: 205.4,
                y0: 627.8,
                x1: 214.3,
                y1: 627.8,
            },
        ];
        spatial::reconstruct(&mut glyphs, &mut rules, 600.0, 800.0);
        assert!(
            glyphs
                .iter()
                .any(|g| g.math == MathKind::Prebuilt && g.text == "\\frac{1}{2}"),
            "first adjacent fraction must reconstruct: {glyphs:?}"
        );
        assert!(
            glyphs
                .iter()
                .any(|g| g.math == MathKind::Prebuilt && g.text == "\\frac{d}{dt}"),
            "second adjacent fraction must reconstruct: {glyphs:?}"
        );

        // 紧邻 CMEX 悬挂基线 `\big\}` 的分式 \frac{\alpha h}{r}（复现 2503.05885_pg8）
        let mut g2 = vec![
            mk("α", 386.0, 392.5, 378.2, 10.9, Italic),
            mk("h", 392.5, 399.0, 378.2, 10.9, Italic),
            mk("r", 390.0, 395.5, 363.4, 10.9, Italic),
            mk("}", 400.4, 405.5, 379.8, 10.9, MathKind::Extension),
        ];
        let mut r2 = vec![crate::geom::Rule {
            x0: 385.9,
            y0: 373.7,
            x1: 399.2,
            y1: 373.7,
        }];
        spatial::reconstruct(&mut g2, &mut r2, 600.0, 800.0);
        assert!(
            g2.iter()
                .any(|g| g.math == MathKind::Prebuilt && g.text == "\\frac{\\alpha h}{r}"),
            "CMEX delimiter beside numerator must not kill fraction: {g2:?}"
        );
    }

    #[test]
    fn viterbi_segment_keeps_inline_units_parenthetical_prose_and_theta_symbol_clean() {
        // 1) 同行有真实公式时，正文字体里的 `(250 μM)` 和 `≥ 90%` 不得被拖入 `$...$`
        let w1 = [
            vec![glyph("Δ", 0.0, 100.0, 10.0, Text)],
            vec![glyph(">", 10.0, 100.0, 10.0, Text)],
            vec![glyph("0", 18.0, 100.0, 10.0, Text)],
            vec![glyph("(250", 30.0, 100.0, 10.0, Text)],
            vec![glyph("μM)", 55.0, 100.0, 10.0, Text)],
        ];
        let r1: Vec<&[Glyph]> = w1.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&r1).unwrap(), "$\\Delta > 0$ (250 μM)");

        let w2 = [
            vec![glyph("x", 0.0, 100.0, 10.0, Italic)],
            vec![glyph(">", 8.0, 100.0, 10.0, Text)],
            vec![
                glyph("0", 16.0, 100.0, 10.0, Text),
                glyph(",", 22.0, 100.0, 10.0, Text),
            ],
            vec![glyph("≥", 30.0, 100.0, 10.0, Text)],
            vec![glyph("90%", 40.0, 100.0, 10.0, Text)],
        ];
        let r2: Vec<&[Glyph]> = w2.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&r2).unwrap(), "$x > 0$, ≥ 90%");

        // 2) 圆括号内紧跟变量的英文短语 `($x$ is fast)` 不得被未配平括号前瞻吞进公式
        let w3 = [
            vec![
                glyph("(", 0.0, 100.0, 10.0, Text),
                glyph("x", 4.0, 100.0, 10.0, Italic),
            ],
            vec![glyph("is", 15.0, 100.0, 10.0, Text)],
            vec![glyph("fast)", 30.0, 100.0, 10.0, Text)],
        ];
        let r3: Vec<&[Glyph]> = w3.iter().map(|v| v.as_slice()).collect();
        let out3 = line(&r3).unwrap();
        assert!(
            out3.ends_with("is fast)"),
            "parenthetical English prose must stay outside math: {out3}"
        );

        // 2b) 含单字母变量与运算符的闭合参数 `t+0)` 仍正常配平
        let w3b = [
            vec![
                glyph("σ", 0.0, 100.0, 10.0, Italic),
                glyph("(", 6.0, 100.0, 10.0, Text),
                glyph("y", 10.0, 100.0, 10.0, Italic),
                glyph(",", 16.0, 100.0, 10.0, Text),
            ],
            vec![glyph("t+0)", 24.0, 100.0, 10.0, Text)],
        ];
        let r3b: Vec<&[Glyph]> = w3b.iter().map(|v| v.as_slice()).collect();
        assert_eq!(line(&r3b).unwrap(), "$\\sigma (y, t+0)$");

        // 3) Unicode Mathematical Capital Theta Symbol (k=17: U+1D6B9 𝚹, U+1D6F3 𝛳) 不得映射到未分配位 U+03A2
        let mut s_bold = String::new();
        symbols::push_char('\u{1D6B9}', Text, &mut s_bold);
        assert_eq!(s_bold, "\\boldsymbol{\\Theta}");
        let mut s_ital = String::new();
        symbols::push_char('\u{1D6F3}', Text, &mut s_ital);
        assert_eq!(s_ital.trim(), "\\Theta");
    }
}
