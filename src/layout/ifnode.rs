use super::column::rail_only;
use super::ctx::{BreakAt, ColEnd, Ctx};
use super::types::Label;
use crate::ir::{Node, NodeKind, Stmt};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Side {
    L,
    R,
    Axis,
}

/// План колонок: (индекс ветки, сторона, ярус, абсцисса колонки).
pub(super) fn build_plan(
    idxs: &[usize],
    base: f64,
    pitch: f64,
    tx0: f64,
    single_left: bool,
) -> Vec<(usize, Side, usize, f64)> {
    let n = idxs.len();
    let mut plan = Vec::new();
    if single_left {
        plan.push((idxs[0], Side::L, 0, tx0 - base));
        return plan;
    }
    for (j, &bi) in idxs.iter().enumerate() {
        if n % 2 == 1 && j == n / 2 {
            plan.push((bi, Side::Axis, 0, tx0));
        } else if j < n / 2 {
            let t = n / 2 - 1 - j;
            plan.push((bi, Side::L, t, tx0 - (base + t as f64 * pitch)));
        } else if n > 0 {
            let t = j - n / 2 - (n % 2);
            plan.push((bi, Side::R, t, tx0 + base + t as f64 * pitch));
        }
    }
    plan
}

/// Каскад else-if: последняя ветка содержит ровно один вложенный
/// Decision (рекурсивно), у каждого звена ровно 2 ветки, первая
/// непуста, без switch. Возвращает (ромбы, колонки): колонки = ромбы
/// плюс хвост-else, если тот непуст (иначе рельса). Ромбы < 2 —
/// обычный if/else, каскадом не рендерится.
pub(super) fn cascade_cols(nd: &Node) -> Option<(usize, usize)> {
    let mut k = 0usize;
    let mut cur = nd;
    loop {
        if cur.switch_var.is_some() || cur.branches.len() != 2 || cur.branches[0].stmts.is_empty() {
            return if k == 0 { None } else { Some((k, k)) };
        }
        k += 1;
        let last = cur.branches.last().unwrap();
        match last.stmts.as_slice() {
            [Stmt::Node(d)] if d.kind == NodeKind::Decision => cur = d,
            _ => return Some((k, k + usize::from(!last.stmts.is_empty()))),
        }
    }
}

impl Ctx<'_> {
    /// Слияние колонок на одну шину: вертикальные спуски остаются по
    /// колонкам, а горизонталь на уровне my рисуется одним отрезком от
    /// крайней колонки до крайней через target — наложенные хвосты
    /// отдельных колонок давали ступеньки разной жирности.
    pub(super) fn merge_bus(&mut self, cols: &[(f64, f64)], target: f64, my: f64) {
        if cols.is_empty() {
            return;
        }
        if cols.len() == 1 {
            let (x, y) = cols[0];
            self.edge(&[(x, y), (x, my), (target, my)], false);
            return;
        }
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for &(x, _) in cols {
            lo = lo.min(x);
            hi = hi.max(x);
        }
        for &(x, y) in cols {
            self.edge(&[(x, y), (x, my)], false);
        }
        self.edge(&[(lo.min(target), my), (hi.max(target), my)], false);
    }

    /// Спуск колонки ветки из ромба: ребро входа + метка да/нет/кейса.
    /// arrow=false — колонка без первой плитки (только break/continue):
    /// стрелка в пустоту запрещена, поток ведёт рельса или слияние.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn column_entry(
        &mut self,
        side: Side,
        t2: usize,
        txx: f64,
        cy: f64,
        y_b: f64,
        top2: f64,
        comb: bool,
        vl: (f64, f64),
        vr: (f64, f64),
        label: &str,
        arrow: bool,
    ) {
        let st = self.st;
        match side {
            Side::Axis => {
                self.edge(&[(txx, y_b), (txx, top2)], arrow);
                self.labels.push(Label {
                    x: txx + st.label_axis_dx,
                    y: top2 - st.label_dy,
                    text: label.into(),
                    ha: "left".into(),
                });
            }
            _ if comb => {
                self.edge(&[(txx, y_b), (txx, top2)], arrow);
                let left = side == Side::L;
                self.labels.push(Label {
                    x: txx + if left { -st.label_dx } else { st.label_dx },
                    y: top2 - st.label_dy,
                    text: label.into(),
                    ha: if left { "right" } else { "left" }.into(),
                });
            }
            _ => {
                let v = if side == Side::L { vl } else { vr };
                self.edge(&[v, (txx, cy), (txx, top2)], arrow);
                let x = if t2 == 0 {
                    v.0 + if side == Side::L {
                        -st.label_exit_dx
                    } else {
                        st.label_exit_dx
                    }
                } else {
                    (v.0 + txx) / 2.0
                };
                self.labels.push(Label {
                    x,
                    y: cy - st.label_dy,
                    text: label.into(),
                    ha: "center".into(),
                });
            }
        }
    }

    pub(super) fn comb_line(&mut self, plan: &[(usize, Side, usize, f64)], y_b: f64) {
        if plan.len() < 2 {
            return;
        }
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for &(_, _, _, x) in plan {
            lo = lo.min(x);
            hi = hi.max(x);
        }
        self.edge(&[(lo, y_b), (hi, y_b)], false);
    }

    /// Вложенный ромб внутри колонки: под-колонки вокруг tx, слияние
    /// обратно на ось колонки. Возвращает (y продолжения, конец).
    /// Для switch (switch_var) — это ещё и scope: кейс-колонки напрямую
    /// растворяют break в слиянии, вложенные уходят рельсой к выходу.
    pub(super) fn sub_if(&mut self, nd: &Node, tx: f64, top: f64) -> (f64, ColEnd) {
        let (dw, dh) = self.sizes["if"];
        let cy = top + dh / 2.0;
        self.add("if", tx, cy, &nd.text);
        let vl = (tx - dw / 2.0, cy);
        let vr = (tx + dw / 2.0, cy);
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
        let sub = idxs
            .iter()
            .map(|&i| self.extent(&nd.branches[i].stmts))
            .fold(self.colw / 2.0, f64::max);
        let pitch2 = 2.0 * sub + self.st.colgap;
        let base2 = dw / 2.0 + self.st.hgap + sub;
        let comb = nd.switch_var.is_some() && n >= 2;
        let top2 = cy + dh / 2.0 + self.st.vgap;
        let y_b = cy + dh / 2.0;
        let plan = build_plan(&idxs, base2, pitch2, tx, n == 1 && !empty.is_empty());
        if comb {
            self.comb_line(&plan, y_b);
        }
        let is_switch = nd.switch_var.is_some();
        let scoped = is_switch || self.loop_depth > 0 || self.switch_depth > 0;
        let saved_direct = self.case_direct;
        let br_mark = self.breaks.len();
        if is_switch {
            self.switch_depth += 1;
            self.case_direct = true;
        } else {
            self.case_direct = false;
        }
        let mut bottoms: Vec<(usize, f64, ColEnd)> = Vec::new();
        for &(bi, side, t2, txx) in &plan {
            let arrow = !(scoped && rail_only(&nd.branches[bi].stmts));
            self.column_entry(
                side,
                t2,
                txx,
                cy,
                y_b,
                top2,
                comb,
                vl,
                vr,
                &nd.branches[bi].label,
                arrow,
            );
            let (y, end) = self.render_column(&nd.branches[bi].stmts, txx, top2);
            bottoms.push((bi, y, end));
        }
        self.case_direct = saved_direct;
        if is_switch {
            self.switch_depth -= 1;
        }
        let mut merge2 = y_b;
        // Осевая под-колонка (txx == tx) стоит на продолжении колонки:
        // её низ учитывается даже если колонка мёртвая (return), иначе
        // спуск продолжения протыкает ret насквозь.
        for (&(_, side, ..), &(_, y, end)) in plan.iter().zip(bottoms.iter()) {
            if side == Side::Axis || end == ColEnd::Flow {
                merge2 = merge2.max(y + self.st.mgap);
            }
        }
        let cols: Vec<(f64, f64)> = bottoms
            .iter()
            .filter(|&&(_, _, end)| end == ColEnd::Flow)
            .map(|&(bi, y, _)| (plan.iter().find(|p| p.0 == bi).unwrap().3, y))
            .collect();
        self.merge_bus(&cols, tx, merge2);
        // рельсы break из веток внутри кейсов (if внутри case): наружу
        // за колонки switch, вниз к слиянию switch, T-стык на стволе
        if is_switch {
            let nested: Vec<BreakAt> = self.breaks.drain(br_mark..).collect();
            let outer = plan
                .iter()
                .map(|&(.., txx)| (txx - tx).abs())
                .fold(dw / 2.0 + 2.0 * self.st.grid, f64::max)
                + sub
                + self.st.grid;
            for b in &nested {
                let slot = self.break_slot;
                self.break_slot += 1;
                let sgn = if b.tx <= tx { -1.0 } else { 1.0 };
                let rx = tx
                    + sgn
                        * super::geometry::up(
                            outer + slot as f64 * 2.0 * self.st.grid,
                            self.st.grid,
                        );
                self.edge(&[(b.tx, b.y), (rx, b.y), (rx, merge2), (tx, merge2)], false);
            }
        }
        if !empty.is_empty() {
            merge2 = merge2.max(y_b + 2.0 * self.st.grid);
            // рельса жмётся к под-ромбу: пол — вершина + 2g, дальше —
            // только правые под-колонки (зеркальный разлёт давал крюк)
            let clear = plan
                .iter()
                .filter(|p| p.1 == Side::R)
                .map(|p| (p.3 - tx) + self.nhe + self.st.grid)
                .fold(dw / 2.0 + 2.0 * self.st.grid, f64::max);
            let bx2 = tx + super::geometry::up(clear, self.st.grid);
            for (k, lbl) in empty.iter().enumerate() {
                self.edge(
                    &[
                        vr,
                        (bx2 + k as f64 * 2.0 * self.st.grid, cy),
                        (bx2 + k as f64 * 2.0 * self.st.grid, merge2),
                        (tx, merge2),
                    ],
                    false,
                );
                self.labels.push(Label {
                    x: tx + dw / 2.0 + self.st.label_exit_dx,
                    y: cy - self.st.label_dy,
                    text: lbl.clone(),
                    ha: "center".into(),
                });
            }
        }
        let all_dead = !bottoms.is_empty()
            && bottoms.iter().all(|&(_, _, e)| e == ColEnd::Return)
            && empty.is_empty();
        // все ветки ушли рельсами: продолжение колонки недостижимо,
        // разве что есть пустые ветки — те падают сквозь if
        let all_rail = !all_dead
            && !bottoms.is_empty()
            && bottoms.iter().all(|&(_, _, e)| e == ColEnd::Rail)
            && empty.is_empty();
        let end = if all_dead {
            ColEnd::Return
        } else if all_rail {
            ColEnd::Rail
        } else {
            ColEnd::Flow
        };
        (
            bottoms
                .iter()
                .map(|&(_, y, _)| y)
                .fold(y_b, f64::max)
                .max(merge2),
            end,
        )
    }
}
