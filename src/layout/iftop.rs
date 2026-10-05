//! Ромб «если»/переключатель на основной линии: колонки веток,
//! метки да/нет/кейсов, слияние, рельсы «-> конец» (pend) и кружки link.

use super::column::rail_only;
use super::ctx::{ColEnd, Ctx, Pend};
use super::ifnode::{build_plan, cascade_cols, Side};
use super::types::{Anchor, Label};
use crate::ir::{Branch, Node, NodeKind, Stmt};

impl Ctx<'_> {
    /// Сколько пустых веток рисуем отдельными рельсами. Дальше — одной
    /// рельсой с подписями через запятую: каждая рельса сдвинута на
    /// 2 модуля, и десяток пустых кейсов раздвигал схему на 300 pt
    /// вбок — ровно настолько, чтобы вписывание упало вдвое. Такое
    /// бывает после разреза ромба по ветвям: половина кейсов уехала на
    /// следующий лист, и на этом листе остались одни подписи.
    pub(super) const EMPTY_RAIL_MAX: usize = 3;

    /// Рельсы пустых веток: из правой вершины вбок и вниз до `join_y`.
    pub(super) fn empty_rails(
        &mut self,
        axis: f64,
        cy: f64,
        join_y: f64,
        bx: f64,
        labels: &[String],
    ) {
        let (dw, _) = self.sizes["if"];
        let vr = (axis + dw / 2.0, cy);
        let target = axis;
        if labels.len() <= Self::EMPTY_RAIL_MAX {
            for (k, lbl) in labels.iter().enumerate() {
                let x = bx + k as f64 * 2.0 * self.st.grid;
                self.edge(&[vr, (x, cy), (x, join_y), (target, join_y)], false);
                self.labels.push(Label {
                    x: dw / 2.0 + self.st.label_exit_dx + k as f64 * 2.0 * self.st.grid,
                    y: cy - self.st.label_dy,
                    text: lbl.clone(),
                    ha: "center".into(),
                });
            }
            return;
        }
        let x = bx;
        self.edge(&[vr, (x, cy), (x, join_y), (target, join_y)], false);
        let text = labels.join(", ");
        // подписи не лезут за левый край ромба — переносим по ширине
        let room = (x - dw / 2.0).max(3.0 * self.st.grid);
        let per_line = (room / self.st.char_w).floor().max(1.0) as usize;
        let items: Vec<&str> = text.split(", ").collect();
        let chunk = (per_line / 2).max(1);
        let lines: Vec<String> = items.chunks(chunk).map(|c| c.join(", ")).collect();
        for (i, line) in lines.iter().enumerate() {
            self.labels.push(Label {
                x,
                y: cy - self.st.label_dy * (i as f64 + 1.0),
                text: line.clone(),
                ha: "center".into(),
            });
        }
    }

    /// Помещается ли шина диспетча (все кейсы в один ряд) в лист
    /// читаемо, то есть без ухода масштаба листа ниже порога.
    ///
    /// Шина раскладывает кейсы ярусами от оси (`build_plan`): кейс
    /// яруса `t` стоит на расстоянии `base + t * pitch`, а сам ряд
    /// занимает `nhe` в каждую сторону. Порог — не «влезает при
    /// масштабе 1.0», а «влезает не хуже, чем режет порезка»: иначе
    /// три кейса, которым не хватало трёх процентов, уезжали в сетку
    /// и читались хуже, чем на шине.
    pub(super) fn bus_fits(&self, cases: usize) -> bool {
        if cases < 2 {
            return true;
        }
        let dw = self.sizes["if"].0;
        let pitch = 2.0 * self.nhe + self.st.colgap;
        let base = dw / 2.0 + self.st.hgap + self.nhe;
        let half = base + super::ifnode::max_tier(cases) as f64 * pitch + self.nhe;
        2.0 * half <= self.st.sheet.text_w() / self.st.split_scale
    }

    /// Ромб «если»/переключатель на основной линии.
    pub(super) fn decision_top(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
    ) -> (Option<(f64, f64)>, f64) {
        if matches!(cascade_cols(nd), Some((k, _)) if k >= 2) {
            return self.decision_cascade(nd, prev, cursor, 0.0);
        }
        // Диспетч: кейсы либо на одной шине (bus), либо сеткой по два
        // на ряд. Выбор — по ширине, а не по числу кейсов: шина растёт
        // линейно, и при 4 кейсах она оказывалась ШИРЕ, чем сетка при
        // 5 (808 pt против 496 pt), то есть четыре кейса читались хуже
        // пяти, а порог «5+» этого не замечал. Решает лист: если шина
        // не помещается в текстовую зону, кладём кейсы сеткой.
        let n_ne = nd.branches.iter().filter(|b| !b.stmts.is_empty()).count();
        if nd.switch_var.is_some() && n_ne >= 2 && !self.bus_fits(n_ne) {
            return self.switch_rows(nd, prev, cursor, 0.0);
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
        let is_switch = nd.switch_var.is_some();
        let scoped = is_switch || self.loop_depth > 0 || self.switch_depth > 0;
        let saved_direct = self.case_direct;
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
        for &(bi, side, ..) in &plan {
            let e = exits[bi].unwrap();
            // Колонка на оси (tx = 0) стоит на стволе продолжения: её низ —
            // живой, мёртвый или «-> конец» — задаёт merge_y, иначе ствол
            // (0, merge_y) → следующий блок протыкает её содержимое.
            // Внесъёмные колонки (tx != 0) ствол не трогают.
            if side == Side::Axis || (!e.2 && e.3 == ColEnd::Flow) {
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
                !e.2 && e.3 == ColEnd::Flow
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
        // Симметрия: пустая ветка не спускается на всю высоту схемы.
        // Раньше она шла от вершины вниз до merge_y (низ самой глубокой
        // колонки) — на схеме с высокой «да»-веткой это давало
        // вертикаль в сотни пунктов пустоты. Теперь идёт вбок и
        // присоединяется к шине на уровне низа ромба (y_b), а если
        // шины выше нет — тоже коротко. Линии обеих веток выходят из
        // вершин ромба на одной высоте и уходят вниз на одинаковое
        // расстояние, а зеркальность не ломается пустотой.
        self.empty_rails(
            0.0,
            cy,
            y_b,
            super::geometry::up(clear, self.st.grid),
            &empty,
        );
        // Вершина ромба, на которую пришла рельса пустой ветки, обязана
        // уходить вниз в то же место, откуда продолжается ствол. Раньше
        // рельса садилась на вершину, а ствол начинался на merge_y —
        // между ними зазор: 2g при мёртвой колонке (examples/switch_case)
        // и вплоть до самой шины, когда колонка живая. Колонка на оси
        // ведёт вершину вниз сама, ей стык не нужен.
        if !has_axis && (!empty.is_empty() || n_merge == 0) {
            self.edge(&[vb, (0.0, merge_y)], false);
        }
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { y: cursor });
        (Some((0.0, merge_y)), cursor)
    }

    /// Большой переключатель: кейсы сеткой — по два на ряд, ряды друг
    /// под другом. Ширина постоянная (две колонки на ±base), растёт
    /// только вниз — одна длинная шина кейсов ужимала всю страницу.
    /// Шины рядов соединяет ствол на оси; слияние живых колонок —
    /// одна шина под последним рядом.
    pub(super) fn switch_rows(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
        axis: f64,
    ) -> (Option<(f64, f64)>, f64) {
        let (dw, dh) = self.sizes["if"];
        let cy = cursor + self.st.vgap + dh / 2.0;
        self.add("if", axis, cy, &nd.text);
        if let Some(p) = prev {
            self.edge(&[p, (axis, cy - dh / 2.0)], true);
        }
        let base = dw / 2.0 + self.st.hgap + self.nhe;
        let saved_direct = self.case_direct;
        self.switch_depth += 1;
        self.case_direct = true;
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
        self.edge(&[(axis, trunk_from), (axis, y_bus)], false);
        let mut col_bottom = y_bus;
        let mut merge_y = y_bus;
        let mut link_bottom = y_bus;
        let mut merged = false;
        for (ri, row) in cases.chunks(2).enumerate() {
            if ri > 0 {
                // ствол от слияния предыдущего ряда к шине следующего
                y_bus = merge_y + self.st.vgap;
                self.edge(&[(axis, trunk_from), (axis, y_bus)], false);
            }
            let mut live: Vec<f64> = Vec::new();
            let mut gone: Vec<(f64, f64, bool, Option<char>)> = Vec::new(); // x, yend, to_end, link
            for (ci, b) in row.iter().enumerate() {
                let left = ci == 0;
                let sgn: f64 = if left { -1.0 } else { 1.0 };
                let cx0 = axis + sgn * base;
                let top = y_bus + self.st.vgap;
                // шина ряда: от ствола до колонки (два сегмента без
                // наложения — вместе образуют шину через ось)
                self.edge(&[(axis, y_bus), (cx0, y_bus)], false);
                self.edge(&[(cx0, y_bus), (cx0, top)], !rail_only(&b.stmts));
                self.labels.push(Label {
                    x: cx0
                        + if left {
                            -self.st.label_dx
                        } else {
                            self.st.label_dx
                        },
                    y: top - self.st.label_dy,
                    text: b.label.clone(),
                    ha: if left { "right" } else { "left" }.into(),
                });
                let (yend, end) = self.render_column(&b.stmts, cx0, top);
                col_bottom = col_bottom.max(yend);
                if end != ColEnd::Flow {
                    // тупик return или рельса break: слияния от колонки
                    // нет — break дорисует дренаж switch
                    continue;
                }
                if b.to_end {
                    gone.push((cx0, yend, true, b.link));
                } else {
                    live.push(cx0);
                    gone.push((cx0, yend, false, None));
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
                self.edge(&[(lo.min(axis), merge_y), (hi.max(axis), merge_y)], false);
                merged = true;
            }
            trunk_from = merge_y;
            // «-> конец»: спуск до уровня слияния ряда, наружу за сетку,
            // дальше рельсу дорисует layout() над «концом»
            for &(cx0, yend, _, link) in gone.iter().filter(|g| g.2) {
                let sgn: f64 = if cx0 > axis { 1.0 } else { -1.0 };
                let key = usize::from(cx0 > axis);
                let out = axis + sgn * (bx0 + empty.len() as f64 * 2.0 * self.st.grid);
                self.edge(&[(cx0, yend), (cx0, merge_y), (out, merge_y)], false);
                let pitch = 2.0 * self.nhe + self.st.colgap;
                let rail = axis
                    + sgn
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
                    &[
                        (axis + dw / 2.0, cy),
                        (bx, cy),
                        (bx, merge_y),
                        (axis, merge_y),
                    ],
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
            self.edge(&[(axis, trunk_from), (axis, merge_y)], false);
        }
        self.switch_depth -= 1;
        self.case_direct = saved_direct;
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { y: cursor });
        (Some((axis, merge_y)), cursor)
    }

    /// Каскад else-if: вертикальный ствол с одной шиной. Ромбы цепочки
    /// на оси друг под другом («нет» — ребро от нижней вершины к верхней,
    /// метка справа от линии), «да» каждого ромба — колонка сбоку,
    /// стороны чередуют L0, R0, L1, R1..., все колонки на одном top0;
    /// хвост-else — следующая свободная сторона, пустой хвост — рельса.
    /// Слияние: спуски живых колонок и ровно одна горизонтальная шина.
    pub(super) fn decision_cascade(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
        axis: f64,
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
            self.add("if", axis, cy0 + step * i as f64, &dnd.text);
        }
        if let Some(p) = prev {
            self.edge(&[p, (axis, cy0 - dh / 2.0)], true);
        }
        // ствол продолжения: «нет» ведёт к следующему ромбу
        for i in 1..k {
            let y0 = cy0 + step * (i - 1) as f64 + dh / 2.0;
            self.edge(&[(axis, y0), (axis, y0 + self.st.vgap)], true);
            self.labels.push(Label {
                x: axis + self.st.label_axis_dx,
                y: y0 + self.st.vgap - self.st.label_dy,
                text: links[i - 1].0.branches.last().unwrap().label.clone(),
                ha: "left".into(),
            });
        }
        let pitch = 2.0 * self.nhe + self.st.colgap;
        let base = dw / 2.0 + self.st.hgap + self.nhe;
        let y_b_last = cy0 + step * (k - 1) as f64 + dh / 2.0;
        let n_cols = k + usize::from(!tail.stmts.is_empty());
        // Да-колонки — ДВЕ, слева и справа, и укладываются друг под
        // другом. Раньше их раскладывали ярусами наружу
        // (`base + (ci/2)*pitch`), и ширина росла вместе с числом
        // веток: восемь else-if давали 3677 pt, а общий масштаб
        // лабораторной работы из-за одного такого файла падал до 15 %.
        // Смысл каскада в том, что ромбы стоят столбиком по оси, так что
        // места вбок не требуется — ветви идут вниз.
        //
        // Вход в колонку: из боковой вершины ромба по коридору
        // x_v (между кромкой ромбов и колонками) вниз до верха своей
        // колонки и вбок в неё. Коридор не пересекает ни ромбы
        // (они уже, чем x_v), ни колонки (они уже, чем x_v), поэтому
        // вертикаль безопасна на всю высоту каскада.
        let x_v = dw / 2.0 + self.st.grid;
        let mut exits: Vec<(f64, f64, ColEnd, bool, Option<char>, bool)> = Vec::new();
        // ветки каскада — не кейс-колонки: break в них не растворяется
        let saved_direct = self.case_direct;
        self.case_direct = false;
        // Точки крепления коридора по сторонам: сюда попадает и выход
        // из боковой вершины ромба, и вход в колонку — коридор
        // рисуется ровно между крайними из них.
        let mut att: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
        let mut tops: Vec<f64> = Vec::with_capacity(n_cols);
        let mut side_bottom = [f64::NEG_INFINITY, f64::NEG_INFINITY];
        // ветка ci — это links[ci] для «да»-звеньев и хвост для последнего
        let branch = |ci: usize| -> &Branch {
            if ci < k {
                links[ci].1
            } else {
                tail
            }
        };
        for ci in 0..n_cols {
            let side = usize::from(ci % 2 == 1);
            let sgn: f64 = if side == 0 { -1.0 } else { 1.0 };
            let cx0 = axis + sgn * base;
            let cy = cy0 + step * ci.min(k - 1) as f64;
            let ybr = branch(ci);
            // верх колонки: не выше своего ромба и не выше низа
            // предыдущей колонки той же стороны
            let top = (cy + dh / 2.0 + self.st.vgap).max(side_bottom[side] + self.st.vgap);
            self.edge(&[(sgn * dw / 2.0, cy), (sgn * x_v, cy)], false);
            self.edge(
                &[(sgn * x_v, top), (cx0, top)],
                !(rail_only(&ybr.stmts) && self.loop_depth + self.switch_depth > 0),
            );
            att[side].push(cy);
            att[side].push(top);
            self.labels.push(Label {
                x: sgn * (dw / 2.0 + self.st.label_exit_dx),
                y: cy - self.st.label_dy,
                text: ybr.label.clone(),
                ha: "center".into(),
            });
            let (yend, end) = self.render_column(&ybr.stmts, cx0, top);
            side_bottom[side] = yend;
            tops.push(top);
            exits.push((cx0, yend, end, ybr.to_end, ybr.link, sgn < 0.0));
        }
        // коридор стороны — один вертикальный участок ровно между
        // крайними горизонталями, что к нему крепятся: сверху от
        // самой верхней ветви, снизу до входа в самую нижнюю колонку.
        // Раньше верх брался от верхней вершины первого ромба, а низ —
        // от низа колонки, и линия свисала в пустоту с обеих сторон:
        // концы висели над схемой без крепления.
        for (side_idx, sgn) in [-1.0f64, 1.0f64].into_iter().enumerate() {
            let a = &att[side_idx];
            if a.is_empty() {
                continue;
            }
            let cy_lo = a.iter().copied().fold(f64::MAX, f64::min);
            let cy_hi = a.iter().copied().fold(f64::MIN, f64::max);
            if cy_hi - cy_lo > 1e-9 {
                self.edge(&[(sgn * x_v, cy_lo), (sgn * x_v, cy_hi)], false);
            }
        }
        self.case_direct = saved_direct;
        let col_bottom = exits.iter().map(|e| e.1).fold(y_b_last, f64::max);
        let mut merge_y = y_b_last + 2.0 * self.st.grid;
        for &(_, yend, end, to_end, _, _) in &exits {
            if end == ColEnd::Flow && !to_end {
                merge_y = merge_y.max(yend + self.st.mgap);
            }
        }
        // Куда спускаться к шине: до самой шины, но не сквозь блок
        // следующей колонки той же стороны. Раньше колонки стояли на
        // разных абсциссах и спуск был свободен; теперь они уложены
        // одна под другой, и спуск верхней колонки к общей шине
        // проходил прямо через блок нижней.
        let drop_to = |ci: usize, merge: f64| -> f64 {
            let step_side = ci + 2;
            tops.get(step_side)
                .map_or(merge, |&t| (t - self.st.grid).min(merge))
        };
        // единая шина: спуск каждой живой колонки и ровно один
        // горизонтальный сегмент от крайней левой до крайней правой
        let mut xs: Vec<f64> = vec![axis];
        for (ci, &(cx0, yend, end, to_end, _, left)) in exits.iter().enumerate() {
            if end == ColEnd::Flow && !to_end {
                let to = drop_to(ci, merge_y);
                if to > yend {
                    self.edge(&[(cx0, yend), (cx0, to)], false);
                }
                if to < merge_y - 1e-6 {
                    // Спуск упёрся в блок нижней колонки той же стороны
                    // (колонки уложены на одну абсциссу): сплошная линия
                    // кончилась бы в пустоту над этим блоком. В обход —
                    // на чистую полосу снаружи колонок: nhe — полная
                    // полутавина содержимого колонки, плюс сетка зазора.
                    let sgn = if left { -1.0 } else { 1.0 };
                    let lane = axis + sgn * ((cx0 - axis).abs() + self.nhe + self.st.grid);
                    self.edge(&[(cx0, to), (lane, to), (lane, merge_y)], false);
                    xs.push(lane);
                } else {
                    xs.push(cx0);
                }
            }
        }
        // «-> конец»: рельса снаружи колонок всей схемы, кружки link
        let mut link_bottom = y_b_last;
        for (ci, &(cx0, yend, end, to_end, link, left)) in exits.iter().enumerate() {
            if !to_end || end != ColEnd::Flow {
                continue;
            }
            let key = usize::from(!left);
            let sgn: f64 = if left { -1.0 } else { 1.0 };
            let rail = axis
                + sgn
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
                let knee = drop_to(ci, col_bottom + self.st.jog);
                self.edge(
                    &[
                        (cx0, yend),
                        (cx0, knee),
                        (rail, knee),
                        (rail, ccy - self.st.conn_r),
                    ],
                    true,
                );
                link_bottom = ccy + self.st.conn_r;
            } else {
                self.pend.push(Pend {
                    x: cx0,
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
            let left = k.is_multiple_of(2);
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
        let n_merge =
            exits.iter().filter(|e| e.2 == ColEnd::Flow && !e.3).count() + usize::from(has_rail);
        if n_merge == 0 {
            // всё в тупиках: формальное продолжение ствола под ромбами
            self.edge(&[(axis, y_b_last), (axis, merge_y)], false);
        }
        let cursor = merge_y.max(col_bottom).max(link_bottom);
        self.anchors.push(Anchor { y: cursor });
        (Some((axis, merge_y)), cursor)
    }
}
