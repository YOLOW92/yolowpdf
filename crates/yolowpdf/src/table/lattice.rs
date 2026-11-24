//! 有框线表格检测（基于 pdfplumber lattice 算法的纯 Rust 实现）。
//! 线段吸附、连接并求交点，从交点寻找闭合矩形作为单元格，共享角点的单元格聚类为独立表格。

use crate::interp::Rule;

/// 吸附、连接、求交点的容差（页面单位，对标 pdfplumber 默认值）。
pub(crate) const SNAP: f64 = 3.0;
pub(crate) const JOIN: f64 = 3.0;
pub(crate) const TOL: f64 = 1.0;
/// 交点上限保护，防止复杂矢量图引发平方级开销。
pub(crate) const MAX_POINTS: usize = 3000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HEdge {
    pub(crate) y: f64,
    pub(crate) x0: f64,
    pub(crate) x1: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VEdge {
    pub(crate) x: f64,
    pub(crate) y0: f64,
    pub(crate) y1: f64,
}

/// 坐标相近（差不超过 `tol`）的聚成一组，每组取平均值。
pub(crate) fn snap(values: &[f64], tol: f64) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut out = vec![0.0; values.len()];
    let mut group: Vec<usize> = Vec::new();
    let flush = |group: &mut Vec<usize>, out: &mut Vec<f64>| {
        if group.is_empty() {
            return;
        }
        let mean = group.iter().map(|&i| values[i]).sum::<f64>() / group.len() as f64;
        for &i in group.iter() {
            out[i] = mean;
        }
        group.clear();
    };
    for i in idx {
        if let Some(&last) = group.last()
            && values[i] - values[last] > tol
        {
            flush(&mut group, &mut out);
        }
        group.push(i);
    }
    flush(&mut group, &mut out);
    out
}

/// 吸附后，同一条直线上首尾相接（间隔不超过 JOIN）的线段连成一条。
pub(crate) fn merge(rules: &[Rule]) -> (Vec<HEdge>, Vec<VEdge>) {
    let hs: Vec<&Rule> = rules.iter().filter(|r| r.horizontal()).collect();
    let vs: Vec<&Rule> = rules.iter().filter(|r| !r.horizontal()).collect();
    let hy = snap(&hs.iter().map(|r| r.y0).collect::<Vec<_>>(), SNAP);
    let vx = snap(&vs.iter().map(|r| r.x0).collect::<Vec<_>>(), SNAP);
    let mut h: Vec<HEdge> = hs
        .iter()
        .zip(&hy)
        .map(|(r, &y)| HEdge {
            y,
            x0: r.x0,
            x1: r.x1,
        })
        .collect();
    let mut v: Vec<VEdge> = vs
        .iter()
        .zip(&vx)
        .map(|(r, &x)| VEdge {
            x,
            y0: r.y0,
            y1: r.y1,
        })
        .collect();
    h.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x0.total_cmp(&b.x0)));
    v.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y0.total_cmp(&b.y0)));
    let mut hj: Vec<HEdge> = Vec::new();
    for e in h {
        match hj.last_mut() {
            Some(l) if l.y == e.y && e.x0 <= l.x1 + JOIN => l.x1 = l.x1.max(e.x1),
            _ => hj.push(e),
        }
    }
    let mut vj: Vec<VEdge> = Vec::new();
    for e in v {
        match vj.last_mut() {
            Some(l) if l.x == e.x && e.y0 <= l.y1 + JOIN => l.y1 = l.y1.max(e.y1),
            _ => vj.push(e),
        }
    }
    (hj, vj)
}

/// 有框线的表格（pdfplumber 的 lattice 算法，采用哈希交点索引 + 二分查找 + 并查集聚类，复杂度 O(P log P)）。
/// 返回每张表的单元格（x0, top, x1, bottom）。
pub(crate) fn lattice(h: &[HEdge], v: &[VEdge]) -> Vec<Vec<(f64, f64, f64, f64)>> {
    use std::collections::HashMap;

    let bits = |v: f64| {
        if v == 0.0 {
            0.0f64.to_bits()
        } else {
            v.to_bits()
        }
    };
    // 交点索引：(x, y) -> points 下标（merge 已把相近坐标吸附成精确相等的 f64）
    let mut points: Vec<(f64, f64, Vec<usize>, Vec<usize>)> = Vec::new();
    let mut coord_to_idx: HashMap<(u64, u64), usize> = HashMap::new();
    for (vi, ve) in v.iter().enumerate() {
        // h 已按 y 升序排好：二分定位 [ve.y0 - TOL, ve.y1 + TOL]
        let lo = h.partition_point(|he| he.y < ve.y0 - TOL);
        for (offset, he) in h[lo..].iter().enumerate() {
            if he.y > ve.y1 + TOL {
                break;
            }
            if he.x0 - TOL <= ve.x && ve.x <= he.x1 + TOL {
                let hi = lo + offset;
                let key = (bits(ve.x), bits(he.y));
                match coord_to_idx.get(&key).copied() {
                    Some(idx) => {
                        points[idx].2.push(vi);
                        points[idx].3.push(hi);
                    }
                    None => {
                        let idx = points.len();
                        coord_to_idx.insert(key, idx);
                        points.push((ve.x, he.y, vec![vi], vec![hi]));
                        if points.len() > MAX_POINTS {
                            return Vec::new();
                        }
                    }
                }
            }
        }
    }
    if points.len() < 4 {
        return Vec::new();
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.total_cmp(&a.1)));
    coord_to_idx.clear();
    let mut by_x: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut by_y: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, p) in points.iter().enumerate() {
        coord_to_idx.insert((bits(p.0), bits(p.1)), i);
        by_x.entry(bits(p.0)).or_default().push(i);
        by_y.entry(bits(p.1)).or_default().push(i);
    }
    let connects = |a: usize, b: usize| {
        let (p, q) = (&points[a], &points[b]);
        (p.0 == q.0 && p.2.iter().any(|e| q.2.contains(e)))
            || (p.1 == q.1 && p.3.iter().any(|e| q.3.contains(e)))
    };
    let mut cells: Vec<(f64, f64, f64, f64)> = Vec::new();
    for i in 0..points.len() {
        let (x, y) = (points[i].0, points[i].1);
        let col = &by_x[&bits(x)];
        let row = &by_y[&bits(y)];
        // col 中按 y 降序排，row 中按 x 升序排（因 points 按 x 升序、y 降序排好）
        let ci = col.partition_point(|&j| j <= i);
        let ri = row.partition_point(|&j| j <= i);
        'found: for &b in &col[ci..] {
            if !connects(i, b) {
                continue;
            }
            for &r in &row[ri..] {
                if !connects(i, r) {
                    continue;
                }
                if let Some(&br) = coord_to_idx.get(&(bits(points[r].0), bits(points[b].1)))
                    && connects(br, r)
                    && connects(br, b)
                {
                    cells.push((x, y, points[r].0, points[b].1));
                    break 'found;
                }
            }
        }
    }
    if cells.is_empty() {
        return Vec::new();
    }
    // 并查集（DSU）：共享角点的单元格聚为同一张表，O(C α(C))
    let mut parent: Vec<usize> = (0..cells.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut corner_owner: HashMap<(u64, u64), usize> = HashMap::with_capacity(cells.len() * 2);
    for (ci, c) in cells.iter().enumerate() {
        for (px, py) in [(c.0, c.1), (c.0, c.3), (c.2, c.1), (c.2, c.3)] {
            let key = (bits(px), bits(py));
            match corner_owner.get(&key).copied() {
                Some(prev) => {
                    let ra = find(&mut parent, prev);
                    let rb = find(&mut parent, ci);
                    if ra != rb {
                        parent[rb] = ra;
                    }
                }
                None => {
                    corner_owner.insert(key, ci);
                }
            }
        }
    }
    type Cell = (f64, f64, f64, f64);
    let mut groups: Vec<(usize, Vec<Cell>)> = Vec::new();
    let mut root_slot: HashMap<usize, usize> = HashMap::new();
    for (ci, &c) in cells.iter().enumerate() {
        let root = find(&mut parent, ci);
        match root_slot.get(&root).copied() {
            Some(s) => groups[s].1.push(c),
            None => {
                let s = groups.len();
                root_slot.insert(root, s);
                groups.push((root, vec![c]));
            }
        }
    }
    // 多格真实表格优先于单格外框，同类内按外框面积升序（内层表格先认领片段，防止外层页框吞并整页）
    let bbox_area = |g: &[(f64, f64, f64, f64)]| {
        let x0 = g.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let top = g.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
        let x1 = g.iter().map(|c| c.2).fold(f64::NEG_INFINITY, f64::max);
        let bot = g.iter().map(|c| c.3).fold(f64::INFINITY, f64::min);
        (x1 - x0).max(0.0) * (top - bot).max(0.0)
    };
    let mut out: Vec<Vec<(f64, f64, f64, f64)>> = groups.into_iter().map(|(_, g)| g).collect();
    out.sort_by(|a, b| {
        (a.len() < 2)
            .cmp(&(b.len() < 2))
            .then_with(|| bbox_area(a).total_cmp(&bbox_area(b)))
    });
    out
}
