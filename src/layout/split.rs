//! Разбивка длинной схемы на листы А4 с кружками-соединителями.

use crate::ir::{Node, NodeKind};
use crate::layout::{layout, Sizes};
use crate::style::Style;

/// Не влезает в А4 -> части, соединённые кружками «А», «Б», ...
/// Межстраничный соединитель: буква + номер листа, где продолжение
/// (ГОСТ 19.701-90: первая строка — номер листа).
pub fn split_scheme(mut items: Vec<Node>, sizes: &Sizes, st: &Style) -> Vec<Vec<Node>> {
    // поиск точки реза ведётся по нижним 88% страницы: ниже листать
    // бессмысленно, выше — резать слишком рано
    const CUT_SEARCH_FRAC: f64 = 0.88;
    let mut parts: Vec<Vec<Node>> = Vec::new();
    let mut li = 0usize;
    let n_letters = st.letters.chars().count();
    let letter = |li: usize| st.letters.chars().nth(li % n_letters).unwrap();
    loop {
        let res = layout(&items, sizes, st);
        let fits = res.bounds.3 <= (st.a4_h - 2.0 * st.page_pad) / st.split_scale;
        if fits || items.len() < 6 {
            parts.push(items);
            break;
        }
        let limit = (st.a4_h - 2.0 * st.page_pad) * CUT_SEARCH_FRAC;
        let mut cut = None;
        for j in 2..items.len().saturating_sub(2) {
            // резать можно перед простым блоком, «если» или циклом
            if !matches!(
                items[j].kind,
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
    use crate::frontend::gvn;
    use crate::layout::normalize;

    /// return на top-level: layout обрывается, якорей меньше, чем узлов —
    /// split индексировал anchors[j-1] и паниковал (index out of bounds).
    #[test]
    fn split_after_top_level_return_no_panic() {
        let st = Style::default();
        let mut text = "a = 111111\n".repeat(14);
        text.push_str("return 1\n");
        text.push_str(&"z = 222222\n".repeat(4));
        let nodes = gvn::parse(&text, &st, "en").unwrap();
        let sizes = normalize(&nodes, &st);
        let parts = split_scheme(nodes, &sizes, &st);
        assert!(!parts.is_empty());
        assert!(parts.iter().all(|p| !p.is_empty()));
    }
}
