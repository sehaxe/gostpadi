//! Ромб «если»/переключатель на основной линии: колонки веток,
//! метки да/нет/кейсов, слияние, рельсы «-> конец» (pend) и кружки link.

use super::ctx::{Ctx, Pend};
use super::ifnode::{build_plan, Side};
use super::types::{Anchor, Label};
use crate::ir::Node;

impl Ctx<'_> {
    /// Ромб «если»/переключатель на основной линии.
    pub(super) fn decision_top(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
    ) -> (Option<(f64, f64)>, f64) {
        let (dw, dh) = self.sizes["if"];
        let cy = cursor + self.st.vgap + dh / 2.0;
        self.add("if", 0.0, cy, &nd.text);
        if let Some(p) = prev {
            self.edge(&[p, (0.0, cy - dh / 2.0)], true);
        }
        let vl = (-dw / 2.0, cy);
        let vr = (dw / 2.0, cy);
        let vb = (0.0, cy + dh / 2.0);
        let idxs: Vec<usize> = (0..nd.branches.len())
            .filter(|&i| !nd.branches[i].stmts.is_empty())
            .collect();
        let empty: Vec<String> = nd
            .branches
            .iter()
            .filter(|b| b.stmts.is_empty())
            .map(|b| b.label.clone())
            .collect();
        let n = idxs.len();
        let comb = nd.switch_var.is_some() && n >= 2;
        let top0 = cy + dh / 2.0 + self.st.vgap;
        let y_b = cy + dh / 2.0;
        let pitch = 2.0 * self.nhe + self.st.colgap;
        let base = dw / 2.0 + self.st.hgap + self.nhe;
        let plan = build_plan(&idxs, base, pitch, 0.0, n == 1 && !empty.is_empty());
        if comb {
            self.comb_line(&plan, y_b);
        }
        let mut exits: Vec<Option<(f64, f64, bool, bool)>> = vec![None; nd.branches.len()];
        for &(bi, side, t2, txx) in &plan {
            self.column_entry(
                side,
                t2,
                txx,
                cy,
                y_b,
                top0,
                comb,
                vl,
                vr,
                &nd.branches[bi].label,
            );
            let (yend, dead) = self.render_column(&nd.branches[bi].stmts, txx, top0);
            exits[bi] = Some((txx, yend, nd.branches[bi].to_end && !dead, dead));
        }
        let mut merge_y = y_b;
        for &(bi, side, ..) in &plan {
            let e = exits[bi].unwrap();
            // Колонка на оси (tx = 0) стоит на стволе продолжения: её низ —
            // живой, мёртвый или «-> конец» — задаёт merge_y, иначе ствол
            // (0, merge_y) → следующий блок протыкает её содержимое.
            // Внесъёмные колонки (tx != 0) ствол не трогают.
            if side == Side::Axis || (!e.2 && !e.3) {
                merge_y = merge_y.max(e.1 + self.st.mgap);
            }
        }
        let col_bottom = plan
            .iter()
            .map(|&(bi, ..)| exits[bi].unwrap().1)
            .fold(y_b, f64::max);
        let mut link_bottom = y_b;
        let mut merge_cols: Vec<(f64, f64)> = Vec::new();
        for &(bi, side, _, _) in &plan {
            let e = exits[bi].unwrap();
            if !e.2 {
                if !e.3 {
                    merge_cols.push((e.0, e.1));
                }
                continue;
            }
            // «-> конец»: рельса снаружи колонок всей схемы, кратно сетке
            let (sgn, key) = if side == Side::L {
                (-1.0, 0usize)
            } else {
                (1.0, 1usize)
            };
            let rail = sgn
                * super::geometry::up(
                    base + self.max_tier as f64 * pitch
                        + self.colw / 2.0
                        + self.st.rail
                        + self.pend_count[key] as f64 * self.st.rail_step,
                    self.st.grid,
                );
            self.pend_count[key] += 1;
            if let Some(letter) = nd.branches[bi].link {
                // «конец» на другом листе: кружок-соединитель у ветки
                let ccy = col_bottom + self.st.jog + self.st.vgap + self.st.conn_r;
                self.add("conn", rail, ccy, &letter.to_string());
                self.edge(
                    &[
                        (e.0, e.1),
                        (e.0, col_bottom + self.st.jog),
                        (rail, col_bottom + self.st.jog),
                        (rail, ccy - self.st.conn_r),
                    ],
                    true,
                );
                link_bottom = ccy + self.st.conn_r;
            } else {
                self.pend.push(Pend {
                    x: e.0,
                    y: e.1,
                    cb: col_bottom,
                    rail,
                });
            }
        }
        // шина слияния: один горизонтальный отрезок вместо наложенных
        // хвостов колонок
        self.merge_bus(&merge_cols, 0.0, merge_y);
        let n_merge = plan
            .iter()
            .filter(|&&(bi, ..)| {
                let e = exits[bi].unwrap();
                !e.2 && !e.3
            })
            .count()
            + empty.len();
        let has_axis = plan.iter().any(|p| p.1 == Side::Axis);
        if !empty.is_empty() {
            merge_y = merge_y.max(y_b + 2.0 * self.st.grid);
        }
        // Рельса пустой ветки спускается только до merge_y этого ромба,
        // поэтому снаружи достаточно собственных правых колонок; глобальный
        // запас max_tier*pitch + colw законен только для pend-рельсов
        // «-> конец», проходящих сквозь всю схему.
        let right_extent = plan
            .iter()
            .filter(|p| p.1 == Side::R)
            .map(|p| p.3 + self.nhe)
            .fold(dw / 2.0, f64::max);
        for (k, lbl) in empty.iter().enumerate() {
            let bx = super::geometry::up(right_extent + 2.0 * self.st.grid, self.st.grid)
                + k as f64 * 2.0 * self.st.grid;
            self.edge(&[vr, (bx, cy), (bx, merge_y), (0.0, merge_y)], false);
            self.labels.push(Label {
                x: dw / 2.0 + self.st.label_exit_dx + k as f64 * 2.0 * self.st.grid,
                y: cy - self.st.label_dy,
                text: lbl.clone(),
                ha: "center".into(),
            });
        }
        if n_merge == 0 && empty.is_empty() && !has_axis {
            self.edge(&[vb, (0.0, merge_y)], false);
        }
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { x: 0.0, y: cursor });
        (Some((0.0, merge_y)), cursor)
    }
}
