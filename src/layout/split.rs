//! Разбивка длинной схемы на листы А4 с кружками-соединителями.

use crate::ir::{Branch, Node, NodeKind, Stmt};
use crate::layout::{layout, Sizes};
use crate::style::Style;

/// Не влезает в А4 -> части, соединённые кружками «А», «Б», ...
/// Межстраничный соединитель: буква + номер листа, где продолжение
/// (ГОСТ 19.701-90: первая строка — номер листа).
pub fn split_scheme(mut items: Vec<Node>, sizes: &Sizes, st: &Style) -> Vec<Vec<Node>> {
    // поиск точки реза ведётся по нижним 88% текстовой зоны листа
    const CUT_SEARCH_FRAC: f64 = 0.88;
    let mut parts: Vec<Vec<Node>> = Vec::new();
    let mut li = 0usize;
    let n_letters = st.letters.chars().count();
    let letter = |li: usize| st.letters.chars().nth(li % n_letters).unwrap();
    loop {
        let res = layout(&items, sizes, st);
        // «влезает ли» решает лист: тот же вопрос, что и при рендере,
        // те же числа. Раньше здесь была своя формула (высота против
        // обеих осей у fit_scale) с собственным page_pad.
        let (_, _, w, h) = res.bounds;
        let fits = st.sheet.fits(w, h, st.split_scale);
        if fits || items.len() < 6 {
            // Порезка по верхнему уровню не помогла: ромб с десятком
            // кейсов — один узел, резать его нечем, а схема в лист не
            // влезает ни в каком масштабе (на реальной лабе — 12 кейсов
            // switch давали 2182 pt высоты и кегль 4.2 pt).
            // Разрезаем такой ромб ветвями: половина кейсов уходит на
            // следующий лист за кружком-соединителем.
            if !fits {
                let l = letter(li);
                if let Some((head, tail)) =
                    cut_decision(&items, sizes, st, l).or_else(|| cut_loop(&items, sizes, st, l))
                {
                    li += 1;
                    parts.push(head);
                    items = tail;
                    continue;
                }
            }
            parts.push(items);
            break;
        }
        // ищем последнюю точку реза не ниже 88% текстовой зоны: ниже
        // резать бессмысленно, выше — слишком рано
        let limit = st.sheet.text_h() * CUT_SEARCH_FRAC;
        let mut cut = None;
        // точки реза: 2..len-2 (как раньше) — take до, skip после,
        // чтобы saturating_sub не дал underflow на пустой схеме
        let end = items.len().saturating_sub(2);
        for (j, item) in items.iter().enumerate().take(end).skip(2) {
            // резать можно перед простым блоком, «если» или циклом
            if !matches!(
                item.kind,
                NodeKind::Act | NodeKind::Io | NodeKind::Decision | NodeKind::Loop
            ) {
                continue;
            }
            // return на top-level обрывает layout: якорей меньше, чем узлов
            let Some(a) = res.anchors.get(j.wrapping_sub(1)) else {
                continue;
            };
            if a.y <= limit {
                cut = Some(j);
            }
        }
        let Some(cut) = cut else {
            parts.push(items);
            break;
        };
        let l = letter(li);
        li += 1;
        let mut head = items[..cut].to_vec();
        head.push(Node::new(NodeKind::Conn, l.to_string()));
        parts.push(head);
        let mut tail = vec![Node::new(NodeKind::Conn, l.to_string())];
        tail.extend(items.drain(cut..));
        items = tail;
    }
    /// Разрез ромба/переключателя ветвями: (голова, хвост) для листа и
    /// буква кружка, которой они склеены.
    ///
    /// Порезка по верхнему уровню режет только между узлами, а `switch` на
    /// 12 кейсов — один узел: в лист он не влезает и резать его нечем,
    /// поэтому схема печаталась нечитаемой (на реальной лабе — 2182 pt
    /// высоты и кегль 4.2 pt). Здесь ромб делится пополам по ветвям,
    /// ушедшие кейсы становятся ветвями такого же ромба на следующем
    /// листе, а листы склеивает буква в кружке — обычный приём разрыва
    /// схемы по ГОСТ 19.701-90.
    ///
    /// Разрез идёт чётным числом кейсов, чтобы не разрывать ряд сетки
    /// пополам. `None`, если резать нечего: кейсов меньше четырёх, либо
    /// это каскад else-if (ветки у него не независимы), либо голова не
    /// стала ниже — иначе порезка крутила бы один и тот же ромб вечно.
    fn cut_decision(
        items: &[Node],
        sizes: &Sizes,
        st: &Style,
        letter: char,
    ) -> Option<(Vec<Node>, Vec<Node>)> {
        let at = items.iter().rposition(|nd| {
            nd.kind == NodeKind::Decision
                && nd.branches.iter().filter(|b| !b.stmts.is_empty()).count() >= 4
                && !is_cascade(nd)
        })?;
        let nd = &items[at];
        let n_ne = nd.branches.iter().filter(|b| !b.stmts.is_empty()).count();
        let conn = || Node::new(NodeKind::Conn, letter.to_string());
        let limit = st.sheet.text_h();

        // Сколько кейсов помещается на лист, формулой не выводится: высота
        // зависит от переноса подписей в плитках, от `merge_y` в сетке и
        // от того, уместился ли ряд кейсов на шину или встал в два ряда.
        // Поэтому перебираем от большего к меньшему и берём первый,
        // который влез. Чётность нужна, чтобы не разорвать ряд сетки.
        let mut halves: Vec<usize> = (1..=(n_ne - 2) / 2).map(|k| k * 2).collect();
        halves.reverse();
        for keep in halves {
            let Some((head, tail)) = split_at(nd, at, keep, items, &conn) else {
                continue;
            };
            if layout(&head, sizes, st).bounds.3 < limit {
                return Some((head, tail));
            }
        }
        None
    }

    /// Ромб, у которого на этом листе остаётся `keep` кейсов с телом.
    /// Остальные теряют тело (подпись остаётся — порезка рисует пустые
    /// ветки рельсами, и читатель видит, какие кейсы ушли на следующий
    /// лист) и целиком переезжают в такой же ромб следующего листа.
    /// `None`, если столько кейсов не набралось.
    fn split_at(
        nd: &Node,
        at: usize,
        keep: usize,
        items: &[Node],
        conn: &dyn Fn() -> Node,
    ) -> Option<(Vec<Node>, Vec<Node>)> {
        // Уехавшие ветки УДАЛЯЮТСЯ, а не остаются пустыми: пустая ветка
        // рисуется рельсой вбок, и десяток таких рельс раздвигал лист на
        // треть ширины — вписывание падало вдвое. На следующем листе эти
        // кейсы подписаны у своего ромба, а кружок с буквой и говорит,
        // что диспетч продолжен.
        let live: Vec<Branch> = nd
            .branches
            .iter()
            .filter(|b| !b.stmts.is_empty())
            .cloned()
            .collect();
        let mut head_nd = nd.clone();
        head_nd.branches = live[..keep].to_vec();
        let mut tail_nd = Node::new(NodeKind::Decision, nd.text.clone());
        tail_nd.switch_var = nd.switch_var.clone();
        tail_nd.branches = live[keep..].to_vec();
        let mut head = items[..at].to_vec();
        head.push(head_nd);
        head.push(conn());
        let mut tail = vec![conn(), tail_nd];
        tail.extend_from_slice(&items[at + 1..]);
        Some((head, tail))
    }

    /// Каскад else-if: последняя ветка ромба — вложенный ромб. Такой
    /// резать нельзя, ветки у него не независимы.
    fn is_cascade(nd: &Node) -> bool {
        matches!(crate::layout::ifnode::cascade_cols(nd), Some((k, _)) if k >= 2)
    }

    /// Разрез цикла телом: (голова, хвост).
    ///
    /// Цикл с пятью вложенными циклами — один верхнеуровневый узел, и он
    /// точно так же не режется порезкой по узлам, как ромб по ветвям.
    /// Такой цикл давал лист с кеглем 6.3 pt; теперь тело делится
    /// пополам, а листы склеивает кружок с буквой.
    ///
    /// Разбивка честная: на первом листе верхняя трапеция без нижней, на
    /// последнем нижняя без верхней. Цикл нигде не выглядит замкнутым.
    fn cut_loop(
        items: &[Node],
        sizes: &Sizes,
        st: &Style,
        letter: char,
    ) -> Option<(Vec<Node>, Vec<Node>)> {
        let at = items.iter().rposition(|nd| {
            nd.kind == NodeKind::Loop
                && nd.body.as_ref().is_some_and(|b| b.len() >= 4)
                && !nd.cont
                && !nd.cont_out
        })?;
        let nd = &items[at];
        let body = nd.body.as_ref().unwrap();
        let conn = || Node::new(NodeKind::Conn, letter.to_string());
        let limit = st.sheet.text_h();

        for keep in (1..body.len() - 1).rev() {
            // `continue` этого цикла на первом листе уводит к нижней
            // трапеции, которой там нет: такой разрез не рисуем
            if body[..keep].iter().any(|s| matches!(s, Stmt::Continue)) {
                continue;
            }
            let mut head = items[..at].to_vec();
            let mut head_loop = nd.clone();
            head_loop.body = Some(body[..keep].to_vec());
            head.push(head_loop.continued(false, true));
            head.push(conn());
            if layout(&head, sizes, st).bounds.3 >= limit {
                continue;
            }
            let mut tail = vec![conn()];
            let mut tail_loop = nd.clone();
            tail_loop.body = Some(body[keep..].to_vec());
            tail.push(tail_loop.continued(true, false));
            tail.extend_from_slice(&items[at + 1..]);
            return Some((head, tail));
        }
        None
    }

    // ветки «-> конец» в не-последних частях кончаются кружком:
    // сам «конец» живёт на последнем листе, туда же ставятся его кружки
    let mut links: Vec<(char, usize)> = Vec::new();
    for pi in 0..parts.len().saturating_sub(1) {
        for nd in &mut parts[pi] {
            if nd.kind != NodeKind::Decision {
                continue;
            }
            for br in &mut nd.branches {
                if br.to_end && br.link.is_none() {
                    let l = letter(li);
                    li += 1;
                    br.link = Some(l);
                    links.push((l, pi + 1));
                }
            }
        }
    }
    links.sort();
    // входящие кружки у «конца»: ir.rs закрыт для правок, поэтому
    // inbound моделируется conn-узлами прямо перед терминатором —
    // layout рисует их слева от «конца» (leads_to_end)
    for (l, _) in &links {
        let last = parts.len() - 1;
        let at = parts[last].len() - 1;
        parts[last].insert(at, Node::new(NodeKind::Conn, l.to_string()));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::NodeKind;
    use crate::layout::normalize;

    /// return на top-level: layout обрывается, якорей меньше, чем узлов —
    /// split индексировал anchors[j-1] и паниковал (index out of bounds).
    #[test]
    fn split_after_top_level_return_no_panic() {
        let st = Style::default();
        // верхнеуровневый return C-фронтенд отбрасывает, поэтому узел
        // Return ставим руками — проверяет тест раскладку
        let mut nodes = vec![Node::new(NodeKind::Term, "Start")];
        nodes.extend((0..14).map(|_| Node::new(NodeKind::Act, "a = 111111")));
        nodes.push(Node::new(NodeKind::Return, "return 1"));
        nodes.extend((0..4).map(|_| Node::new(NodeKind::Act, "z = 222222")));
        nodes.push(Node::new(NodeKind::Term, "End"));
        let sizes = normalize(&nodes, &st);
        let parts = split_scheme(nodes, &sizes, &st);
        assert!(!parts.is_empty());
        assert!(parts.iter().all(|p| !p.is_empty()));
    }
}

#[cfg(test)]
mod branch_cut_tests {
    use super::*;
    use crate::ir::{Branch, Stmt, TileKind};
    use crate::layout::normalize;

    fn switch(cases: usize) -> Node {
        let mut nd = Node::new(NodeKind::Decision, "switch (a)");
        nd.switch_var = Some("a".into());
        for i in 0..cases {
            nd.branches.push(Branch {
                label: format!("{}", i + 1),
                stmts: vec![Stmt::Tile {
                    kind: TileKind::Act,
                    text: format!("case_{}", i + 1),
                }],
                to_end: false,
                link: None,
            });
        }
        nd
    }

    fn scheme(cases: usize) -> Vec<Node> {
        vec![
            Node::new(NodeKind::Term, "начало"),
            switch(cases),
            Node::new(NodeKind::Term, "конец"),
        ]
    }

    /// Переключатель на 12 кейсов — 2182 pt высоты. В лист он не влезает
    /// ни в каком масштабе, а порезка по верхнему уровню его не берёт:
    /// ромб это один узел. Без разреза по ветвям схема печаталась с
    /// кеглем 4.6 pt вместо 14 — то есть не читалась совсем.
    #[test]
    fn big_switch_is_split_by_branches() {
        let st = Style::default();
        let items = scheme(12);
        let sizes = normalize(&items, &st);
        let whole = layout(&items, &sizes, &st).bounds;
        assert!(
            !st.sheet.fits(whole.2, whole.3, st.split_scale),
            "схема должна быть не по листу, иначе тест бессмыслен"
        );
        let parts = split_scheme(items, &sizes, &st);
        assert!(parts.len() > 1, "резать нечем: порезка вернула один лист");
        assert!(parts.iter().all(|p| !p.is_empty()), "пустой лист");
    }

    /// Схема не должна быть и шире листа: раньше проверялась только
    /// высота, и широкая шина кейсов проходила как годная, а лист
    /// отдавался с масштабом 0.13.
    #[test]
    fn split_respects_page_width_too() {
        let st = Style::default();
        let items = scheme(12);
        let sizes = normalize(&items, &st);
        for part in split_scheme(items, &sizes, &st) {
            let b = layout(&part, &sizes, &st).bounds;
            assert!(
                st.sheet.fits(b.2, b.3, st.split_scale),
                "лист {}×{} не влезает: проверяли только высоту",
                b.2,
                b.3
            );
        }
    }

    /// Кружок с буквой в конце первого листа и в начале следующего:
    /// без него страницы не связаны и схема выглядит оборванной.
    #[test]
    fn split_parts_are_joined_by_conn_circle() {
        let st = Style::default();
        let items = scheme(12);
        let sizes = normalize(&items, &st);
        let parts = split_scheme(items, &sizes, &st);
        let a = parts.first().unwrap().last().unwrap();
        let b = parts.get(1).unwrap().first().unwrap();
        assert_eq!(a.kind, NodeKind::Conn, "нет кружка в конце листа");
        assert_eq!(b.kind, NodeKind::Conn, "нет кружка в начале листа");
        assert_eq!(a.text, b.text, "буквы кружков должны совпадать");
    }

    /// Ни один кейс не теряется: сумма по листам равна исходному числу.
    /// Тихо терять ветки нельзя — схема врёт.
    #[test]
    fn no_case_is_lost_by_split() {
        let st = Style::default();
        for n in [4usize, 6, 8, 12, 20] {
            let items = scheme(n);
            let sizes = normalize(&items, &st);
            let seen: usize = split_scheme(items, &sizes, &st)
                .iter()
                .flatten()
                .filter(|nd| nd.kind == NodeKind::Decision && nd.switch_var.is_some())
                .map(|nd| nd.branches.iter().filter(|b| !b.stmts.is_empty()).count())
                .sum();
            assert_eq!(seen, n, "кейсов {n}, на листах {seen}");
        }
    }

    /// Мелкий ромб резать нельзя: два листа по три кейса хуже одного.
    #[test]
    fn small_decision_is_not_split() {
        let st = Style::default();
        let items = scheme(2);
        let sizes = normalize(&items, &st);
        assert_eq!(
            split_scheme(items, &sizes, &st).len(),
            1,
            "мелкий ромб не режется"
        );
    }
}
