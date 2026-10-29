//! Ромб «если»/переключатель на основной линии: колонки веток,
//! метки да/нет/кейсов, слияние, рельсы «-> конец» (pend) и кружки link.

use super::column::rail_only;
use super::ctx::{BreakAt, ColEnd, Ctx, Pend};
use super::ifnode::{cascade_cols, pack_plan, single_plan, Side};
use super::types::{Anchor, Label};
use crate::ir::{Branch, Node, NodeKind, Stmt};

impl Ctx<'_> {
    /// Полоса рельсы «-> конец»: снаружи всех колонок схемы. env — край
    /// своих колонок; запас по ярусам (max_tier) и общий nhe страхуют от
    /// чужих фигур ниже, которые рельса проходит насквозь.
    fn rail_lane(&self, env: f64) -> f64 {
        let pitch = 2.0 * self.nhe + self.st.colgap;
        let lane =
            self.nhe + self.st.hgap + self.colw / 2.0 + self.st.rail + self.max_tier as f64 * pitch;
        env.max(lane)
    }

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
        // большой переключатель: кейсы сеткой, по два на ряд. Три кейса
        // тоже сеткой (2 + 1): веер ставит средний кейс на ось, и его
        // спуск сливается со стволом в узле с четырьмя сторонами.
        let n_ne = nd.branches.iter().filter(|b| !b.stmts.is_empty()).count();
        if nd.switch_var.is_some() && (n_ne == 3 || n_ne >= 5) {
            return self.switch_rows(nd, prev, cursor);
        }
        let (dw, dh) = self.sizes["if"];
        let cy = cursor + self.vg() + dh / 2.0;
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
        // пустая ветка простого if: не ствол, а короткое колено в свою
        // сторону. Индекс нужен, чтобы знать сторону (0 — «да», 1 — «нет»).
        let empty_idx: Vec<(usize, String)> = nd
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.stmts.is_empty())
            .map(|(i, b)| (i, b.label.clone()))
            .collect();
        let n = idxs.len();
        let is_switch = nd.switch_var.is_some();
        let comb = is_switch && n >= 2;
        let top0 = cy + dh / 2.0 + self.vg();
        let y_b = cy + dh / 2.0;
        let ext: Vec<(f64, f64)> = idxs
            .iter()
            .map(|&i| self.extent_lr(&nd.branches[i].stmts))
            .collect();
        let hinge = dw / 2.0 + self.st.hgap;
        let scoped = is_switch || self.loop_depth > 0 || self.switch_depth > 0;
        // ветка из одних break/continue ВНУТРИ scope: колонки нет, рельса
        // идёт от боковой вершины ромба. Вне scope break рисуется плиткой —
        // такая колонка обязана быть нормальной ширины.
        let narrow: Vec<bool> = if scoped {
            idxs.iter()
                .map(|&i| rail_only(&nd.branches[i].stmts))
                .collect()
        } else {
            Vec::new()
        };
        // switch читается слева направо, у ромба «да» — слева
        let (plan, envl, envr) = if n == 1 && !empty.is_empty() {
            single_plan(
                idxs[0],
                ext[0],
                narrow.first().copied().unwrap_or(false),
                hinge,
                dw / 2.0,
                0.0,
            )
        } else {
            pack_plan(
                &idxs,
                &ext,
                &narrow,
                hinge,
                dw / 2.0,
                self.st.colgap,
                0.0,
                false,
            )
        };
        let env = envl.max(envr);
        if comb {
            self.comb_line(&plan, y_b);
        }
        let saved_direct = self.case_direct;
        let br_mark = self.breaks.len();
        if is_switch {
            self.switch_depth += 1;
            self.case_direct = true;
        } else {
            self.case_direct = false;
        }
        let mut exits: Vec<Option<(f64, f64, bool, ColEnd)>> = vec![None; nd.branches.len()];
        for &(bi, side, t2, txx) in &plan {
            let arrow = !(scoped && rail_only(&nd.branches[bi].stmts));
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
                arrow,
            );
            let (yend, end) = self.render_column(&nd.branches[bi].stmts, txx, top0);
            exits[bi] = Some((
                txx,
                yend,
                nd.branches[bi].to_end && end != ColEnd::Return,
                end,
            ));
        }
        if is_switch {
            self.switch_depth -= 1;
        }
        self.case_direct = saved_direct;
        let mut merge_y = y_b;
        let mut axis_tail: Option<f64> = None;
        for &(bi, side, ..) in &plan {
            let e = exits[bi].unwrap();
            if side == Side::Axis {
                axis_tail = Some(axis_tail.map_or(e.1, |v: f64| v.max(e.1)));
            }
            // Колонка на оси (tx = 0) стоит на стволе продолжения: её низ —
            // живой, мёртвый или «-> конец» — задаёт merge_y, иначе ствол
            // (0, merge_y) → следующий блок протыкает её содержимое.
            // Внесъёмные колонки (tx != 0) ствол не трогают.
            if side == Side::Axis || (!e.2 && e.3 == ColEnd::Flow) {
                merge_y = merge_y.max(e.1 + self.st.mgap);
            }
        }
        // Пустая ветка простого if — не ствол: короткое колено в свою
        // сторону (0 — «да», 1 — «нет») и возврат в шину слияния.
        // Пустой кейс switch — отдельная ветвь, он уходит рельсой справа.
        let stub: Vec<(usize, String)> = if is_switch {
            Vec::new()
        } else {
            empty_idx.clone()
        };
        let stubs: Vec<(f64, f64, String)> = stub
            .iter()
            .map(|(i, lbl)| {
                let sgn = if *i == 0 { -1.0 } else { 1.0 };
                (sgn * (dw / 2.0 + self.st.hgap), sgn, lbl.clone())
            })
            .collect();
        let rails: Vec<String> = if is_switch { empty.clone() } else { Vec::new() };
        if !stubs.is_empty() {
            merge_y = merge_y.max(y_b + 2.0 * self.st.grid);
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
                if e.3 == ColEnd::Flow {
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
                    self.rail_lane(env) + self.pend_count[key] as f64 * self.st.rail_step,
                    self.st.grid,
                );
            self.pend_count[key] += 1;
            if let Some(letter) = nd.branches[bi].link {
                // «конец» на другом листе: кружок-соединитель у ветки
                let ccy = col_bottom + self.st.jog + self.vg() + self.st.conn_r;
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
        // колено пустой ветки: боковая вершина -> в сторону -> вниз в
        // шину слияния. Стрелки на колене нет: входить в него некуда,
        // поток на шине ведёт следующий блок.
        for (sx, sgn, lbl) in &stubs {
            self.edge(&[(*sgn * dw / 2.0, cy), (*sx, cy), (*sx, merge_y)], false);
            self.labels.push(Label {
                x: *sgn * (dw / 2.0 + self.st.grid),
                y: cy - self.st.label_dy,
                text: lbl.clone(),
                ha: "center".into(),
            });
            merge_cols.push((*sx, merge_y));
        }
        // шина слияния: один горизонтальный отрезок вместо наложенных
        // хвостов колонок
        self.merge_bus(&merge_cols, 0.0, merge_y);
        let n_merge = plan
            .iter()
            .filter(|&&(bi, ..)| {
                let e = exits[bi].unwrap();
                !e.2 && e.3 == ColEnd::Flow
            })
            .count()
            + rails.len()
            + stubs.len();
        let has_axis = plan.iter().any(|p| p.1 == Side::Axis);
        // Рельса пустого кейса жмётся к ромбу: пол — правая вершина + 2g,
        // дальше вправо — только чтобы очистить правые колонки. Старая
        // «зеркальность самой широкой стороне» при пустом правом боку
        // давала крюк через всю схему: шина росла без контента.
        let clear = (env + self.st.grid).max(dw / 2.0 + 2.0 * self.st.grid);
        for (k, lbl) in rails.iter().enumerate() {
            merge_y = merge_y.max(y_b + 2.0 * self.st.grid);
            let bx = super::geometry::up(clear, self.st.grid) + k as f64 * 2.0 * self.st.grid;
            self.edge(&[vr, (bx, cy), (bx, merge_y), (0.0, merge_y)], false);
            self.labels.push(Label {
                x: dw / 2.0 + self.st.label_exit_dx + k as f64 * 2.0 * self.st.grid,
                y: cy - self.st.label_dy,
                text: lbl.clone(),
                ha: "center".into(),
            });
        }
        if n_merge == 0 && !has_axis {
            // все ветки мертвы (return/рельсы «-> конец»): живого
            // продолжения нет, формальный стык к недостижимому блоку.
            self.edge(&[vb, (0.0, merge_y)], false);
        }
        // рельсы break из веток switch (if внутри case): наружу за
        // колонки, вниз к выходу switch, T-стык на стволе
        if is_switch {
            let nested: Vec<BreakAt> = self.breaks.drain(br_mark..).collect();
            let outer = env.max(dw / 2.0 + 2.0 * self.st.grid) + self.st.grid;
            for b in &nested {
                let slot = self.break_slot;
                self.break_slot += 1;
                let sgn = if b.tx <= 0.0 { -1.0 } else { 1.0 };
                let rx = sgn
                    * super::geometry::up(outer + slot as f64 * 2.0 * self.st.grid, self.st.grid);
                self.edge(
                    &[(b.tx, b.y), (rx, b.y), (rx, merge_y), (0.0, merge_y)],
                    false,
                );
            }
        }
        // тупиковый кейс на оси не даёт шины, но продолжение не может
        // начаться внутри его плитки — точка стыка не выше её низа
        let cont_y = merge_y.max(axis_tail.unwrap_or(f64::MIN));
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { x: 0.0, y: cursor });
        (Some((0.0, cont_y)), cursor)
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
        let cy = cursor + self.vg() + dh / 2.0;
        self.add("if", 0.0, cy, &nd.text);
        if let Some(p) = prev {
            self.edge(&[p, (0.0, cy - dh / 2.0)], true);
        }
        let saved_direct = self.case_direct;
        let br_mark = self.breaks.len();
        self.switch_depth += 1;
        self.case_direct = true;
        let empty: Vec<String> = nd
            .branches
            .iter()
            .filter(|b| b.stmts.is_empty())
            .map(|b| b.label.clone())
            .collect();
        let cases: Vec<&Branch> = nd.branches.iter().filter(|b| !b.stmts.is_empty()).collect();
        // Абсциссы кейсов — по их собственному содержимому: колонка встаёт
        // вплотную к ромбу, шина ряда не тянется через пол-листа (на
        // strany.c общий nhe давал шину в 900pt на плитку в 150).
        // Нечётный хвост — на оси: отдельная колонка ему не нужна.
        let hinge = dw / 2.0 + self.st.hgap;
        let ext: Vec<(f64, f64)> = cases.iter().map(|b| self.extent_lr(&b.stmts)).collect();
        let mut xs = vec![0.0f64; cases.len()];
        for (ri, row) in cases.chunks(2).enumerate() {
            if row.len() == 1 {
                xs[ri * 2] = 0.0;
            } else {
                xs[ri * 2] = -(hinge + ext[ri * 2].1);
                xs[ri * 2 + 1] = hinge + ext[ri * 2 + 1].0;
            }
        }
        let env = cases
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let (l, r) = ext[i];
                if xs[i] >= 0.0 {
                    xs[i] + r
                } else {
                    -xs[i] + l
                }
            })
            .fold(dw / 2.0, f64::max);
        // коридоры за сеткой: сначала рельсы пустых кейсов, потом
        // «-> конец» — чтобы вертикали не накладывались
        let bx0 = super::geometry::up(
            (env + self.st.grid).max(dw / 2.0 + 2.0 * self.st.grid),
            self.st.grid,
        );
        // шина первого ряда: от нижней вершины ромба по стволу
        let mut y_bus = cy + dh / 2.0 + self.vg();
        let mut trunk_from = cy + dh / 2.0;
        self.edge(&[(0.0, trunk_from), (0.0, y_bus)], false);
        let mut col_bottom = y_bus;
        let mut merge_y = y_bus;
        let mut link_bottom = y_bus;
        let mut merged = false;
        // последний (нечётный) кейс стоит на оси: его низ — точка продолжения
        let mut axis_tail: Option<f64> = None;
        for (ri, row) in cases.chunks(2).enumerate() {
            if ri > 0 {
                // Шина ряда питается от оси switch, а не от слияния прошлого
                // ряда: слив и вход следующего ряда — разные высоты, иначе
                // выходы кейсов ряда 1 читаются как вход кейсов ряда 2.
                y_bus = merge_y + self.vg();
                self.edge(&[(0.0, merge_y), (0.0, y_bus)], false);
                trunk_from = y_bus;
            }
            let mut live: Vec<f64> = Vec::new();
            let mut gone: Vec<(f64, f64, bool, Option<char>)> = Vec::new(); // x, yend, to_end, link
            let mut axis_bottom = y_bus;
            axis_tail = None;
            for (ci, b) in row.iter().enumerate() {
                let left = ci == 0 && row.len() > 1;
                let tx = xs[ri * 2 + ci];
                let top = y_bus + self.vg();
                // шина ряда: от ствола до колонки (два сегмента без
                // наложения — вместе образуют шину через ось); кейс на
                // оси входа не требует — его продолжает сам ствол
                if tx.abs() > 1e-9 {
                    self.edge(&[(0.0, y_bus), (tx, y_bus)], false);
                }
                self.edge(&[(tx, y_bus), (tx, top)], !rail_only(&b.stmts));
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
                let (yend, end) = self.render_column(&b.stmts, tx, top);
                col_bottom = col_bottom.max(yend);
                if tx.abs() < 1e-9 {
                    axis_bottom = axis_bottom.max(yend);
                    axis_tail = Some(yend);
                }
                if end != ColEnd::Flow {
                    // тупик return или рельса break: слияния от колонки
                    // нет — break дорисует дренаж switch
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
            // Ствол между рядами: связь ромба с шиной следующего ряда.
            // У нечётного хвоста на оси стоит его колонка — там ствол не
            // рисуется. Если шина слияния проходит мимо оси, ствол идёт
            // на шаг вбок: вертикаль сквозь точку шины давала узел с
            // четырьмя сторонами, который читается как пересечение.
            if row.len() > 1 {
                let empty_live = live.is_empty();
                let (lo, hi) = if empty_live {
                    (0.0, 0.0)
                } else {
                    (
                        live.iter().cloned().fold(f64::MAX, f64::min),
                        live.iter().cloned().fold(f64::MIN, f64::max),
                    )
                };
                let off = if hi > self.st.grid {
                    self.st.grid
                } else if lo < -self.st.grid {
                    -self.st.grid
                } else {
                    self.st.grid
                };
                self.edge(&[(off, y_bus), (off, merge_y)], false);
                if empty_live {
                    self.edge(&[(off, merge_y), (0.0, merge_y)], false);
                }
            }

            // «-> конец»: спуск до уровня слияния ряда, наружу за сетку,
            // дальше рельсу дорисует layout() над «концом»
            for &(tx, yend, _, link) in gone.iter().filter(|g| g.2) {
                let sgn: f64 = if tx > 0.0 { 1.0 } else { -1.0 };
                let key = usize::from(tx > 0.0);
                let out = sgn * (bx0 + empty.len() as f64 * 2.0 * self.st.grid);
                self.edge(&[(tx, yend), (tx, merge_y), (out, merge_y)], false);
                let rail = sgn
                    * super::geometry::up(
                        self.rail_lane(env) + self.pend_count[key] as f64 * self.st.rail_step,
                        self.st.grid,
                    );
                self.pend_count[key] += 1;
                if let Some(letter) = link {
                    let ccy = col_bottom + self.st.jog + self.vg() + self.st.conn_r;
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
        if !merged && axis_tail.is_none() {
            // все живые кейсы в тупиках/«-> конец»: формальное
            // продолжение ствола до точки выхода
            self.edge(&[(0.0, trunk_from), (0.0, merge_y)], false);
        }
        // рельсы break из веток внутри кейсов (if внутри case): наружу
        // за сетку, вниз к выходу switch, T-стык на стволе
        let nested: Vec<BreakAt> = self.breaks.drain(br_mark..).collect();
        self.switch_depth -= 1;
        self.case_direct = saved_direct;
        let outer = env + self.st.grid;
        for b in &nested {
            let slot = self.break_slot;
            self.break_slot += 1;
            let sgn = if b.tx <= 0.0 { -1.0 } else { 1.0 };
            let rx =
                sgn * super::geometry::up(outer + slot as f64 * 2.0 * self.st.grid, self.st.grid);
            self.edge(
                &[(b.tx, b.y), (rx, b.y), (rx, merge_y), (0.0, merge_y)],
                false,
            );
        }
        // тупиковый кейс на оси не даёт шины, но продолжение не может
        // начаться внутри его плитки — точка стыка не выше её низа
        let cont_y = merge_y.max(axis_tail.unwrap_or(f64::MIN));
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { x: 0.0, y: cursor });
        (Some((0.0, cont_y)), cursor)
    }

    /// Каскад else-if: ромбы цепочки на оси друг под другом. У каждого
    /// ромба «да» — колонка СЛЕВА, «нет» — колено вправо: к следующему
    /// ромбу (горизонталь, спуск, стрелка в верхнюю вершину) или к
    /// хвосту-колонке справа. Вертикали из нижней вершины ромба нет.
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
        let step = dh + self.vg();
        let cy0 = cursor + self.vg() + dh / 2.0;
        for (i, (dnd, _)) in links.iter().enumerate() {
            self.add("if", 0.0, cy0 + step * i as f64, &dnd.text);
        }
        if let Some(p) = prev {
            self.edge(&[p, (0.0, cy0 - dh / 2.0)], true);
        }
        let top0 = cy0 + dh / 2.0 + self.vg();
        // Абсциссы «да»-колонок — по содержимому своих ветвей (как в
        // остальных ромбах): общий nhe тянул шину через пол-листа.
        // Колонки слева: ближняя кромка — правый вынос r, поэтому ось
        // считается от r и упаковка идёт влево.
        let hinge = dw / 2.0 + self.st.hgap;
        let exts: Vec<(f64, f64)> = links
            .iter()
            .map(|(_, b)| self.extent_lr(&b.stmts))
            .collect();
        let mut xs = vec![0.0f64; k];
        let mut edge = hinge;
        for (ci, &(l, r)) in exts.iter().enumerate() {
            xs[ci] = -(edge + r);
            edge = -xs[ci] + l + self.st.colgap;
        }
        let env = exts
            .iter()
            .enumerate()
            .map(|(ci, &(l, _))| -xs[ci] + l)
            .fold(hinge, f64::max);
        // «да»-гребёнка: общий вертикальный участок в коридоре слева от
        // ромбов, у каждой ветви своя высота обхода над колонками.
        let x_v = -(dw / 2.0 + self.st.grid);
        let y_over = |ci: usize| top0 - (ci + 1) as f64 * self.st.grid;
        let y_top = (0..k)
            .map(|ci| (cy0 + step * ci as f64).min(y_over(ci)))
            .fold(f64::MAX, f64::min);
        let y_bot = (0..k)
            .map(|ci| (cy0 + step * ci as f64).max(y_over(ci)))
            .fold(f64::MIN, f64::max);
        self.edge(&[(x_v, y_top), (x_v, y_bot)], false);
        let saved_direct = self.case_direct;
        self.case_direct = false;
        let mut exits: Vec<(f64, f64, ColEnd, bool, Option<char>, bool)> = Vec::new();
        for ci in 0..k {
            let (_, b) = links[ci];
            let cy = cy0 + step * ci as f64;
            let tx = xs[ci];
            self.edge(&[(-dw / 2.0, cy), (x_v, cy)], false);
            self.edge(&[(x_v, y_over(ci)), (tx, y_over(ci))], false);
            self.edge(
                &[(tx, y_over(ci)), (tx, top0)],
                !(rail_only(&b.stmts) && self.loop_depth + self.switch_depth > 0),
            );
            self.labels.push(Label {
                x: -(dw / 2.0 + self.st.label_exit_dx),
                y: cy - self.st.label_dy,
                text: b.label.clone(),
                ha: "center".into(),
            });
            let (yend, end) = self.render_column(&b.stmts, tx, top0);
            exits.push((tx, yend, end, b.to_end, b.link, true));
        }
        self.case_direct = saved_direct;
        // «нет» звеньев: своя полоса у каждого звена (правее на шаг),
        // конец колена выше ромба-приёмника — вертикалей из ромбов нет.
        for i in 0..k.saturating_sub(1) {
            let y = cy0 + step * i as f64;
            let lane = dw / 2.0 + (i + 1) as f64 * self.st.grid;
            let y_top_next = cy0 + step * (i + 1) as f64 - dh / 2.0;
            let y_entry = y_top_next - self.st.grid;
            self.edge(
                &[
                    (dw / 2.0, y),
                    (lane, y),
                    (lane, y_entry),
                    (0.0, y_entry),
                    (0.0, y_top_next),
                ],
                true,
            );
            self.labels.push(Label {
                x: dw / 2.0 + (i as f64 + 0.5) * self.st.grid,
                y: y - self.st.label_dy,
                text: links[i].0.branches.last().unwrap().label.clone(),
                ha: "center".into(),
            });
        }
        // хвост-«нет» последнего ромба: непустой — колонка справа (верх
        // общий с да-колонками), пустой — рельса к шине слияния.
        let y_last = cy0 + step * (k - 1) as f64;
        let y_b_last = y_last + dh / 2.0;
        let tail_ext = if tail.stmts.is_empty() {
            (0.0, 0.0)
        } else {
            self.extent_lr(&tail.stmts)
        };
        let mut bus_xs: Vec<f64> = vec![0.0];
        let mut has_rail = false;
        if !tail.stmts.is_empty() {
            // Хвост стоит правее всех «нет»-колен: левый край его колонки —
            // за последней полосой, иначе вертикаль колена шла бы сквозь
            // верхнюю плитку хвоста.
            let right = dw / 2.0 + k as f64 * self.st.grid;
            let tail_x = right + self.st.grid + tail_ext.0;
            let y_over_tail = top0 - self.st.grid;
            self.edge(
                &[
                    (dw / 2.0, y_last),
                    (right, y_last),
                    (right, y_over_tail),
                    (tail_x, y_over_tail),
                    (tail_x, top0),
                ],
                true,
            );
            self.labels.push(Label {
                x: (dw / 2.0 + right) / 2.0,
                y: y_last - self.st.label_dy,
                text: tail.label.clone(),
                ha: "center".into(),
            });
            let (yend, end) = self.render_column(&tail.stmts, tail_x, top0);
            exits.push((tail_x, yend, end, tail.to_end, tail.link, false));
        }
        let col_bottom = exits.iter().map(|e| e.1).fold(y_b_last, f64::max);
        // в каскаде осевой колонки нет: хвост-«нет» стоит справа
        let axis_tail: Option<f64> = None;
        let mut merge_y = y_b_last + 2.0 * self.st.grid;
        for &(_, yend, end, to_end, _, _) in &exits {
            if end == ColEnd::Flow && !to_end {
                merge_y = merge_y.max(yend + self.st.mgap);
            }
        }
        // единая шина: спуск каждой живой колонки и ровно один
        // горизонтальный сегмент от крайней левой до крайней правой
        for &(tx, yend, end, to_end, _, _) in &exits {
            if end == ColEnd::Flow && !to_end {
                self.edge(&[(tx, yend), (tx, merge_y)], false);
                bus_xs.push(tx);
            }
        }
        // «-> конец»: рельса снаружи колонок всей схемы, кружки link
        let mut link_bottom = y_b_last;
        for &(tx, yend, end, to_end, link, left) in &exits {
            if !to_end || end != ColEnd::Flow {
                continue;
            }
            let key = usize::from(!left);
            let sgn: f64 = if left { -1.0 } else { 1.0 };
            let rail = sgn
                * super::geometry::up(
                    self.rail_lane(env) + self.pend_count[key] as f64 * self.st.rail_step,
                    self.st.grid,
                );
            self.pend_count[key] += 1;
            if let Some(letter) = link {
                let ccy = col_bottom + self.st.jog + self.vg() + self.st.conn_r;
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
        // пустой хвост: рельса «нет» на правой стороне, жмётся к ромбу
        if tail.stmts.is_empty() {
            has_rail = true;
            let bx = super::geometry::up(dw / 2.0 + 2.0 * self.st.grid, self.st.grid);
            // Ветка каскада может вынести вложенный switch правее оси
            // ромба: фиксированный отступ от ромба садился на кромку её
            // плитки. Рельса уходит за фактические фигуры с зазором.
            let bx = super::geometry::clear_rail_x(
                &self.shapes,
                bx,
                y_last,
                merge_y,
                true,
                self.st.grid,
            );
            self.edge(&[(dw / 2.0, y_last), (bx, y_last), (bx, merge_y)], false);
            bus_xs.push(bx);
            self.labels.push(Label {
                x: dw / 2.0 + self.st.grid,
                y: y_last - self.st.label_dy,
                text: tail.label.clone(),
                ha: "center".into(),
            });
        }
        let lo = bus_xs.iter().cloned().fold(f64::MAX, f64::min);
        let hi = bus_xs.iter().cloned().fold(f64::MIN, f64::max);
        if hi - lo > 1e-9 {
            self.edge(&[(lo, merge_y), (hi, merge_y)], false);
        }
        let n_merge =
            exits.iter().filter(|e| e.2 == ColEnd::Flow && !e.3).count() + usize::from(has_rail);
        if n_merge == 0 {
            // всё мертво: формальный стык к недостижимому продолжению
            self.edge(&[(0.0, y_b_last), (0.0, merge_y)], false);
        }
        // тупиковый кейс на оси не даёт шины, но продолжение не может
        // начаться внутри его плитки — точка стыка не выше её низа
        let cont_y = merge_y.max(axis_tail.unwrap_or(f64::MIN));
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { x: 0.0, y: cursor });
        (Some((0.0, cont_y)), cursor)
    }
}
