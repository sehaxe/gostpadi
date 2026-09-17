use super::ctx::{BreakAt, Ctx};
use super::measure::kind_name;
use super::types::Sizes;
use crate::ir::{Branch, NodeKind, Stmt};
use crate::style::Style;

/// Инструкция return — тупик ветки (порт gostpadi.py `^return\b`).
pub(super) fn is_return(t: &str) -> bool {
    match t.trim_start().strip_prefix("return") {
        None => false,
        Some(rest) => {
            rest.is_empty()
                || rest
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_alphanumeric() && c != '_')
        }
    }
}

/// Полуширина содержимого колонки вокруг её оси: плитки, вложенные
/// ромбы с их под-колонками, вложенные циклы с каналами возврата.
pub(super) fn extent(sizes: &Sizes, st: &Style, colw: f64, items: &[Stmt]) -> f64 {
    let mut he = colw / 2.0;
    for it in items {
        let Stmt::Node(nd) = it else { continue };
        let dw2 = sizes["if"].0;
        if nd.kind == NodeKind::Decision {
            let ne: Vec<&Branch> = nd.branches.iter().filter(|b| !b.stmts.is_empty()).collect();
            let sub = ne
                .iter()
                .map(|b| extent(sizes, st, colw, &b.stmts))
                .fold(colw / 2.0, f64::max);
            let n = ne.len();
            let tiers = n.saturating_sub(1) / 2;
            let pitch2 = 2.0 * sub + st.colgap;
            he = he
                .max(dw2 / 2.0 + st.hgap + sub + tiers as f64 * pitch2)
                .max(super::geometry::up(
                    dw2 / 2.0 + 2.0 * st.grid + tiers as f64 * pitch2 + sub,
                    st.grid,
                ));
        } else {
            // цикл: две трапеции + тело
            he = he.max(sizes["loop"].0 / 2.0);
            if let Some(body) = &nd.body {
                he = he.max(extent(sizes, st, colw, body) + st.mgap);
            }
        }
    }
    he
}

impl Ctx<'_> {
    pub(super) fn extent(&self, items: &[Stmt]) -> f64 {
        extent(self.sizes, self.st, self.colw, items)
    }

    /// Колонка на абсциссе tx: плитки и вложенные «если»/циклы.
    /// Возвращает (y низа, тупик): тупик — колонку завершил return,
    /// из него линии не выходят, хвост колонки недостижим.
    pub(super) fn render_column(&mut self, items: &[Stmt], tx: f64, top0: f64) -> (f64, bool) {
        let mut prev_bottom: Option<f64> = None;
        for it in items {
            let top = match prev_bottom {
                None => top0,
                Some(pb) => pb + self.st.vgap,
            };
            match it {
                Stmt::Node(nd) => {
                    if let Some(pb) = prev_bottom {
                        self.edge(&[(tx, pb), (tx, top)], true);
                    }
                    let (y_bot, dead) = if nd.kind == NodeKind::Decision {
                        self.sub_if(nd, tx, top)
                    } else {
                        self.sub_loop(nd, tx, top)
                    };
                    if dead {
                        return (y_bot, true);
                    }
                    prev_bottom = Some(y_bot);
                }
                Stmt::Text(t) => {
                    if is_return(t) {
                        let (_, h_r) = self.sizes["ret"];
                        if let Some(pb) = prev_bottom {
                            self.edge(&[(tx, pb), (tx, top)], true);
                        }
                        self.add("ret", tx, top + h_r / 2.0, t);
                        return (top + h_r, true);
                    }
                    // break: рельса влево мимо loop_end, T-стык ниже цикла
                    if self.loop_depth > 0 && t.trim() == "break" {
                        self.breaks.push(BreakAt {
                            tx,
                            y: prev_bottom.unwrap_or(top0),
                        });
                        continue;
                    }
                    let k = if self.st.is_io(t) { "io" } else { "act" };
                    let (_, h_k) = self.sizes[k];
                    let bottom = top + h_k;
                    self.add(k, tx, top + h_k / 2.0, t);
                    if let Some(pb) = prev_bottom {
                        self.edge(&[(tx, pb), (tx, top)], true);
                    }
                    prev_bottom = Some(bottom);
                }
            }
        }
        (prev_bottom.unwrap_or(top0), false)
    }

    /// Простой блок на основной линии. merge_stop: Some(y) — входная
    /// линия кончается T-стыком на y без стрелки (слияние pend над End).
    pub(super) fn simple(
        &mut self,
        nd: &crate::ir::Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
        merge_stop: Option<f64>,
    ) -> ((f64, f64), f64, usize) {
        let key = kind_name(&nd.kind);
        let (_, h) = self.sizes[key];
        let cy = cursor + self.st.vgap + h / 2.0;
        let i = self.add(key, 0.0, cy, &nd.text);
        match (prev, merge_stop) {
            (Some(p), Some(my)) => self.edge(&[p, (0.0, my)], false),
            (Some(p), None) => self.edge(&[p, (0.0, cy - h / 2.0)], true),
            _ => {}
        }
        self.anchors.push(super::Anchor {
            x: 0.0,
            y: cy + h / 2.0,
        });
        ((0.0, cy + h / 2.0), cy + h / 2.0, i)
    }
}
