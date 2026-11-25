//! 嵌入字体程序内建编码解析器（支持 Type 1 与 CFF/Type1C）。

pub(crate) mod cff;
pub(crate) mod strings;
pub(crate) mod truetype;
pub(crate) mod type1;

use crate::file::{Dictionary, Document, Object};
use crate::obj::{get, get_num, stream_bytes, stream_bytes_prefix};
use cff::cff;
use type1::type1;

/// 字符码 0..=255 对应的字形名。字体程序用标准编码、或者读不出来时返回 None。
pub(crate) fn builtin_encoding(
    doc: &Document,
    descriptor: &Dictionary,
    limit: usize,
) -> Option<Vec<Option<String>>> {
    if let Some(obj @ Object::Reference(_) | obj @ Object::Stream(_)) = descriptor.get(b"FontFile")
    {
        let length1 = match crate::obj::deref(doc, obj) {
            Object::Stream(s) => get_num(doc, &s.dict, b"Length1"),
            _ => None,
        };
        fn strip_pfb(b: &[u8]) -> &[u8] {
            if b.len() > 6 && b[0] == 0x80 && b[1] == 1 {
                &b[6..]
            } else {
                b
            }
        }
        // Type1 的 `/Encoding` 只在明文段 `Length1` 内（通常仅 1–3 KB），前缀解压跳过整段 eexec 字形轮廓；
        // 若畸形 PDF 的 `Length1` 偏小导致未见到 `/Encoding` 结束标记或 `eexec`，自动回退全量解压防截断。
        if let Some(n) = length1
            && n > 0.0
            && (n as usize) <= limit
        {
            let n = n as usize;
            let want = n.saturating_add(16);
            let pref = stream_bytes_prefix(doc, obj, want, limit)?;
            let body = strip_pfb(&pref);
            let clear = &body[..n.min(body.len())];
            let complete = type1::find(clear, b"StandardEncoding").is_some()
                || type1::find(clear, b"readonly def").is_some()
                || type1::find(clear, b"currentdict end").is_some()
                || type1::find(clear, b"eexec").is_some();
            if complete || pref.len() < want {
                return type1(clear);
            }
        }
        let bytes = stream_bytes(doc, obj, limit)?;
        let clear = strip_pfb(&bytes);
        return type1(clear);
    }
    let obj = get(doc, descriptor, b"FontFile3")?;
    let bytes = stream_bytes(doc, obj, limit)?;
    cff(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type1_builtin_encoding() {
        let clear = b"%!PS-AdobeFont-1.0: CMSY10\n/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\n\
dup 0 /minus put\ndup 20 /lessequal put\ndup 50 /element put\nreadonly def\ncurrentfile eexec";
        let enc = type1::type1(clear).unwrap();
        assert_eq!(enc[0].as_deref(), Some("minus"));
        assert_eq!(enc[20].as_deref(), Some("lessequal"));
        assert_eq!(enc[50].as_deref(), Some("element"));
        assert_eq!(enc[1], None);
        assert_eq!(type1::type1(b"/Encoding StandardEncoding def"), None);
    }

    #[test]
    fn type1_malformed_short_length1_falls_back_to_full_stream() {
        // 构造一个 FlateDecode 压缩的 Type1 FontFile，其 Length1 故意短于 /Encoding 结束位置，
        // 校验 builtin_encoding 会自动回退全量解压而不截断编码表
        let clear = b"%!PS-AdobeFont-1.0: TEST10\n/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\n\
dup 10 /alpha put\ndup 99 /omega put\nreadonly def\ncurrentfile eexec\n0123456789abcdef";
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(clear, 6);
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        let o1 = pdf.len();
        pdf.extend_from_slice(b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n");
        let o2 = pdf.len();
        pdf.extend_from_slice(b"2 0 obj<</Type/Pages/Kids[]/Count 0>>endobj\n");
        let o3 = pdf.len();
        pdf.extend_from_slice(
            format!(
                "3 0 obj<</Filter/FlateDecode/Length {}/Length1 60>>stream\n",
                compressed.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        let o4 = pdf.len();
        pdf.extend_from_slice(b"4 0 obj<</Type/FontDescriptor/FontFile 3 0 R>>endobj\n");
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!(
                "xref\n0 5\n0000000000 65535 f \n{o1:010} 00000 n \n{o2:010} 00000 n \n{o3:010} 00000 n \n{o4:010} 00000 n \ntrailer<</Size 5/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n"
            )
            .as_bytes(),
        );
        let doc = Document::open(&pdf, 1 << 20).unwrap();
        let Object::Dictionary(desc) = doc.get_object((4, 0)).unwrap() else {
            panic!("expected font descriptor dict");
        };
        let enc = builtin_encoding(&doc, desc, 1 << 20).unwrap();
        assert_eq!(enc[10].as_deref(), Some("alpha"));
        assert_eq!(enc[99].as_deref(), Some("omega"));
    }

    #[test]
    fn cff_builtin_encoding() {
        fn index(items: &[&[u8]]) -> Vec<u8> {
            if items.is_empty() {
                return vec![0, 0];
            }
            let mut out = vec![0, items.len() as u8, 1];
            let mut off = 1u8;
            out.push(off);
            for it in items {
                off += it.len() as u8;
                out.push(off);
            }
            for it in items {
                out.extend_from_slice(it);
            }
            out
        }
        let enc_int = |v: i32| -> Vec<u8> {
            let b = v.to_be_bytes();
            vec![29, b[0], b[1], b[2], b[3]]
        };
        let header = [1u8, 0, 4, 1];
        let names = index(&[b"F"]);
        let strings = index(&[b"summationtext"]);
        let gsubrs = index(&[]);
        let top_len = 18;
        let top_index_len = 2 + 1 + 2 + top_len;
        let base = header.len() + names.len() + top_index_len + strings.len() + gsubrs.len();
        let charset_off = base;
        let charset = [0u8, 1, 135];
        let enc_off = charset_off + charset.len();
        let encoding = [0u8, 1, 0x58];
        let cs_off = enc_off + encoding.len();
        let charstrings = index(&[&[14], &[14]]);
        let mut top = Vec::new();
        top.extend(enc_int(charset_off as i32));
        top.push(15);
        top.extend(enc_int(enc_off as i32));
        top.push(16);
        top.extend(enc_int(cs_off as i32));
        top.push(17);
        assert_eq!(top.len(), top_len);
        let mut font = header.to_vec();
        font.extend(names);
        font.extend(index(&[&top]));
        font.extend(strings);
        font.extend(gsubrs);
        font.extend(charset);
        font.extend(encoding);
        font.extend(charstrings);
        let enc = cff::cff(&font).unwrap();
        assert_eq!(enc[0x58].as_deref(), Some("summationtext"));
        assert_eq!(
            crate::encoding::glyph_to_unicode(b"summationtext").as_deref(),
            Some("\u{2211}")
        );
        assert_eq!(
            crate::encoding::glyph_to_unicode(b"parenleftBigg").as_deref(),
            Some("(")
        );
    }
}
