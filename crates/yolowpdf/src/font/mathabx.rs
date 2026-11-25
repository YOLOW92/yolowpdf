//! CTAN `mathabx`（`TeX-matha` / `TeX-mathb` / `TeX-mathx`）规范解码器。
//!
//! `mathabx` Type1/CFF 字体在 pdfTeX/dvips 中普遍携带错误的 T1/Cork `ToUnicode` 映射
//! （例如 `0x10` 映成 `“`、`0x70`/`0x71` 映成 `p`/`q`、`0xA4` 映成 `ď`、`0xD1` 映成 `Ñ`）。
//! 当检测到 `TeX-matha`/`TeX-mathb`/`TeX-mathx` 字体且其当前映射带有 T1 特征时，
//! 按 Anthony Phan 的官方 `mathabx.dcl` 槽位表直接恢复真实 Unicode 数学符号。

use crate::font::MathKind;
use crate::short::Short;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathabxFamily {
    MathA,
    MathB,
    MathX,
}

/// 校验设计字号后缀：CTAN `mathabx` 字体名只允许 `matha` / `matha10` / `TeX-mathb12` 等纯数字字号后缀，
/// 严格排除 `MathBold` / `MathBlackboard` / `MathAlpha` 等普通字体。
fn is_design_size_suffix(rest: &str) -> bool {
    rest.is_empty()
        || rest.bytes().all(|b| b.is_ascii_digit())
        || rest
            .strip_prefix('-')
            .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
}

/// 识别 `BaseFont` 是否为 `mathabx` 族字体（去掉子集前缀 `ABCDEF+`）。
pub(crate) fn detect_family(base_font: &str) -> Option<MathabxFamily> {
    let name = base_font
        .rsplit('+')
        .next()
        .unwrap_or(base_font)
        .to_ascii_lowercase();
    let stripped = name.strip_prefix("tex-").unwrap_or(&name);
    if let Some(rest) = stripped.strip_prefix("matha")
        && is_design_size_suffix(rest)
    {
        Some(MathabxFamily::MathA)
    } else if let Some(rest) = stripped.strip_prefix("mathb")
        && is_design_size_suffix(rest)
    {
        Some(MathabxFamily::MathB)
    } else if let Some(rest) = stripped.strip_prefix("mathx")
        && is_design_size_suffix(rest)
    {
        Some(MathabxFamily::MathX)
    } else {
        None
    }
}

/// `mathabx` 字体对应的 `MathKind`。
pub(crate) fn math_kind_of(fam: MathabxFamily) -> MathKind {
    match fam {
        MathabxFamily::MathX => MathKind::Extension,
        MathabxFamily::MathA | MathabxFamily::MathB => MathKind::Symbol,
    }
}

/// 检查现有 `ToUnicode` / `simple_map` 是否带有 pdfTeX T1/Cork 错映签名
/// （例如 `0x00`→`` ` ``、`0x10`→`“`、`0x70`→`p`、`0xA4`→`ď`、`0xB0`→`ř`、`0xD1`→`Ñ`、`0xE3`→`ã`）。
/// 若 PDF 已经自带真正的 Unicode 映射则不覆盖。
pub(crate) fn has_t1_cork_signature(lookup: &dyn Fn(u8) -> Option<Short>) -> bool {
    const PROBES: &[(u8, &str)] = &[
        (0x00, "`"),
        (0x01, "´"),
        (0x02, "ˆ"),
        (0x04, "¨"),
        (0x06, "˚"),
        (0x0D, "‚"),
        (0x10, "“"),
        (0x11, "”"),
        (0x15, "–"),
        (0x16, "—"),
        (0x18, "‰"),
        (0x24, "$"),
        (0x25, "%"),
        (0x26, "&"),
        (0x38, "8"),
        (0x39, "9"),
        (0x3A, ":"),
        (0x3E, ">"),
        (0x50, "P"),
        (0x6C, "l"),
        (0x70, "p"),
        (0x71, "q"),
        (0x72, "r"),
        (0x73, "s"),
        (0x7A, "z"),
        (0xA0, "ă"),
        (0xA1, "ą"),
        (0xA4, "ď"),
        (0xA5, "ě"),
        (0xB0, "ř"),
        (0xB3, "ş"),
        (0xB8, "ÿ"),
        (0xBB, "ż"),
        (0xC0, "À"),
        (0xC1, "Á"),
        (0xD1, "Ñ"),
        (0xDE, "Þ"),
        (0xE0, "à"),
        (0xE3, "ã"),
        (0xF0, "ð"),
        (0xF1, "ñ"),
        (0xF4, "ô"),
    ];
    let mut hits = 0usize;
    let mut non_empty = 0usize;
    for &(code, t1) in PROBES {
        if let Some(s) = lookup(code)
            && !s.is_empty()
        {
            non_empty += 1;
            if s.as_str() == t1 {
                hits += 1;
            }
        }
    }
    hits >= 1 && hits * 2 >= non_empty.max(1)
}

/// 官方 CTAN `matha`（`mathabx.dcl`）槽位 → Unicode 字符映射。
pub(crate) fn matha_char(code: u8) -> Option<char> {
    Some(match code {
        // 0x00..0x0F：二元运算符
        0x00 => '+',
        0x01 => '−',
        0x02 => '×',
        0x03 => '÷',
        0x04 => '·',
        0x05 => '∘',
        0x06 => '∗',
        0x07 => '⋆',
        0x08 => '±',
        0x09 => '∓',
        0x0A => '∙',
        0x0B => '⋄',
        0x0C => '△',
        0x0D => '∙',
        0x0E => '⋆',
        0x0F => '▷',
        // 0x10..0x1F：等号与等价/相似关系
        0x10 => '=',
        0x11 => '≡',
        0x12 => '∼',
        0x13 => '≃',
        0x14 => '≈',
        0x15 => '≅',
        0x16 => '≍',
        0x17 => '≐',
        0x18 => '≠',
        0x19 => '≢',
        0x1A => '≁',
        0x1B => '≄',
        0x1C => '≉',
        0x1D => '≇',
        // 0x20..0x2F：远小于/远大于与序关系
        0x21 => '≪',
        0x22 => '≫',
        // 0x30..0x3F：导数撇号、空集、无穷大等
        0x30 => '°',
        0x31 => '′',
        0x32 => '″',
        0x33 => '‴',
        0x36 => '∅',
        0x38 => '∞',
        0x39 => '′',
        0x3A => '†',
        // 0x40..0x4F：量词、偏导、梯度、垂直/平行
        0x40 => '∀',
        0x41 => '∃',
        0x42 => '∂',
        0x43 => '∇',
        0x44 => '∄',
        0x4A => '∥',
        0x4B => '⊥',
        0x4C => '⊤',
        0x4D => '⊢',
        0x4E => '⊣',
        0x4F => '⊨',
        // 0x50..0x5F：集合属于与交并逻辑
        0x50 => '∈',
        0x51 => '∋',
        0x52 => '∉',
        0x53 => '∌',
        0x58 => '∩',
        0x59 => '∪',
        0x5A => '∧',
        0x5B => '∨',
        0x5C => '⊓',
        0x5D => '⊔',
        0x5E => '⊎',
        // 0x60..0x6F：带圈运算符
        0x60 => '⊕',
        0x61 => '⊖',
        0x62 => '⊗',
        0x63 => '⊘',
        0x64 => '⊙',
        // 0x70..0x7F：定界符
        0x70 => '(',
        0x71 => ')',
        0x72 => '[',
        0x73 => ']',
        0x74 => '{',
        0x75 => '}',
        0x76 => '⟨',
        0x77 => '⟩',
        0x78 => '⌊',
        0x79 => '⌋',
        0x7A => '∖',
        0x7B => '/',
        0x7C => '|',
        0x7D => '∥',
        0x7E => '⌈',
        0x7F => '⌉',
        // 0x80..0x8F：子集关系
        0x80 => '⊂',
        0x81 => '⊃',
        0x82 => '⊄',
        0x83 => '⊅',
        0x84 => '⊆',
        0x85 => '⊇',
        0x86 => '⊈',
        0x87 => '⊉',
        0x88 => '⊊',
        0x89 => '⊋',
        // 0x90..0x9F：偏序与格序
        0x90 => '≺',
        0x91 => '≻',
        0x94 => '⪯',
        0x95 => '⪰',
        0x98 => '⊑',
        0x99 => '⊒',
        // 0xA0..0xAF：大小不等号
        0xA0 => '<',
        0xA1 => '>',
        0xA2 => '≮',
        0xA3 => '≯',
        0xA4 => '≤',
        0xA5 => '≥',
        0xA6 => '≰',
        0xA7 => '≱',
        0xA8 => '⩽',
        0xA9 => '⩾',
        // 0xC0..0xC1：近似不等号
        0xC0 => '≲',
        0xC1 => '≳',
        // 0xD0..0xEF：箭头
        0xD0 => '←',
        0xD1 => '→',
        0xD2 => '↑',
        0xD3 => '↓',
        0xD4 => '↔',
        0xD5 => '↕',
        0xD6 => '↖',
        0xD7 => '↗',
        0xD8 => '⇐',
        0xD9 => '⇒',
        0xDA => '⇑',
        0xDB => '⇓',
        0xDC => '⇔',
        0xDD => '⇕',
        0xDE => '↦',
        0xDF => '↘',
        0xE0 => '↙',
        0xE1 => '↼',
        0xE2 => '⇀',
        0xE3 => '↪',
        0xE4 => '↩',
        _ => return None,
    })
}

/// 官方 CTAN `mathx`（`mathabx.dcl`）大型算子、宽重音与可扩展定界符槽位。
pub(crate) fn mathx_char(code: u8) -> Option<char> {
    Some(match code {
        // 可扩展圆括号/方括号/大括号及堆叠片断（Unicode U+239B..U+23AD，直接喂给 2D 矩阵/cases 引擎）
        0x00..=0x03 => '(',
        0x04 => '\u{239B}', // ⎛
        0x05 => '\u{239D}', // ⎝
        0x06 => '\u{239C}', // ⎜
        0x08..=0x0B => ')',
        0x0C => '\u{239E}', // ⎞
        0x0D => '\u{23A0}', // ⎠
        0x0E => '\u{239F}', // ⎟
        0x0F => '|',
        0x10..=0x13 => '[',
        0x14 => '\u{23A1}', // ⎡
        0x15 => '\u{23A3}', // ⎣
        0x16 => '\u{23A2}', // ⎢
        0x18..=0x1B => ']',
        0x1C => '\u{23A4}', // ⎤
        0x1D => '\u{23A6}', // ⎦
        0x1E => '\u{23A5}', // ⎥
        0x20..=0x23 => '{',
        0x24 => '\u{23A7}', // ⎧
        0x25 => '\u{23A9}', // ⎩
        0x26 => '\u{23A8}', // ⎨
        0x27 => '\u{23AA}', // ⎪
        0x28..=0x2B => '}',
        0x2C => '\u{23AB}', // ⎫
        0x2D => '\u{23AD}', // ⎭
        0x2E => '\u{23AC}', // ⎬
        0x2F => '\u{23AA}', // ⎪
        0x30..=0x33 => '⌊',
        0x38..=0x3B => '⌋',
        0x40..=0x43 => '⌈',
        0x48..=0x4B => '⌉',
        0x50..=0x53 => '⟨',
        0x58..=0x5B => '⟩',
        // 宽重音（`\widehat`, `\widecheck`, `\widetilde`）
        0x70 => '\u{0302}',
        0x71 => 'ˇ',
        0x72 => '\u{0303}',
        // 根号
        0x80..=0x84 => '√',
        // 大型算子（行内与行间两套尺寸）
        0x90 | 0x98 => '⋀',
        0x91 | 0x99 => '⋁',
        0x92 | 0x9A => '⋂',
        0x93 | 0x9B => '⋃',
        0x94 | 0x9C => '⨁',
        0x95 | 0x9D => '⨂',
        0x96 | 0x9E => '⨀',
        0x97 | 0x9F => '⨄',
        0xB0 | 0xB8 => '∑',
        0xB1 | 0xB9 => '∏',
        0xB2 | 0xBA => '∐',
        0xB3 | 0xBB => '∫',
        0xB4 | 0xBC => '∬',
        0xB6 | 0xBE => '∮',
        _ => return None,
    })
}

/// 官方 CTAN `mathb` 常用符号槽位（经 arXiv `TeX-mathb` 实证核验）。
pub(crate) fn mathb_char(code: u8) -> Option<char> {
    Some(match code {
        0x0D => '·',
        0x23 => '#',
        0x39 => '˙',
        0x3E => '∠',
        0x6C => '□',
        0xE3 => '↪',
        _ => return None,
    })
}

/// 按 `mathabx` 族映射槽位。
pub(crate) fn decode_slot(fam: MathabxFamily, code: u8) -> Option<char> {
    match fam {
        MathabxFamily::MathA => matha_char(code),
        MathabxFamily::MathB => mathb_char(code),
        MathabxFamily::MathX => mathx_char(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mathabx_families_and_decodes_core_slots() {
        assert_eq!(
            detect_family("UXSBSO+TeX-matha12"),
            Some(MathabxFamily::MathA)
        );
        assert_eq!(
            detect_family("NFYEEG+TeX-mathb12"),
            Some(MathabxFamily::MathB)
        );
        assert_eq!(
            detect_family("GZLDIX+TeX-mathx10"),
            Some(MathabxFamily::MathX)
        );
        assert_eq!(detect_family("ZHGGTU+CMMI10"), None);
        // 严禁误伤 MathBold / MathBlackboard / MathAlpha 等非 mathabx 字体
        assert_eq!(detect_family("ABCDEF+MathBold"), None);
        assert_eq!(detect_family("ABCDEF+MathBlackboard"), None);
        assert_eq!(detect_family("ABCDEF+MathAlpha"), None);

        // 真实 Unicode 映射不触发 T1/Cork 覆写；T1/Cork 子集（含单个 0xE3→ã）触发
        assert!(!has_t1_cork_signature(&|b| match b {
            0x10 => Some(Short::from('=')),
            0x70 => Some(Short::from('(')),
            0x71 => Some(Short::from(')')),
            _ => None,
        }));
        assert!(has_t1_cork_signature(&|b| match b {
            0xE3 => Some(Short::from('ã')),
            _ => None,
        }));

        assert_eq!(decode_slot(MathabxFamily::MathA, 0x0D), Some('∙'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x0E), Some('⋆'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x10), Some('='));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x3A), Some('†'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x70), Some('('));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x71), Some(')'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x7A), Some('∖'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0xA4), Some('≤'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0xC0), Some('≲'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0x50), Some('∈'));
        assert_eq!(decode_slot(MathabxFamily::MathA, 0xD1), Some('→'));
        assert_eq!(decode_slot(MathabxFamily::MathB, 0x0D), Some('·'));
        assert_eq!(decode_slot(MathabxFamily::MathB, 0x23), Some('#'));
        assert_eq!(decode_slot(MathabxFamily::MathB, 0x39), Some('˙'));
        assert_eq!(decode_slot(MathabxFamily::MathB, 0x3E), Some('∠'));
        assert_eq!(decode_slot(MathabxFamily::MathB, 0x6C), Some('□'));
        assert_eq!(decode_slot(MathabxFamily::MathB, 0xE3), Some('↪'));
        assert_eq!(decode_slot(MathabxFamily::MathX, 0x24), Some('\u{23A7}'));
        assert_eq!(decode_slot(MathabxFamily::MathX, 0xB0), Some('∑'));
        assert_eq!(decode_slot(MathabxFamily::MathX, 0xB3), Some('∫'));
    }
}
