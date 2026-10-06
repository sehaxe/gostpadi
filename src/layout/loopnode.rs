use super::ctx::{BreakAt, ColEnd, Ctx};
use super::geometry::up;
use super::types::Anchor;
use crate::ir::Node;

impl Ctx<'_> {
    /// Цикл на основной линии: entry-стрелка сверху, две трапеции
    /// (loop_begin сверху, loop_end снизу), тело между ними.
    /// Порт гост-паттерна 3.4: обратная связь не рисуется — совпадение
    /// номеров на трапециях связывает begin/end неявно.
    pub(super) fn loop_top(
        &mut self,
        nd: &Node,
        prev: Option<(f64, f64)>,
        cursor: f64,
    ) -> (Option<(f64, f64)>, f64) {
        if let Some(p) = prev {
            self.edge(&[p, (0.0, cursor + self.st.vgap)], true);
        }
        let (merge, _) = self.sub_loop(nd, 0.0, cursor + self.st.vgap);
        self.anchors.push(Anchor { x: 0.0, y: merge });
        (Some((0.0, merge)), merge)
    }

    /// Цикл внутри колонки. Возвращает (y продолжения, конец): из двух
    /// трапеций поток всегда продолжается вниз (ColEnd::Flow), разве что
    /// тело целиком в тупике return.
    pub(super) fn sub_loop(&mut self, nd: &Node, tx: f64, top: f64) -> (f64, ColEnd) {
        let (lw, lh) = self.sizes["loop"];
        let cy1 = top + lh / 2.0;
        self.add("loop_begin", tx, cy1, &nd.loop_label());
        self.loop_depth += 1;
        let num = self.loop_depth;
        // коридор: шире половины трапеции и любого вложенного содержимого,
        // чтобы рельсы break/continue не задевали фигуры
        let chan = up(self.nhe.max(lw / 2.0) + 2.0 * self.st.grid, self.st.grid);
        let top0 = cy1 + lh / 2.0 + self.st.vgap;
        let mark = self.breaks.len();
        let mark_c = self.continues.len();
        let saved_direct = self.case_direct;
        self.case_direct = false;
        let (yend, end) = match &nd.body {
            Some(body) => {
                self.edge(&[(tx, cy1 + lh / 2.0), (tx, top0)], true);
                self.render_column(body, tx, top0)
            }
            None => (cy1 + lh / 2.0, ColEnd::Flow),
        };
        self.case_direct = saved_direct;
        // нижняя трапеция: вход сверху (из тела), штатный выход вниз
        let cy2 = yend + self.st.vgap + lh / 2.0;
        self.add("loop_end", tx, cy2, &num.to_string());
        if end != ColEnd::Return {
            self.edge(&[(tx, yend), (tx, cy2 - lh / 2.0)], true);
        }
        let merge = cy2 + lh / 2.0 + self.st.mgap;
        self.edge(&[(tx, cy2 + lh / 2.0), (tx, merge)], false);
        // Рельсы break: вниз мимо loop_end, T-стык на продолжении.
        //
        // Колонка плитки (b.tx) уже своя ось: если она не накрыта трапецией
        // слияния, рельса падает из центра плитки прямо на merge и вправо
        // на ось — один изгиб. Коридор тогда не нужен, и лишней ступеньки
        // нет. Трапеция шириной lw по центру tx, так что достаточно
        // выйти за её половину.
        //
        // Плитка на месте — отъезд вниз на в-gap нужен ещё и по виду:
        // горизонталь с высоты низа плитки сливается с её границей и
        // читается как уход из угла, а не из центра. Колонка без плиток
        // (только break/continue) выходит сразу влево: выходить неоткуда,
        // и вертикаль на её оси запрещена.
        let breaks: Vec<BreakAt> = self.breaks.drain(mark..).collect();
        for b in &breaks {
            let slot = self.break_slot;
            self.break_slot += 1;
            let clear = b.tx < tx - lw / 2.0 || b.tx > tx + lw / 2.0;
            if b.from_tile && clear {
                self.edge(&[(b.tx, b.y), (b.tx, merge), (tx, merge)], false);
                continue;
            }
            let rx = tx - chan - slot as f64 * 2.0 * self.st.grid;
            let mut pts = vec![(b.tx, b.y)];
            if b.from_tile {
                let by = b.y + self.st.vgap;
                pts.push((b.tx, by));
                pts.push((rx, by));
            } else {
                pts.push((rx, b.y));
            }
            pts.push((rx, merge));
            pts.push((tx, merge));
            self.edge(&pts, false);
        }
        // рельсы continue: тот же коридор, T-стык на выходе loop_end —
        // следующая итерация
        let conts: Vec<BreakAt> = self.continues.drain(mark_c..).collect();
        if !conts.is_empty() {
            let yj = (cy2 + lh / 2.0 + merge) * 0.5;
            for c in &conts {
                let slot = self.break_slot;
                self.break_slot += 1;
                let rx = tx - chan - slot as f64 * 2.0 * self.st.grid;
                self.edge(&[(c.tx, c.y), (rx, c.y), (rx, yj), (tx, yj)], false);
            }
        }
        self.loop_depth -= 1;
        (merge, ColEnd::Flow)
    }
}
