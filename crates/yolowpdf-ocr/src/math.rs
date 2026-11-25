//! 行间公式：版面模型找出公式框，公式模型把框里的图认成 LaTeX。框里的普通文字行由流水线去掉。

use std::path::Path;

use yolowpdf_infer::Weights;

use crate::formula::{Decoder, Tokenizer, to_latex};
use crate::formula_enc::{Config, Encoder, preprocess_to};
use crate::layout::{Layout, Region};
use crate::{Image, Line, read_json};

/// [协议] PP-DocLayoutV3 的 `inference.yml` 里行间公式的细分类名
const DISPLAY_FORMULA: &str = "display_formula";
/// [策略] 同类行间公式框的交并比去重阈值：轻量化 3 层解码器可能对同一公式产出多个近乎重合的查询框
const FORMULA_NMS_IOU: f32 = 0.5;

/// 版面检测加公式识别。
pub struct Math {
    layout: Layout,
    encoder: Encoder,
    decoder: Decoder,
    tokenizer: Tokenizer,
}

impl Math {
    pub fn load(layout_dir: &Path, formula_dir: &Path) -> Result<Math, String> {
        let cfg = read_json(formula_dir, "config.json")?;
        let generation = read_json(formula_dir, "generation_config.json")?;
        // 编码器和解码器的权重在同一个文件里，读一次，建好就放掉
        let w = crate::read_weights(formula_dir)?;
        let decoder = Decoder::from_weights(&w, &cfg["text_config"], &generation)?;
        let encoder = Encoder::from_weights(&w, Config::from_json(&cfg)?)?;
        drop::<Weights>(w);
        Ok(Math {
            layout: Layout::load(layout_dir)?,
            encoder,
            decoder,
            tokenizer: Tokenizer::load(formula_dir)?,
        })
    }

    /// 这页的行间公式框（按置信度做交并比去重后保持原阅读顺序）。
    pub(crate) fn formula_boxes(&self, page: &Image) -> Result<Vec<Region>, String> {
        let mut regions = self.layout.detect(page)?;
        regions.retain(|r| r.kind == DISPLAY_FORMULA);
        Ok(dedup_boxes(regions, FORMULA_NMS_IOU))
    }

    /// 认出一个框，得到一行 `$$…$$`；没认出内容时返回 `None`。
    pub(crate) fn recognize(&self, page: &Image, r: &Region) -> Result<Option<Line>, String> {
        let crop = crop_rect(page, r);
        if crop.width == 0 || crop.height == 0 {
            return Ok(None);
        }
        let cfg = self.encoder.config();
        let memory =
            self.encoder
                .encode(&preprocess_to(&crop, cfg.image_height, cfg.image_width))?;
        let ids = self.decoder.generate(&memory, self.decoder.max_length())?;
        let latex = to_latex(&self.tokenizer.decode(&ids));
        let latex = latex.trim();
        Ok((!latex.is_empty()).then(|| Line {
            quad: [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)],
            text: format!("$${latex}$$"),
            score: r.score,
            formula: true,
        }))
    }
}

/// 按框裁图（不像文字行那样按高宽比转向：公式常比宽还高）。
fn crop_rect(page: &Image, r: &Region) -> Image {
    let (x0, y0) = (r.x0.max(0.0) as usize, r.y0.max(0.0) as usize);
    let x1 = (r.x1.ceil().max(0.0) as usize).min(page.width);
    let y1 = (r.y1.ceil().max(0.0) as usize).min(page.height);
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let c = page.channels;
    let mut data = Vec::with_capacity(w * h * c);
    // 框整个在页外时宽或高是 0，没有可读的行
    for y in (y0..y0 + h).filter(|_| w > 0) {
        let at = (y * page.width + x0) * c;
        data.extend_from_slice(&page.data[at..at + w * c]);
    }
    Image {
        width: w,
        height: h,
        channels: c,
        data,
    }
}

/// 中心落在某个框里的行算在公式里。
pub(crate) fn inside_any(quad: &crate::Quad, boxes: &[Region]) -> bool {
    let n = quad.len() as f32;
    let cx = quad.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = quad.iter().map(|p| p.1).sum::<f32>() / n;
    boxes
        .iter()
        .any(|r| cx >= r.x0 && cx <= r.x1 && cy >= r.y0 && cy <= r.y1)
}

fn box_iou(a: &Region, b: &Region) -> f32 {
    let ix0 = a.x0.max(b.x0);
    let iy0 = a.y0.max(b.y0);
    let ix1 = a.x1.min(b.x1);
    let iy1 = a.y1.min(b.y1);
    let inter = (ix1 - ix0).max(0.0) * (iy1 - iy0).max(0.0);
    if inter <= 0.0 {
        return 0.0;
    }
    let area_a = (a.x1 - a.x0).max(0.0) * (a.y1 - a.y0).max(0.0);
    let area_b = (b.x1 - b.x0).max(0.0) * (b.y1 - b.y0).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// 同类框按置信度降序做 NMS 去重，保留项恢复原本的阅读顺序。
fn dedup_boxes(regions: Vec<Region>, iou_thresh: f32) -> Vec<Region> {
    if regions.len() <= 1 {
        return regions;
    }
    let mut order: Vec<usize> = (0..regions.len()).collect();
    order.sort_by(|&a, &b| {
        regions[b]
            .score
            .total_cmp(&regions[a].score)
            .then(a.cmp(&b))
    });
    let mut kept_idx: Vec<usize> = Vec::with_capacity(regions.len());
    for idx in order {
        if kept_idx
            .iter()
            .all(|&k| box_iou(&regions[k], &regions[idx]) <= iou_thresh)
        {
            kept_idx.push(idx);
        }
    }
    kept_idx.sort_unstable();
    let mut slots: Vec<Option<Region>> = regions.into_iter().map(Some).collect();
    kept_idx
        .into_iter()
        .filter_map(|i| slots[i].take())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(x0: f32, y0: f32, x1: f32, y1: f32) -> Region {
        Region {
            x0,
            y0,
            x1,
            y1,
            label: "formula".into(),
            kind: DISPLAY_FORMULA.into(),
            score: 1.0,
        }
    }

    #[test]
    fn crop_stays_in_the_page() {
        let page = Image {
            width: 4,
            height: 3,
            channels: 1,
            data: (0..12).collect(),
        };
        let c = crop_rect(&page, &region(1.0, 1.0, 3.0, 9.0));
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(c.data, [5, 6, 9, 10]);
        let c = crop_rect(&page, &region(9.0, 0.0, 12.0, 2.0));
        assert_eq!(c.width, 0);
    }

    #[test]
    fn lines_inside_a_formula_box_are_found_by_center() {
        let boxes = [region(10.0, 10.0, 20.0, 20.0)];
        let q = |x: f32, y: f32| {
            [
                (x - 1.0, y - 1.0),
                (x + 1.0, y - 1.0),
                (x + 1.0, y + 1.0),
                (x - 1.0, y + 1.0),
            ]
        };
        assert!(inside_any(&q(15.0, 15.0), &boxes));
        assert!(!inside_any(&q(25.0, 15.0), &boxes));
    }

    #[test]
    fn dedup_suppresses_overlapping_lower_score_and_preserves_order() {
        let mut r0 = region(10.0, 10.0, 110.0, 60.0);
        r0.score = 0.55;
        let mut r1 = region(12.0, 11.0, 111.0, 61.0);
        r1.score = 0.85;
        let mut r2 = region(10.0, 100.0, 110.0, 150.0);
        r2.score = 0.70;
        let out = dedup_boxes(vec![r0, r1.clone(), r2.clone()], FORMULA_NMS_IOU);
        assert_eq!(out, vec![r1, r2]);
    }
}
