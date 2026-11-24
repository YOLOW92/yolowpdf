//! 内容流和 CMap 共用的词法器：把字节流切成"操作数 + 操作符"。
//! 不递归，嵌套深度有上限，任何输入都不会 panic。
//! 采用零拷贝 `Cow<[u8]>`、256 字节查表（LUT）、零分配快速数字解析与缓冲区复用（`OpBuf`）。

use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Operand<'a> {
    Num(f64),
    Str(Cow<'a, [u8]>),
    Name(Cow<'a, [u8]>),
    Array(Vec<Operand<'a>>),
    /// 内联字典（BDC 的属性表）：只留顶层的键和标量值，嵌套的值记为 Other。
    Dict(Vec<(Cow<'a, [u8]>, Operand<'a>)>),
    Other,
}

impl<'a> Operand<'a> {
    pub(crate) fn num(&self) -> Option<f64> {
        match self {
            Operand::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// 字典里某个键的值。
    pub(crate) fn get(&self, key: &[u8]) -> Option<&Operand<'a>> {
        match self {
            Operand::Dict(d) => d.iter().find(|(k, _)| k.as_ref() == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct Op<'a> {
    pub(crate) name: &'a [u8],
    pub(crate) args: Vec<Operand<'a>>,
}

/// 可复用的操作符缓冲区：整段内容流共享 `args` 和数组栈的堆容量，消除每条指令的分配。
#[derive(Debug, Default)]
pub(crate) struct OpBuf<'a> {
    pub(crate) name: &'a [u8],
    pub(crate) args: Vec<Operand<'a>>,
    stack: Vec<Vec<Operand<'a>>>,
}

const MAX_ARRAY_DEPTH: usize = 16;
const MAX_OPERANDS: usize = 4096;

/// 字节分类查找表：1 = 空白，2 = 分隔符，0 = 普通字节。
const CLS_WHITE: u8 = 1;
const CLS_DELIM: u8 = 2;

static BYTE_CLASS: [u8; 256] = {
    let mut t = [0u8; 256];
    t[0] = CLS_WHITE;
    t[b'\t' as usize] = CLS_WHITE;
    t[b'\n' as usize] = CLS_WHITE;
    t[0x0c] = CLS_WHITE;
    t[b'\r' as usize] = CLS_WHITE;
    t[b' ' as usize] = CLS_WHITE;
    t[b'(' as usize] = CLS_DELIM;
    t[b')' as usize] = CLS_DELIM;
    t[b'<' as usize] = CLS_DELIM;
    t[b'>' as usize] = CLS_DELIM;
    t[b'[' as usize] = CLS_DELIM;
    t[b']' as usize] = CLS_DELIM;
    t[b'{' as usize] = CLS_DELIM;
    t[b'}' as usize] = CLS_DELIM;
    t[b'/' as usize] = CLS_DELIM;
    t[b'%' as usize] = CLS_DELIM;
    t
};

static HEX_LUT: [u8; 256] = {
    let mut t = [0xFFu8; 256];
    let mut b = b'0';
    while b <= b'9' {
        t[b as usize] = b - b'0';
        b += 1;
    }
    let mut b = b'a';
    while b <= b'f' {
        t[b as usize] = b - b'a' + 10;
        b += 1;
    }
    let mut b = b'A';
    while b <= b'F' {
        t[b as usize] = b - b'A' + 10;
        b += 1;
    }
    t
};

#[inline(always)]
fn is_white(b: u8) -> bool {
    BYTE_CLASS[b as usize] == CLS_WHITE
}

#[inline(always)]
fn is_delim(b: u8) -> bool {
    BYTE_CLASS[b as usize] == CLS_DELIM
}

#[inline(always)]
fn is_regular(b: u8) -> bool {
    BYTE_CLASS[b as usize] == 0
}

#[inline(always)]
fn hex_val(b: u8) -> Option<u8> {
    let v = HEX_LUT[b as usize];
    (v != 0xFF).then_some(v)
}

pub(crate) struct Lexer<'a> {
    data: &'a [u8],
    pos: usize,
}

enum Token<'a> {
    Operand(Operand<'a>),
    ArrayStart,
    ArrayEnd,
    Keyword(&'a [u8]),
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Lexer { data, pos: 0 }
    }

    #[inline(always)]
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    #[inline]
    fn skip_white(&mut self) {
        let data = self.data;
        let len = data.len();
        let mut p = self.pos;
        while p < len {
            let b = data[p];
            if is_white(b) {
                p += 1;
            } else if b == b'%' {
                p += 1;
                while p < len {
                    let c = data[p];
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                    p += 1;
                }
            } else {
                break;
            }
        }
        self.pos = p;
    }

    #[inline]
    fn regular(&mut self) -> &'a [u8] {
        let data = self.data;
        let len = data.len();
        let start = self.pos;
        let mut p = start;
        while p < len && is_regular(data[p]) {
            p += 1;
        }
        self.pos = p;
        &data[start..p]
    }

    /// 无转义且无嵌套括号的字面串（绝大多数 PDF 文本串）直接零拷贝借用底层切片。
    fn literal_string(&mut self) -> Cow<'a, [u8]> {
        // 调用时 pos 指向 '(' 之后
        let data = self.data;
        let len = data.len();
        let start = self.pos;
        let mut p = start;
        while p < len {
            let b = data[p];
            if b == b')' {
                self.pos = p + 1;
                return Cow::Borrowed(&data[start..p]);
            }
            if b == b'\\' || b == b'(' {
                break;
            }
            p += 1;
        }
        if p >= len {
            self.pos = len;
            return Cow::Borrowed(&data[start..len]);
        }
        // 含转义或嵌套括号：先拷贝已扫描的无转义前缀，再慢路径解码
        let mut out = Vec::with_capacity((p - start) + 16);
        out.extend_from_slice(&data[start..p]);
        self.pos = p;
        let mut depth = 1usize;
        while let Some(b) = self.peek() {
            self.pos += 1;
            match b {
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Cow::Owned(out);
                    }
                    out.push(b);
                }
                b'\\' => {
                    let Some(e) = self.peek() else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push((v & 0xff) as u8);
                        }
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                _ => out.push(b),
            }
        }
        Cow::Owned(out)
    }

    fn hex_string(&mut self) -> Cow<'a, [u8]> {
        // 调用时 pos 指向 '<' 之后
        let data = self.data;
        let len = data.len();
        let start = self.pos;
        let mut end = start;
        while end < len && data[end] != b'>' {
            end += 1;
        }
        let mut out = Vec::with_capacity((end - start).div_ceil(2));
        let mut hi: Option<u8> = None;
        let mut p = start;
        while p < len {
            let b = data[p];
            p += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hex_val(b) {
                match hi.take() {
                    Some(h) => out.push(h << 4 | v),
                    None => hi = Some(v),
                }
            }
        }
        self.pos = p;
        if let Some(h) = hi {
            out.push(h << 4);
        }
        Cow::Owned(out)
    }

    #[inline]
    fn name(&mut self) -> Cow<'a, [u8]> {
        let raw = self.regular();
        if !raw.contains(&b'#') {
            return Cow::Borrowed(raw);
        }
        let mut out = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == b'#'
                && let (Some(h), Some(l)) = (
                    raw.get(i + 1).copied().and_then(hex_val),
                    raw.get(i + 2).copied().and_then(hex_val),
                )
            {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
            out.push(raw[i]);
            i += 1;
        }
        Cow::Owned(out)
    }

    /// 读 `<<` 之后的字典：顶层的 `/键 标量` 留下，数组和嵌套字典跳过记为 Other。
    fn dict(&mut self) -> Operand<'a> {
        let mut out = Vec::new();
        loop {
            self.skip_white();
            let Some(b) = self.peek() else { break };
            if b == b'>' && self.data.get(self.pos + 1) == Some(&b'>') {
                self.pos += 2;
                break;
            }
            if b != b'/' {
                self.skip_value();
                continue;
            }
            self.pos += 1;
            let key = self.name();
            self.skip_white();
            let value = match self.peek() {
                Some(b'(') => {
                    self.pos += 1;
                    Operand::Str(self.literal_string())
                }
                Some(b'<') if self.data.get(self.pos + 1) != Some(&b'<') => {
                    self.pos += 1;
                    Operand::Str(self.hex_string())
                }
                Some(b'/') => {
                    self.pos += 1;
                    Operand::Name(self.name())
                }
                Some(b'>') if self.data.get(self.pos + 1) == Some(&b'>') => continue,
                Some(_) => {
                    let start = self.pos;
                    let word = self.regular();
                    if !word.is_empty()
                        && (word[0].is_ascii_digit() || matches!(word[0], b'+' | b'-' | b'.'))
                    {
                        Operand::Num(parse_number(word))
                    } else {
                        self.pos = start;
                        self.skip_value();
                        Operand::Other
                    }
                }
                None => break,
            };
            out.push((key, value));
        }
        Operand::Dict(out)
    }

    /// 跳过一个值（字符串、数组、嵌套字典或普通词）。
    fn skip_value(&mut self) {
        let Some(b) = self.peek() else { return };
        match b {
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                self.skip_dict();
            }
            b'<' => {
                self.pos += 1;
                self.skip_hex_string();
            }
            b'(' => {
                self.pos += 1;
                self.skip_literal_string();
            }
            b'[' => {
                self.pos += 1;
                let mut depth = 1usize;
                while depth > 0 {
                    self.skip_white();
                    match self.peek() {
                        None => return,
                        Some(b'[') => {
                            self.pos += 1;
                            depth += 1;
                        }
                        Some(b']') => {
                            self.pos += 1;
                            depth -= 1;
                        }
                        Some(_) => self.skip_value(),
                    }
                }
            }
            _ if is_delim(b) => self.pos += 1,
            _ => {
                if self.regular().is_empty() {
                    self.pos += 1;
                }
            }
        }
    }

    fn skip_literal_string(&mut self) {
        let data = self.data;
        let len = data.len();
        let mut p = self.pos;
        let mut depth = 1usize;
        while p < len {
            match data[p] {
                b'\\' => p = (p + 2).min(len),
                b'(' => {
                    depth += 1;
                    p += 1;
                }
                b')' => {
                    depth -= 1;
                    p += 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => p += 1,
            }
        }
        self.pos = p;
    }

    fn skip_hex_string(&mut self) {
        let data = self.data;
        let len = data.len();
        let mut p = self.pos;
        while p < len {
            let b = data[p];
            p += 1;
            if b == b'>' {
                break;
            }
        }
        self.pos = p;
    }

    /// 跳过 `<< ... >>`。
    fn skip_dict(&mut self) {
        let mut depth = 1usize;
        while depth > 0 {
            self.skip_white();
            let Some(b) = self.peek() else { return };
            match b {
                b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                    self.pos += 2;
                    depth += 1;
                }
                b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                    self.pos += 2;
                    depth -= 1;
                }
                b'<' => {
                    self.pos += 1;
                    self.skip_hex_string();
                }
                b'(' => {
                    self.pos += 1;
                    self.skip_literal_string();
                }
                _ if is_delim(b) => self.pos += 1,
                _ => {
                    self.regular();
                }
            }
        }
    }

    #[inline]
    fn token(&mut self) -> Option<Token<'a>> {
        loop {
            self.skip_white();
            let b = self.peek()?;
            match b {
                b'(' => {
                    self.pos += 1;
                    return Some(Token::Operand(Operand::Str(self.literal_string())));
                }
                b'<' => {
                    if self.data.get(self.pos + 1) == Some(&b'<') {
                        self.pos += 2;
                        return Some(Token::Operand(self.dict()));
                    }
                    self.pos += 1;
                    return Some(Token::Operand(Operand::Str(self.hex_string())));
                }
                b'/' => {
                    self.pos += 1;
                    return Some(Token::Operand(Operand::Name(self.name())));
                }
                b'[' => {
                    self.pos += 1;
                    return Some(Token::ArrayStart);
                }
                b']' => {
                    self.pos += 1;
                    return Some(Token::ArrayEnd);
                }
                b'>' | b')' | b'{' | b'}' => {
                    self.pos += 1;
                }
                _ => {
                    let word = self.regular();
                    if word.is_empty() {
                        self.pos += 1;
                        continue;
                    }
                    let first = word[0];
                    if first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.') {
                        return Some(Token::Operand(Operand::Num(parse_number(word))));
                    }
                    if matches!(word, b"true" | b"false" | b"null") {
                        return Some(Token::Operand(Operand::Other));
                    }
                    return Some(Token::Keyword(word));
                }
            }
        }
    }

    /// 内联图像：`BI ... ID <二进制> EI`，跳过二进制数据。返回字典部分（`BI` 与 `ID` 之间）和
    /// 图像数据的字节范围，图片识别（image 模块）按需再解析。
    fn skip_inline_image(&mut self) -> Option<((usize, usize), (usize, usize))> {
        let head = self.pos;
        let head_end = loop {
            let at = self.pos;
            match self.token() {
                None => return None,
                Some(Token::Keyword(b"ID")) => break at,
                Some(_) => {}
            }
        };
        self.pos += 1;
        let data = self.data;
        let start = self.pos.min(data.len());
        let mut i = self.pos;
        while i + 1 < data.len() {
            if data[i] == b'E'
                && data[i + 1] == b'I'
                && (i == 0 || is_white(data[i - 1]))
                && data.get(i + 2).is_none_or(|&c| is_white(c))
            {
                self.pos = i + 2;
                return Some(((head, head_end), (start, i.saturating_sub(1).max(start))));
            }
            i += 1;
        }
        self.pos = data.len();
        Some(((head, head_end), (start, data.len())))
    }

    /// 零分配复用缓冲区读取下一条操作符。读到时填好 `buf.name` 与 `buf.args` 并返回 true。
    pub(crate) fn next_op_into(&mut self, buf: &mut OpBuf<'a>) -> bool {
        buf.args.clear();
        buf.stack.clear();
        loop {
            let Some(tok) = self.token() else {
                return false;
            };
            match tok {
                Token::Operand(o) => {
                    if let Some(top) = buf.stack.last_mut() {
                        top.push(o);
                    } else {
                        if buf.args.len() >= MAX_OPERANDS {
                            buf.args.clear();
                        }
                        buf.args.push(o);
                    }
                }
                Token::ArrayStart => {
                    if buf.stack.len() < MAX_ARRAY_DEPTH {
                        buf.stack.push(Vec::new());
                    } else if let Some(top) = buf.stack.last_mut() {
                        top.push(Operand::Other);
                    }
                }
                Token::ArrayEnd => {
                    if let Some(done) = buf.stack.pop() {
                        let arr = Operand::Array(done);
                        if let Some(top) = buf.stack.last_mut() {
                            top.push(arr);
                        } else {
                            buf.args.push(arr);
                        }
                    }
                }
                Token::Keyword(k) => {
                    while let Some(done) = buf.stack.pop() {
                        let arr = Operand::Array(done);
                        if let Some(top) = buf.stack.last_mut() {
                            top.push(arr);
                        } else {
                            buf.args.push(arr);
                        }
                    }
                    if k == b"BI" {
                        buf.args.clear();
                        if let Some(((h0, h1), (d0, d1))) = self.skip_inline_image() {
                            buf.args
                                .push(Operand::Str(Cow::Borrowed(&self.data[h0..h1])));
                            buf.args
                                .push(Operand::Str(Cow::Borrowed(&self.data[d0..d1])));
                        }
                    }
                    buf.name = k;
                    return true;
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn next_op(&mut self) -> Option<Op<'a>> {
        let mut buf = OpBuf::default();
        self.next_op_into(&mut buf).then_some(Op {
            name: buf.name,
            args: buf.args,
        })
    }
}

/// 精确的 10^k 查表（0 <= k <= 18 在 IEEE-754 f64 中均可精确表示，k <= 22 也精确）。
static POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// 零分配快速 ASCII 浮点/整数解析：容忍 "--5"、"1.2.3" 等宽松 PDF 写法。
#[inline]
fn parse_number(word: &[u8]) -> f64 {
    if word.is_empty() {
        return 0.0;
    }
    let mut neg = false;
    let mut i = 0;
    if word[0] == b'-' {
        neg = true;
        i = 1;
    } else if word[0] == b'+' {
        i = 1;
    }
    let mut mantissa: u64 = 0;
    let mut digits: u32 = 0;
    let mut frac_digits: u32 = 0;
    let mut seen_dot = false;
    let mut overflow = false;
    while i < word.len() {
        let b = word[i];
        i += 1;
        match b {
            b'0'..=b'9' => {
                if digits < 18 {
                    mantissa = mantissa * 10 + u64::from(b - b'0');
                    digits += 1;
                    if seen_dot {
                        frac_digits += 1;
                    }
                } else {
                    overflow = true;
                    break;
                }
            }
            b'.' if !seen_dot => seen_dot = true,
            b'+' | b'-' => {}
            _ => break,
        }
    }
    if !overflow {
        if digits == 0 {
            return 0.0;
        }
        let v = if frac_digits == 0 {
            mantissa as f64
        } else {
            mantissa as f64 / POW10[frac_digits as usize]
        };
        return if neg { -v } else { v };
    }
    // 超过 18 位的极少见数字：64 字节内走栈缓冲，超长数字走堆分配避免截断
    if word.len() <= 64 {
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        let mut dot = false;
        for (idx, &b) in word.iter().enumerate() {
            match b {
                b'+' | b'-' if idx == 0 => {
                    buf[len] = b;
                    len += 1;
                }
                b'+' | b'-' => {}
                b'.' if !dot => {
                    dot = true;
                    buf[len] = b'.';
                    len += 1;
                }
                b'0'..=b'9' => {
                    buf[len] = b;
                    len += 1;
                }
                _ => break,
            }
        }
        let v = std::str::from_utf8(&buf[..len])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        return if v.is_finite() { v } else { 0.0 };
    }
    let mut s = String::with_capacity(word.len());
    let mut dot = false;
    for (idx, &b) in word.iter().enumerate() {
        match b {
            b'+' | b'-' if idx == 0 => s.push(b as char),
            b'+' | b'-' => {}
            b'.' if !dot => {
                dot = true;
                s.push('.');
            }
            b'0'..=b'9' => s.push(b as char),
            _ => break,
        }
    }
    let v = s.parse::<f64>().unwrap_or(0.0);
    if v.is_finite() { v } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(src: &[u8]) -> Vec<Op<'_>> {
        let mut lx = Lexer::new(src);
        let mut v = Vec::new();
        while let Some(op) = lx.next_op() {
            v.push(op);
        }
        v
    }

    #[test]
    fn basic_text_ops() {
        let v = ops(b"BT /F1 12 Tf 72 700 Td (Hello \\(x\\)) Tj [(A) -120 <42>] TJ ET");
        let names: Vec<_> = v.iter().map(|o| o.name).collect();
        assert_eq!(
            names,
            vec![
                &b"BT"[..],
                &b"Tf"[..],
                &b"Td"[..],
                &b"Tj"[..],
                &b"TJ"[..],
                &b"ET"[..]
            ]
        );
        assert_eq!(v[3].args, vec![Operand::Str(Cow::Borrowed(b"Hello (x)"))]);
        assert_eq!(
            v[4].args,
            vec![Operand::Array(vec![
                Operand::Str(Cow::Borrowed(b"A")),
                Operand::Num(-120.0),
                Operand::Str(Cow::Borrowed(b"B"))
            ])]
        );
    }

    #[test]
    fn inline_image_skipped() {
        let v = ops(b"q BI /W 2 /H 2 ID \x00 EIx\xff EI Q (x) Tj");
        let names: Vec<_> = v.iter().map(|o| o.name).collect();
        assert_eq!(names, vec![&b"q"[..], &b"BI"[..], &b"Q"[..], &b"Tj"[..]]);
        // 字典部分和数据（EI 前的空白不算数据）留给图片识别
        assert_eq!(
            v[1].args,
            vec![
                Operand::Str(Cow::Borrowed(b" /W 2 /H 2")),
                Operand::Str(Cow::Borrowed(b"\x00 EIx\xff"))
            ]
        );
    }

    #[test]
    fn octal_and_names() {
        let v = ops(b"/A#20B (\\101\\1) Tj");
        assert_eq!(v[0].args[0], Operand::Name(Cow::Borrowed(b"A B")));
        assert_eq!(v[0].args[1], Operand::Str(Cow::Owned(vec![b'A', 1])));
    }

    #[test]
    fn fast_number_parsing_matches_edge_cases() {
        assert_eq!(parse_number(b"0"), 0.0);
        assert_eq!(parse_number(b"-120"), -120.0);
        assert_eq!(parse_number(b"+3.25"), 3.25);
        assert_eq!(parse_number(b"-.5"), -0.5);
        assert_eq!(parse_number(b"--5"), -5.0);
        assert_eq!(parse_number(b"1.2.3"), 1.2);
        assert!((parse_number(b"12345678901234567890") - 1.2345678901234567e19).abs() < 1e6);
        let big = format!("1{}", "0".repeat(69));
        assert!((parse_number(big.as_bytes()) - 1e69).abs() / 1e69 < 1e-12);
        let tiny = format!("0.{}5", "0".repeat(65));
        assert!((parse_number(tiny.as_bytes()) - 5e-66).abs() / 5e-66 < 1e-12);
    }
}
