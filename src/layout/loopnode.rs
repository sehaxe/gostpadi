use super::ctx::{BreakAt, Ctx};
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

    /// Цикл внутри колонки. Возвращает (y продолжения, тупик=false):
    /// из двух трапеций поток всегда продолжается вниз.
    pub(super) fn sub_loop(&mut self, nd: &Node, tx: f64, top: f64) -> (f64, bool) {
        let (lw, lh) = self.sizes["loop"];
        let cy1 = top + lh / 2.0;
        self.add("loop_begin", tx, cy1, &nd.text);
        self.loop_depth += 1;
        let num = self.loop_depth;
        // коридор: шире половины трапеции и любого вложенного содержимого,
        // чтобы рельсы break/возврат не задевали фигуры (исправление 2)
        let chan = up(self.nhe.max(lw / 2.0) + 2.0 * self.st.grid, self.st.grid);
        let top0 = cy1 + lh / 2.0 + self.st.vgap;
        let mark = self.breaks.len();
        let (yend, dead) = match &nd.body {
            Some(body) => {
                self.edge(&[(tx, cy1 + lh / 2.0), (tx, top0)], true);
                self.render_column(body, tx, top0)
            }
            None => (cy1 + lh / 2.0, false),
        };
        // нижняя трапеция: вход сверху (из тела), штатный выход вниз
        let cy2 = yend + self.st.vgap + lh / 2.0;
        self.add("loop_end", tx, cy2, &num.to_string());
        if !dead {
            self.edge(&[(tx, yend), (tx, cy2 - lh / 2.0)], true);
        }
        let merge = cy2 + lh / 2.0 + self.st.mgap;
        self.edge(&[(tx, cy2 + lh / 2.0), (tx, merge)], false);
        // рельсы break: влево, вниз мимо loop_end, T-стык на продолжении
        let breaks: Vec<BreakAt> = self.breaks.drain(mark..).collect();
        for b in &breaks {
            let slot = self.break_slot;
            self.break_slot += 1;
            let rx = tx - chan - slot as f64 * 2.0 * self.st.grid;
            self.edge(&[(b.tx, b.y), (rx, b.y), (rx, merge), (tx, merge)], false);
        }
        self.loop_depth -= 1;
        (merge, false)
    }
}
