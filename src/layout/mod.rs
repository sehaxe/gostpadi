//! Основной проход раскладки: главная линия по оси x=0.

mod column;
mod ctx;
mod geometry;
mod ifnode;
mod iftop;
mod loopnode;
mod measure;
mod split;
#[cfg(test)]
mod tests;
mod types;

pub use geometry::{crossings_ok, overlaps_ok, single_entry_ok};
pub use measure::{measure, normalize, uniform_sizes};
pub use split::split_scheme;
pub use types::{Anchor, Edge, Label, Layout, Shape, Sizes};

use ctx::Ctx;

use crate::ir::{Node, NodeKind};
use crate::style::Style;

/// conn-узлы подряд прямо перед терминатором «конец» — входящие
/// кружки с других листов: рисуются слева от «конца» на его высоте.
fn leads_to_end(nodes: &[Node], mut j: usize) -> bool {
    j += 1;
    while j < nodes.len() && nodes[j].kind == NodeKind::Conn {
        j += 1;
    }
    j == nodes.len().saturating_sub(1) && nodes.get(j).is_some_and(|n| n.kind == NodeKind::Term)
}

/// Схема -> (фигуры, рёбра, подписи, границы, якоря узлов) в пунктах.
/// Фигуры kind: term/ret/io/act/if/loop_begin/loop_end/conn.
/// Ребро: ломаная из сегментов строго 90°; стрелка ставится в конце,
/// если arrow. Слияния (T-стыки) рисуются с arrow=false.
pub fn layout(nodes: &[Node], sizes: &Sizes, st: &Style) -> Layout {
    if nodes.is_empty() {
        return Layout::default();
    }
    let mut c = Ctx::new(nodes, sizes, st);
    let last = nodes.len().saturating_sub(1);
    let mut prev: Option<(f64, f64)> = None;
    let mut cursor = 0.0f64;
    for (idx, nd) in nodes.iter().enumerate() {
        match nd.kind {
            NodeKind::Loop => {
                let r = c.loop_top(nd, prev, cursor);
                prev = r.0;
                cursor = r.1;
            }
            NodeKind::Return => {
                c.simple(nd, prev, cursor, None);
                break; // из return линии не выходят — схема завершена
            }
            NodeKind::Decision => {
                let r = c.decision_top(nd, prev, cursor);
                prev = r.0;
                cursor = r.1;
            }
            _ => {
                if nd.kind == NodeKind::Conn && leads_to_end(nodes, idx) {
                    c.inbound.push(nd.text.clone());
                    c.anchors.push(Anchor { x: 0.0, y: cursor });
                    continue;
                }
                let pend_here = idx == last && !c.pend.is_empty();
                if pend_here {
                    let py = c.pend.iter().map(|p| p.y).fold(cursor, f64::max);
                    cursor = cursor.max(py);
                }
                // T-точка слияния pend-рельс над «концом» (single-entry)
                let my = cursor + st.vgap / 2.0;
                let stop = if pend_here { Some(my) } else { None };
                let (_, cur, sh_i) = c.simple(nd, prev, cursor, stop);
                cursor = cur;
                prev = Some((0.0, cur));
                if idx == last {
                    let sh = c.shapes[sh_i].clone();
                    let (stop_l, top) = (sh.cx - sh.w / 2.0, sh.cy - sh.h / 2.0);
                    // кружки ставим левее рельс «-> конец», иначе рельсы
                    // проходят сквозь них; без рельс — как раньше
                    let left_rail = c.pend.iter().map(|p| p.rail).fold(f64::INFINITY, f64::min);
                    if pend_here {
                        for p in std::mem::take(&mut c.pend) {
                            c.edge(
                                &[
                                    (p.x, p.y),
                                    (p.x, p.cb + st.jog),
                                    (p.rail, p.cb + st.jog),
                                    (p.rail, my),
                                    (0.0, my),
                                ],
                                false,
                            );
                        }
                        // единственная стрелка в «конец»
                        c.edge(&[(0.0, my), (0.0, top)], true);
                    }
                    // кружки левее рельс «-> конец»; цепочка друг за другом:
                    // каждое ребро — к соседнему, стрелка только в «конец».
                    // Рельсы бывают и справа: якорь — крайняя ЛЕВАЯ рельса,
                    // а правее кромки «конца» кружки не ставим.
                    let mut prev_cx = stop_l;
                    let anchor = left_rail.min(stop_l);
                    for (i, letter) in std::mem::take(&mut c.inbound).iter().enumerate() {
                        let cx_ = anchor
                            - st.conn_step
                            - st.conn_r
                            - i as f64 * (2.0 * st.conn_r + st.conn_step);
                        c.add("conn", cx_, sh.cy, letter);
                        let target = if i == 0 { stop_l } else { prev_cx - st.conn_r };
                        c.edge(&[(cx_ + st.conn_r, sh.cy), (target, sh.cy)], i == 0);
                        prev_cx = cx_;
                    }
                }
            }
        }
    }
    // pend-рельсы, не дошедшие до простого блока: последний узел —
    // цикл, ромб или return. Рельсы вливаются в магистраль под ним
    // (стрелка в ствол), как под «концом» в простой ветке.
    if !c.pend.is_empty() {
        let py = c.pend.iter().map(|p| p.y).fold(cursor, f64::max);
        cursor = cursor.max(py);
        let my = cursor;
        for p in std::mem::take(&mut c.pend) {
            c.edge(
                &[
                    (p.x, p.y),
                    (p.x, p.cb + st.jog),
                    (p.rail, p.cb + st.jog),
                    (p.rail, my),
                    (0.0, my),
                ],
                true,
            );
        }
    }
    let l = c.finish();
    #[cfg(debug_assertions)]
    geometry::assert_no_crossing(&l.shapes, &l.edges);
    l
}
