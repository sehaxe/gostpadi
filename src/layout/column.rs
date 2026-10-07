use super::ctx::{BreakAt, ColEnd, Ctx};
use super::measure::kind_name;
use super::types::Sizes;
use crate::ir::{Branch, NodeKind, Stmt, TileKind};
use crate::style::Style;

/// Колонка, в которую нечего входить: её тело не рисует ни одного
/// блока, поэтому стрелка в неё была бы стрелкой в пустоту.
///
/// Раньше сюда попадал и `break`: он уходил рельсой и сам блока не
/// рисовал. Теперь `break` — обычный прямоугольник (ADR-0004), и вход
/// в него со стрелкой обязателен. Единственная инструкция, после
/// которой в колонке не остаётся ничего видимого, — `continue`.
pub(super) fn rail_only(items: &[Stmt]) -> bool {
    !items.is_empty() && items.iter().all(|s| matches!(s, Stmt::Continue))
}

/// Сколько ярусов вправо от оси занимает разлёт веток ромба: столько,
/// сколько даёт шина, но только если шина вообще влезает в лист.
///
/// Решение по ширине листа, а не по числу кейсов, — но по **локальной**
/// полуширине колонки ветки, а не по общей `nhe`. Общая в этом месте
/// давала круг: `nhe` растёт от ярусов, ярусы считаются от `nhe`... и
/// на реальной лабе (`switch` на 12 кейсов внутри `else`) `nhe`
/// разъезжался до 1445 pt, после чего сетка кейсов раскладывалась по
/// колонкам шириной 1563 pt и лист ужимался до 13 %.
///
/// Тот же предикат использует рендер (`sub_if`), поэтому габарит и
/// реальная раскладка считаются по одним числам.
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
            // Диспетч всегда раскладывается одной шиной: кейсы в один ряд,
            // число ярусов — `max_tier`. Раньше здесь стоял `bus_tiers`,
            // который при нехватке ширины возвращал 0 (кейсы уходили в
            // сетку), и полуширина колонки занижалась вдвое — рельса
            // continue уходила по блокам.
            let tiers = super::ifnode::max_tier(n);
            let pitch2 = 2.0 * sub + st.colgap;
            he = he
                // + sub в конце обязателен: колонка ветки стоит на
                // расстоянии base2 = dw2/2 + hgap + sub от оси, а её
                // собственная полуширина — ещё sub. Без этого nhe
                // занижался, и рельса continue уходила по блокам
                // (ловил --check на реальной лабе: while с if/else и
                // switch внутри).
                .max(dw2 / 2.0 + st.hgap + sub + tiers as f64 * pitch2 + sub)
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
    /// Возвращает (y низа, чем закончена): тупик return закрывает
    /// колонку, рельса break/continue уводит поток к scope-владельцу.
    pub(super) fn render_column(&mut self, items: &[Stmt], tx: f64, top0: f64) -> (f64, ColEnd) {
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
                    let (y_bot, end) = if nd.kind == NodeKind::Decision {
                        self.sub_if(nd, tx, top)
                    } else {
                        self.sub_loop(nd, tx, top)
                    };
                    if end != ColEnd::Flow {
                        // тупик или рельса: следующие инструкции колонки
                        // недостижимы, слияние колонки не рисуется
                        return (y_bot, end);
                    }
                    prev_bottom = Some(y_bot);
                }
                Stmt::Return(t) => {
                    let (_, h_r) = self.sizes["ret"];
                    if let Some(pb) = prev_bottom {
                        self.edge(&[(tx, pb), (tx, top)], true);
                    }
                    self.add("ret", tx, top + h_r / 2.0, t);
                    return (top + h_r, ColEnd::Return);
                }
                Stmt::Break | Stmt::Continue => {
                    // Раствор break: напрямую в кейс-колонке switch
                    // (без цикла вокруг) колонка кончается, дренаж
                    // сам доводит поток до шины switch — как в эталоне
                    // старого движка (strany-2). Плитка «break» под
                    // каждым кейсом раздувала каждый ряд вдвое, и
                    // диспетч из 13 кейсов не влезал ни в один лист.
                    // Обрыв колонки к шине читается как break сам по
                    // себе: в C kейсовый break иного и не делает.
                    //
                    // Внутри цикла break = плитка: рельса тут была бы
                    // ложью (break выходит из цикла, а не возвращается
                    // к началу, как continue). Иначе — рельса.
                    if matches!(it, Stmt::Break)
                        && self.loop_depth == 0
                        && self.switch_depth > 0
                        && self.case_direct
                    {
                        return (prev_bottom.unwrap_or(top0), ColEnd::Flow);
                    }
                    if matches!(it, Stmt::Break) {
                        self.draw_tile("break", TileKind::Act, tx, top, &mut prev_bottom);
                        continue;
                    }
                    let scoped = self.loop_depth > 0 || self.switch_depth > 0;
                    if scoped {
                        self.continues.push(BreakAt {
                            tx,
                            y: prev_bottom.unwrap_or(top0),
                        });
                        return (prev_bottom.unwrap_or(top0), ColEnd::Rail);
                    }
                    // вне цикла и switch — обычная плитка
                    self.draw_tile("continue", TileKind::Act, tx, top, &mut prev_bottom);
                }
                Stmt::Tile { kind, text } => {
                    self.draw_tile(text, *kind, tx, top, &mut prev_bottom);
                }
            }
        }
        (prev_bottom.unwrap_or(top0), ColEnd::Flow)
    }

    /// Обычная плитка на колонке.
    fn draw_tile(
        &mut self,
        text: &str,
        kind: TileKind,
        tx: f64,
        top: f64,
        prev_bottom: &mut Option<f64>,
    ) {
        let k = match kind {
            TileKind::Io => "io",
            TileKind::Act => "act",
        };
        let (_, h_k) = self.sizes[k];
        let bottom = top + h_k;
        self.add(k, tx, top + h_k / 2.0, text);
        if let Some(pb) = *prev_bottom {
            self.edge(&[(tx, pb), (tx, top)], true);
        }
        *prev_bottom = Some(bottom);
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
        self.anchors.push(super::Anchor { y: cy + h / 2.0 });
        ((0.0, cy + h / 2.0), cy + h / 2.0, i)
    }
}
