//! Общий контекст раскладки: холст (фигуры/рёбра/подписи) и предрасчёт
//! единых полуширин колонок (nhe), рельсов «-> конец» (max_tier).

use super::types::{Anchor, Edge, Label, Layout, Shape, Sizes};
use crate::ir::{Node, NodeKind, Stmt};
use crate::style::Style;

/// Ветка «-> конец»: ждёт блока «конец» (слияние T-узлом над ним).
pub(super) struct Pend {
    pub x: f64,
    pub y: f64,
    pub cb: f64,
    pub rail: f64,
}

/// break внутри тела цикла: точка ухода на левую рельсу.
pub(super) struct BreakAt {
    pub tx: f64,
    pub y: f64,
}

pub(super) struct Ctx<'a> {
    pub st: &'a Style,
    pub sizes: &'a Sizes,
    pub colw: f64,
    pub nhe: f64,
    pub max_tier: usize,
    pub shapes: Vec<Shape>,
    pub edges: Vec<Edge>,
    pub labels: Vec<Label>,
    pub anchors: Vec<Anchor>,
    pub pend: Vec<Pend>,
    pub pend_count: [usize; 2],
    pub inbound: Vec<String>,
    pub loop_depth: usize,
    pub break_slot: usize,
    pub breaks: Vec<BreakAt>,
}

impl<'a> Ctx<'a> {
    pub(super) fn new(nodes: &[Node], sizes: &'a Sizes, st: &'a Style) -> Self {
        let colw = sizes["act"].0.max(sizes["io"].0);
        let mut nhe = colw / 2.0;
        let mut max_tier = 0;
        for scan in nodes {
            match scan.kind {
                NodeKind::Decision => {
                    let m = scan.branches.iter().filter(|b| !b.stmts.is_empty()).count();
                    max_tier = max_tier.max(m.saturating_sub(1) / 2);
                    match super::ifnode::cascade_cols(scan) {
                        Some((k, c)) if k >= 2 => {
                            // ярусы каскада: рельсы «-> конец» обязаны
                            // обходить его внешние колонки
                            max_tier = max_tier.max(c.saturating_sub(1) / 2);
                            // колонки каскада: да-звенья и хвост; «нет»-ветка
                            // с вложенным ромбом не рендерится как колонка
                            // и nhe не раздувает
                            let mut cur = scan;
                            loop {
                                nhe = nhe.max(super::column::extent(
                                    sizes,
                                    st,
                                    colw,
                                    &cur.branches[0].stmts,
                                ));
                                let last = cur.branches.last().unwrap();
                                match last.stmts.as_slice() {
                                    [Stmt::Node(d)] if d.kind == NodeKind::Decision => cur = d,
                                    _ => {
                                        if !last.stmts.is_empty() {
                                            nhe = nhe.max(super::column::extent(
                                                sizes,
                                                st,
                                                colw,
                                                &last.stmts,
                                            ));
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {
                            for b in &scan.branches {
                                if !b.stmts.is_empty() {
                                    nhe = nhe.max(super::column::extent(sizes, st, colw, &b.stmts));
                                }
                            }
                        }
                    }
                }
                NodeKind::Loop if scan.body.is_some() => {
                    nhe = nhe.max(sizes["loop"].0 / 2.0).max(
                        super::column::extent(sizes, st, colw, scan.body.as_ref().unwrap())
                            + st.mgap,
                    );
                }
                _ => {}
            }
        }
        Ctx {
            st,
            sizes,
            colw,
            nhe,
            max_tier,
            shapes: Vec::new(),
            edges: Vec::new(),
            labels: Vec::new(),
            anchors: Vec::new(),
            pend: Vec::new(),
            pend_count: [0, 0],
            inbound: Vec::new(),
            loop_depth: 0,
            break_slot: 0,
            breaks: Vec::new(),
        }
    }

    pub(super) fn add(&mut self, kind: &str, cx: f64, cy: f64, text: &str) -> usize {
        let key = match kind {
            "loop_begin" | "loop_end" => "loop",
            k => k,
        };
        let (w, h) = self.sizes[key];
        // ГОСТ 19.701: срез углов трапеций цикла под 45°: Δ = h/2,
        // но не более w/4 (защита от дурацких пропорций)
        const SKEW_45_H: f64 = 0.5;
        const SKEW_MAX_W: f64 = 0.25;
        let skew = match kind {
            "loop_begin" | "loop_end" => (SKEW_45_H * h).min(SKEW_MAX_W * w),
            _ => 0.0,
        };
        self.shapes.push(Shape {
            kind: kind.into(),
            cx,
            cy,
            w,
            h,
            skew,
            lines: text.split('\n').map(str::to_string).collect(),
        });
        self.shapes.len() - 1
    }

    pub(super) fn edge(&mut self, pts: &[(f64, f64)], arrow: bool) {
        let mut clean = vec![pts[0]];
        for &p in &pts[1..] {
            if p != clean[clean.len() - 1] {
                clean.push(p);
            }
        }
        if clean.len() >= 2 {
            self.edges.push(Edge {
                points: clean,
                arrow,
            });
        }
    }

    pub(super) fn finish(self) -> Layout {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for e in &self.edges {
            for &(x, y) in &e.points {
                xs.push(x);
                ys.push(y);
            }
        }
        for sh in &self.shapes {
            xs.extend([sh.cx - sh.w / 2.0, sh.cx + sh.w / 2.0]);
            ys.extend([sh.cy - sh.h / 2.0, sh.cy + sh.h / 2.0]);
        }
        for l in &self.labels {
            // оценка ширины подписи: половина глифа на символ (моноширинный)
            let half = l.text.chars().count() as f64 * self.st.char_w / 2.0;
            match l.ha.as_str() {
                "right" => xs.push(l.x - half),
                "left" => xs.push(l.x + half),
                _ => {
                    xs.push(l.x - half);
                    xs.push(l.x + half);
                }
            }
            ys.push(l.y);
        }
        let pad = self.st.page_pad;
        let minx = xs.iter().cloned().fold(f64::MAX, f64::min);
        let miny = ys.iter().cloned().fold(f64::MAX, f64::min);
        let maxx = xs.iter().cloned().fold(f64::MIN, f64::max);
        let maxy = ys.iter().cloned().fold(f64::MIN, f64::max);
        Layout {
            shapes: self.shapes,
            edges: self.edges,
            labels: self.labels,
            bounds: (
                minx - pad,
                miny - pad,
                maxx - minx + 2.0 * pad,
                maxy - miny + 2.0 * pad,
            ),
            anchors: self.anchors,
        }
    }
}
