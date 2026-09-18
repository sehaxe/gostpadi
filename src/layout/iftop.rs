//! Ромб «если»/переключатель на основной линии: колонки веток,
//! метки да/нет/кейсов, слияние, рельсы «-> конец» (pend) и кружки link.

use super::ctx::{Ctx, Pend};
use super::ifnode::{build_plan, cascade_cols, Side};
use super::types::{Anchor, Label};
use crate::ir::{Branch, Node, NodeKind, Stmt};

impl Ctx<'_> {
    /// Ромб «если»/переключатель на основной линии.
    pub(super) fn decision_top(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
    ) -> (Option<(f64, f64)>, f64) {
        if matches!(cascade_cols(nd), Some((k, _)) if k >= 2) {
            return self.decision_cascade(nd, prev, cursor);
        }
        // большой переключатель: кейсы сеткой, по два на ряд
        let n_ne = nd.branches.iter().filter(|b| !b.stmts.is_empty()).count();
        if nd.switch_var.is_some() && n_ne >= 5 {
            return self.switch_rows(nd, prev, cursor);
        }
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
        // Рельса пустой ветки жмётся к ромбу: пол — правая вершина + 2g,
        // дальше вправо — только чтобы очистить правые колонки. Старая
        // «зеркальность самой широкой стороне» при пустом правом боку
        // давала крюк через всю схему: шина росла без контента.
        let clear = plan
            .iter()
            .filter(|p| p.1 == Side::R)
            .map(|p| p.3 + self.nhe + self.st.grid)
            .fold(dw / 2.0 + 2.0 * self.st.grid, f64::max);
        for (k, lbl) in empty.iter().enumerate() {
            let bx = super::geometry::up(clear, self.st.grid) + k as f64 * 2.0 * self.st.grid;
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

    /// Большой переключатель: кейсы сеткой — по два на ряд, ряды друг
    /// под другом. Ширина постоянная (две колонки на ±base), растёт
    /// только вниз — одна длинная шина кейсов ужимала всю страницу.
    /// Шины рядов соединяет ствол на оси; слияние живых колонок —
    /// одна шина под последним рядом.
    fn switch_rows(
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
        let base = dw / 2.0 + self.st.hgap + self.nhe;
        let empty: Vec<String> = nd
            .branches
            .iter()
            .filter(|b| b.stmts.is_empty())
            .map(|b| b.label.clone())
            .collect();
        let cases: Vec<&Branch> = nd.branches.iter().filter(|b| !b.stmts.is_empty()).collect();
        // коридоры за сеткой: сначала рельсы пустых кейсов, потом
        // «-> конец» — чтобы вертикали не накладывались
        let bx0 = super::geometry::up(
            (base + self.nhe + self.st.grid).max(dw / 2.0 + 2.0 * self.st.grid),
            self.st.grid,
        );
        // шина первого ряда: от нижней вершины ромба по стволу
        let mut y_bus = cy + dh / 2.0 + self.st.vgap;
        let mut trunk_from = cy + dh / 2.0;
        self.edge(&[(0.0, trunk_from), (0.0, y_bus)], false);
        let mut col_bottom = y_bus;
        let mut merge_y = y_bus;
        let mut link_bottom = y_bus;
        let mut merged = false;
        for (ri, row) in cases.chunks(2).enumerate() {
            if ri > 0 {
                // ствол от слияния предыдущего ряда к шине следующего
                y_bus = merge_y + self.st.vgap;
                self.edge(&[(0.0, trunk_from), (0.0, y_bus)], false);
            }
            let mut live: Vec<f64> = Vec::new();
            let mut gone: Vec<(f64, f64, bool, Option<char>)> = Vec::new(); // x, yend, to_end, link
            for (ci, b) in row.iter().enumerate() {
                let left = ci == 0;
                let sgn: f64 = if left { -1.0 } else { 1.0 };
                let tx = sgn * base;
                let top = y_bus + self.st.vgap;
                // шина ряда: от ствола до колонки (два сегмента без
                // наложения — вместе образуют шину через ось)
                self.edge(&[(0.0, y_bus), (tx, y_bus)], false);
                self.edge(&[(tx, y_bus), (tx, top)], true);
                self.labels.push(Label {
                    x: tx
                        + if left {
                            -self.st.label_dx
                        } else {
                            self.st.label_dx
                        },
                    y: top - self.st.label_dy,
                    text: b.label.clone(),
                    ha: if left { "right" } else { "left" }.into(),
                });
                let (yend, dead) = self.render_column(&b.stmts, tx, top);
                col_bottom = col_bottom.max(yend);
                if dead {
                    continue;
                }
                if b.to_end {
                    gone.push((tx, yend, true, b.link));
                } else {
                    live.push(tx);
                    gone.push((tx, yend, false, None));
                }
            }
            // слияние ряда: уровень ниже содержимого ряда; капли колонок
            // не могут спускаться ниже — под ними следующие ряды сетки
            merge_y = gone
                .iter()
                .map(|g| g.1)
                .fold(y_bus + 2.0 * self.st.grid, f64::max)
                + self.st.mgap;
            for g in &gone {
                if g.2 {
                    continue; // «-> конец» уходит своим коридором ниже
                }
                self.edge(&[(g.0, g.1), (g.0, merge_y)], false);
            }
            if !live.is_empty() {
                let lo = live.iter().cloned().fold(f64::MAX, f64::min);
                let hi = live.iter().cloned().fold(f64::MIN, f64::max);
                self.edge(&[(lo.min(0.0), merge_y), (hi.max(0.0), merge_y)], false);
                merged = true;
            }
            trunk_from = merge_y;
            // «-> конец»: спуск до уровня слияния ряда, наружу за сетку,
            // дальше рельсу дорисует layout() над «концом»
            for &(tx, yend, _, link) in gone.iter().filter(|g| g.2) {
                let sgn: f64 = if tx > 0.0 { 1.0 } else { -1.0 };
                let key = usize::from(tx > 0.0);
                let out = sgn * (bx0 + empty.len() as f64 * 2.0 * self.st.grid);
                self.edge(&[(tx, yend), (tx, merge_y), (out, merge_y)], false);
                let pitch = 2.0 * self.nhe + self.st.colgap;
                let rail = sgn
                    * super::geometry::up(
                        base + self.max_tier as f64 * pitch
                            + self.colw / 2.0
                            + self.st.rail
                            + self.pend_count[key] as f64 * self.st.rail_step,
                        self.st.grid,
                    );
                self.pend_count[key] += 1;
                if let Some(letter) = link {
                    let ccy = col_bottom + self.st.jog + self.st.vgap + self.st.conn_r;
                    self.add("conn", rail, ccy, &letter.to_string());
                    self.edge(
                        &[
                            (out, merge_y),
                            (out, col_bottom + self.st.jog),
                            (rail, col_bottom + self.st.jog),
                            (rail, ccy - self.st.conn_r),
                        ],
                        true,
                    );
                    link_bottom = link_bottom.max(ccy + self.st.conn_r);
                } else {
                    self.pend.push(Pend {
                        x: out,
                        y: merge_y,
                        cb: col_bottom,
                        rail,
                    });
                }
            }
        }
        // пустые кейсы: рельсы справа, чистят сетку, сливаются на стволе
        if !empty.is_empty() {
            for (k, lbl) in empty.iter().enumerate() {
                let bx = bx0 + k as f64 * 2.0 * self.st.grid;
                self.edge(
                    &[(dw / 2.0, cy), (bx, cy), (bx, merge_y), (0.0, merge_y)],
                    false,
                );
                self.labels.push(Label {
                    x: dw / 2.0 + self.st.label_exit_dx + k as f64 * 2.0 * self.st.grid,
                    y: cy - self.st.label_dy,
                    text: lbl.clone(),
                    ha: "center".into(),
                });
            }
            merged = true;
        }
        if !merged {
            // все живые кейсы в тупиках/«-> конец»: формальное
            // продолжение ствола до точки выхода
            self.edge(&[(0.0, trunk_from), (0.0, merge_y)], false);
        }
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { x: 0.0, y: cursor });
        (Some((0.0, merge_y)), cursor)
    }

    /// Каскад else-if: вертикальный ствол с одной шиной. Ромбы цепочки
    /// на оси друг под другом («нет» — ребро от нижней вершины к верхней,
    /// метка справа от линии), «да» каждого ромба — колонка сбоку,
    /// стороны чередуют L0, R0, L1, R1..., все колонки на одном top0;
    /// хвост-else — следующая свободная сторона, пустой хвост — рельса.
    /// Слияние: спуски живых колонок и ровно одна горизонтальная шина.
    fn decision_cascade(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
    ) -> (Option<(f64, f64)>, f64) {
        let (dw, dh) = self.sizes["if"];
        // развёртка цепочки: (ромб, да-ветка) и хвостовая ветка последнего
        let mut links: Vec<(&Node, &Branch)> = Vec::new();
        let mut cur = nd;
        let tail: &Branch = loop {
            links.push((cur, &cur.branches[0]));
            let last = cur.branches.last().unwrap();
            match last.stmts.as_slice() {
                [Stmt::Node(d)] if d.kind == NodeKind::Decision => cur = d,
                _ => break last,
            }
        };
        let k = links.len();
        let step = dh + self.st.vgap;
        let cy0 = cursor + self.st.vgap + dh / 2.0;
        for (i, (dnd, _)) in links.iter().enumerate() {
            self.add("if", 0.0, cy0 + step * i as f64, &dnd.text);
        }
        if let Some(p) = prev {
            self.edge(&[p, (0.0, cy0 - dh / 2.0)], true);
        }
        // ствол продолжения: «нет» ведёт к следующему ромбу
        for i in 1..k {
            let y0 = cy0 + step * (i - 1) as f64 + dh / 2.0;
            self.edge(&[(0.0, y0), (0.0, y0 + self.st.vgap)], true);
            self.labels.push(Label {
                x: self.st.label_axis_dx,
                y: y0 + self.st.vgap - self.st.label_dy,
                text: links[i - 1].0.branches.last().unwrap().label.clone(),
                ha: "left".into(),
            });
        }
        let top0 = cy0 + dh / 2.0 + self.st.vgap;
        let pitch = 2.0 * self.nhe + self.st.colgap;
        let base = dw / 2.0 + self.st.hgap + self.nhe;
        let y_b_last = cy0 + step * (k - 1) as f64 + dh / 2.0;
        let n_cols = k + usize::from(!tail.stmts.is_empty());
        let mut exits: Vec<(f64, f64, bool, bool, Option<char>, bool)> = Vec::new();
        // да-входы: «гребёнка» на каждой стороне. Общий вертикальный
        // участок в коридоре между кромкой ромбов и колонками
        // (dw/2 + g — внутри полосы hgap = 2g), у каждой ветви своя
        // высота горизонтального обхода НАД колонками (top0 - (ci+1)*g),
        // спуск в свою колонку сверху, стрелка в плитку. Высоты колонок
        // не мешают: длинные горизонтали только над верхом колонок.
        let x_v = dw / 2.0 + self.st.grid;
        let y_over = |ci: usize| top0 - (ci + 1) as f64 * self.st.grid;
        let mut ins: Vec<(f64, f64, f64, usize)> = Vec::new(); // sgn, tx, cy, ci
        for ci in 0..n_cols {
            let left = ci % 2 == 0;
            let sgn: f64 = if left { -1.0 } else { 1.0 };
            let tx = sgn * (base + (ci / 2) as f64 * pitch);
            let cy = cy0 + step * ci.min(k - 1) as f64;
            ins.push((sgn, tx, cy, ci));
        }
        // общий вертикальный участок стороны: от высшей точки обхода до
        // низшей ветви, один раз; ветви-ответвления в него не рисуются
        for sgn in [-1.0, 1.0] {
            let side: Vec<&(f64, f64, f64, usize)> = ins.iter().filter(|e| e.0 == sgn).collect();
            if side.is_empty() {
                continue;
            }
            let y_top = side.iter().map(|e| y_over(e.3)).fold(f64::MAX, f64::min);
            let y_bot = side.iter().map(|e| e.2).fold(f64::MIN, f64::max);
            self.edge(&[(sgn * x_v, y_top), (sgn * x_v, y_bot)], false);
        }
        for &(sgn, tx, cy, ci) in &ins {
            // ветвь: боковая вершина -> гребёнка -> обход над колонками ->
            // спуск в свою колонку сверху
            self.edge(&[(sgn * dw / 2.0, cy), (sgn * x_v, cy)], false);
            self.edge(&[(sgn * x_v, y_over(ci)), (tx, y_over(ci))], false);
            self.edge(&[(tx, y_over(ci)), (tx, top0)], true);
            let ybr = if ci < k { links[ci].1 } else { tail };
            self.labels.push(Label {
                x: sgn * (dw / 2.0 + self.st.label_exit_dx),
                y: cy - self.st.label_dy,
                text: ybr.label.clone(),
                ha: "center".into(),
            });
            let (yend, dead) = self.render_column(&ybr.stmts, tx, top0);
            exits.push((tx, yend, dead, ybr.to_end, ybr.link, sgn < 0.0));
        }
        let col_bottom = exits.iter().map(|e| e.1).fold(y_b_last, f64::max);
        let mut merge_y = y_b_last + 2.0 * self.st.grid;
        for &(_, yend, dead, to_end, _, _) in &exits {
            if !dead && !to_end {
                merge_y = merge_y.max(yend + self.st.mgap);
            }
        }
        // единая шина: спуск каждой живой колонки и ровно один
        // горизонтальный сегмент от крайней левой до крайней правой
        let mut xs: Vec<f64> = vec![0.0];
        for &(tx, yend, dead, to_end, _, _) in &exits {
            if !dead && !to_end {
                self.edge(&[(tx, yend), (tx, merge_y)], false);
                xs.push(tx);
            }
        }
        // «-> конец»: рельса снаружи колонок всей схемы, кружки link
        let mut link_bottom = y_b_last;
        for &(tx, yend, dead, to_end, link, left) in &exits {
            if !to_end || dead {
                continue;
            }
            let key = usize::from(!left);
            let sgn: f64 = if left { -1.0 } else { 1.0 };
            let rail = sgn
                * super::geometry::up(
                    base + self.max_tier as f64 * pitch
                        + self.colw / 2.0
                        + self.st.rail
                        + self.pend_count[key] as f64 * self.st.rail_step,
                    self.st.grid,
                );
            self.pend_count[key] += 1;
            if let Some(letter) = link {
                let ccy = col_bottom + self.st.jog + self.st.vgap + self.st.conn_r;
                self.add("conn", rail, ccy, &letter.to_string());
                self.edge(
                    &[
                        (tx, yend),
                        (tx, col_bottom + self.st.jog),
                        (rail, col_bottom + self.st.jog),
                        (rail, ccy - self.st.conn_r),
                    ],
                    true,
                );
                link_bottom = ccy + self.st.conn_r;
            } else {
                self.pend.push(Pend {
                    x: tx,
                    y: yend,
                    cb: col_bottom,
                    rail,
                });
            }
        }
        // пустой хвост: рельса «нет» на следующей свободной стороне,
        // жмётся к ромбу и чистит колонки только своей стороны; свою
        // горизонталь не рисует — её накрывает шина
        let has_rail = tail.stmts.is_empty();
        if has_rail {
            let left = k % 2 == 0;
            let sgn: f64 = if left { -1.0 } else { 1.0 };
            let clear = exits
                .iter()
                .filter(|e| e.5 == left)
                .map(|e| e.0.abs() + self.nhe + self.st.grid)
                .fold(dw / 2.0 + 2.0 * self.st.grid, f64::max);
            let bx = super::geometry::up(clear, self.st.grid);
            let cy = cy0 + step * (k - 1) as f64;
            self.edge(
                &[(sgn * dw / 2.0, cy), (sgn * bx, cy), (sgn * bx, merge_y)],
                false,
            );
            xs.push(sgn * bx);
            self.labels.push(Label {
                x: sgn * (dw / 2.0 + self.st.label_exit_dx),
                y: cy - self.st.label_dy,
                text: tail.label.clone(),
                ha: "center".into(),
            });
        }
        let lo = xs.iter().cloned().fold(f64::MAX, f64::min);
        let hi = xs.iter().cloned().fold(f64::MIN, f64::max);
        if hi - lo > 1e-9 {
            self.edge(&[(lo, merge_y), (hi, merge_y)], false);
        }
        let n_merge = exits.iter().filter(|e| !e.2 && !e.3).count() + usize::from(has_rail);
        if n_merge == 0 {
            // всё в тупиках: формальное продолжение ствола под ромбами
            self.edge(&[(0.0, y_b_last), (0.0, merge_y)], false);
        }
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { x: 0.0, y: cursor });
        (Some((0.0, merge_y)), cursor)
    }
}
