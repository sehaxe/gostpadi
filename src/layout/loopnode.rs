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
        self.anchors.push(Anchor { y: merge });
        (Some((0.0, merge)), merge)
    }

    /// Цикл внутри колонки. Возвращает (y продолжения, конец): из двух
    /// трапеций поток всегда продолжается вниз (ColEnd::Flow), разве что
    /// тело целиком в тупике return.
    pub(super) fn sub_loop(&mut self, nd: &Node, tx: f64, top: f64) -> (f64, ColEnd) {
        let (lw, lh) = self.sizes["loop"];
        let cy1 = top + lh / 2.0;
        // Цикл, разорванный по листам: на продолжении верхняя
        // трапеция уже нарисована на прошлом листе, а при уходе тела
        // дальше нижняя не рисуется вовсе — её заменяет кружок.
        // Иначе на листе с разрывом цикл выглядел бы замкнутым, и
        // читатель не видел бы, что он не кончился.
        if !nd.cont {
            self.add("loop_begin", tx, cy1, &nd.loop_label());
        }
        self.loop_depth += 1;
        let num = self.loop_depth;
        // коридор: шире половины трапеции и любого вложенного содержимого,
        // чтобы рельсы break/continue не задевали фигуры
        let chan = up(self.nhe.max(lw / 2.0) + 2.0 * self.st.grid, self.st.grid);
        let top0 = if nd.cont {
            top
        } else {
            cy1 + lh / 2.0 + self.st.vgap
        };
        let mark_c = self.continues.len();
        let saved_direct = self.case_direct;
        self.case_direct = false;
        let (yend, end) = match &nd.body {
            Some(body) if !body.is_empty() => {
                if !nd.cont {
                    self.edge(&[(tx, cy1 + lh / 2.0), (tx, top0)], true);
                }
                self.render_column(body, tx, top0)
            }
            // Пустое тело (`while (getchar() != '\n');`): рисовать нечего,
            // и стрелка «в тело» встала бы в середине прямой линии —
            // вторая стрелка у нижней трапеции осталась бы ниже. Поток
            // идёт из верхней трапеции прямо в нижнюю, одной стрелкой.
            _ => (if nd.cont { top0 } else { cy1 + lh / 2.0 }, ColEnd::Flow),
        };
        self.case_direct = saved_direct;
        if nd.cont_out {
            // тело продолжится на следующем листе: нижней трапеции
            // нет, поток просто идёт в кружок-соединитель
            self.loop_depth -= 1;
            return (yend + self.st.vgap, ColEnd::Flow);
        }
        // нижняя трапеция: вход сверху (из тела), штатный выход вниз
        let cy2 = yend + self.st.vgap + lh / 2.0;
        self.add("loop_end", tx, cy2, &num.to_string());
        if end != ColEnd::Return {
            self.edge(&[(tx, yend), (tx, cy2 - lh / 2.0)], true);
        }
        let merge = cy2 + lh / 2.0 + self.st.mgap;
        self.edge(&[(tx, cy2 + lh / 2.0), (tx, merge)], false);
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
