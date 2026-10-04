//! Перенос текста по ширине фигуры.
//!
//! Перенос живёт здесь, а не в парсере: парсер отдаёт текст как есть,
//! а «сколько влезет» — вопрос геометрии. Раньше перенос делал удалённый
//! парсер `.gvn`, из-за чего условие `if` на 66 символов доходило до
//! раскладки одной строкой, ромб распухал, а лист сжимался до 0.365
//! вместо 0.49.
//!
//! Перенос нужен ДО `normalize`: размеры фигур считаются по тексту, и
//! однострочный текст даёт другой размер, чем перенесённый.

use crate::ir::{Branch, Node, NodeKind, Stmt};

/// Перенос текста: режем по последнему пробелу внутри окна, но не сразу
/// после оператора — `a *` / `b / 2` отрывает оператор от операнда.
/// Пробелов нет (длинный идентификатор) — режем по границе слова после
/// окна: жёсткий рез по лимиту склеивал `a =` и `long_name`.
pub fn wrap(text: &str, limit: usize) -> String {
    if limit == 0 {
        return text.to_string();
    }
    let mut res: Vec<String> = Vec::new();
    for para in text.split('\n') {
        if para.chars().count() <= limit {
            res.push(para.to_string());
            continue;
        }
        let chars: Vec<char> = para.chars().collect();
        let mut start = 0;
        while chars.len() - start > limit {
            // окно на один символ шире лимита: строка длиной ровно
            // `limit + 1` переносится по этому пробелу, а не по лимиту
            let window = &chars[start..(start + limit).min(chars.len())];
            let cut = match window.iter().rposition(|&c| c == ' ') {
                Some(at) if at > 0 && !after_op(&chars, start + at) => at,
                // за границей окна ищем следующий пробел, чтобы не резать слово
                _ => chars[start..]
                    .iter()
                    .position(|&c| c == ' ')
                    .filter(|&p| p > limit && p < chars.len() - start)
                    .unwrap_or(limit),
            };
            // последняя строка: режем по границе слова, чтобы не рубить
            // идентификатор и не оставлять строку длиннее лимита
            let cut = if cut == limit && start + limit >= chars.len() {
                chars[start..]
                    .iter()
                    .position(|&c| c == ' ')
                    .unwrap_or(chars.len() - start)
            } else {
                cut
            };
            if cut == 0 {
                // слово длиннее окна: режем по лимиту, иначе цикл не идёт
                let line: String = chars[start..start + limit].iter().collect();
                res.push(line);
                start += limit;
                continue;
            }
            let line: String = chars[start..start + cut].iter().collect();
            res.push(line.trim_end().to_string());
            // перенос не съедает разделитель: стартуем с символа после него
            let mut next = start + cut;
            while next < chars.len() && chars[next] == ' ' {
                next += 1;
            }
            if next == start {
                break; // страховка от бесконечного цикла
            }
            start = next;
        }
        res.push(chars[start..].iter().collect::<String>());
    }
    res.join("\n")
}

/// Пробел стоит сразу за бинарным оператором: резать там нельзя,
/// оператор уезжает на следующую строку и теряет правый операнд.
fn after_op(chars: &[char], at: usize) -> bool {
    at.checked_sub(1)
        .map(|k| {
            matches!(
                chars[k],
                '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '!' | '&' | '|'
            )
        })
        .unwrap_or(false)
}

/// Перенос по всем текстам схемы: узлы, тела, ветки, метки кейсов.
pub fn wrap_nodes(nodes: &mut [Node], limit: usize) {
    for nd in nodes.iter_mut() {
        wrap_node(nd, limit);
    }
}

fn wrap_node(nd: &mut Node, limit: usize) {
    if nd.kind == NodeKind::Loop {
        // заголовок цикла несёт ключевое слово: переносим его вместе
        // с текстом, иначе «while» и условие окажутся на разных строках
        nd.text = wrap(&nd.text, limit);
    } else {
        nd.text = wrap(&nd.text, limit);
    }
    if let Some(sv) = &nd.switch_var {
        nd.switch_var = Some(wrap(sv, limit));
    }
    for br in nd.branches.iter_mut() {
        wrap_branch(br, limit);
    }
    if let Some(body) = nd.body.as_mut() {
        wrap_stmts(body, limit);
    }
}

fn wrap_branch(br: &mut Branch, limit: usize) {
    br.label = wrap(&br.label, limit);
    wrap_stmts(&mut br.stmts, limit);
}

fn wrap_stmts(stmts: &mut [Stmt], limit: usize) {
    for s in stmts.iter_mut() {
        match s {
            Stmt::Tile { text, .. } | Stmt::Return(text) => *text = wrap(text, limit),
            Stmt::Node(nd) => wrap_node(nd, limit),
            Stmt::Break | Stmt::Continue => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_untouched() {
        assert_eq!(wrap("a = 1", 30), "a = 1");
    }

    /// Жадный перенос: заполняем строку до лимита. При limit=3 в строку
    /// влезает одно слово, поэтому «a b c d e» даёт четыре строки.
    #[test]
    fn splits_on_space() {
        let w = wrap("a b c d e", 3);
        assert_eq!(w, "a\nb\nc\nd e");
        for line in w.split('\n') {
            assert!(line.chars().count() <= 3, "{line:?}");
        }
    }

    /// Кириллица: срезы в байтах рвут символ пополам.
    #[test]
    fn cyrillic_not_cut_mid_char() {
        let s = "а".repeat(29) + " б";
        for part in wrap(&s, 30).split('\n') {
            assert!(part.chars().count() <= 30, "{part:?}");
        }
    }

    /// Оператор не отрывается от операнда: `a *` / `b / 2` читается как
    /// `a` умножить на ничто.
    #[test]
    fn operator_stays_with_its_operand() {
        let w = wrap("a * b / c", 3);
        for line in w.split('\n') {
            assert!(!line.ends_with(" *"), "{w:?}");
            assert!(!line.ends_with(" /"), "{w:?}");
        }
    }

    /// Перенос не склеивает операнды: жёсткий рез давал `a =long_name`.
    #[test]
    fn long_identifier_keeps_separator() {
        let w = wrap("a = long_name", 4);
        for line in w.split('\n') {
            assert!(!line.starts_with("= "), "{w:?}");
            assert!(!line.starts_with('='), "{w:?}");
        }
    }

    /// Условие if длиной больше лимита переносится, а не рисуется одной
    /// строкой: широкий ромб жмёт лист (регрессия масштаба 0.49 -> 0.365).
    #[test]
    fn long_condition_wraps() {
        let cond = "scanf(\"%d\", &month) != 1 || month < 1 || month > 12";
        let w = wrap(cond, 30);
        assert!(w.contains('\n'), "условие не перенесено: {w:?}");
        for line in w.split('\n') {
            assert!(line.chars().count() <= 30, "{line:?}");
        }
    }

    /// Нулевой лимит не должен зависать: возвращаем текст как есть.
    #[test]
    fn zero_limit_is_passthrough() {
        assert_eq!(wrap("a b c", 0), "a b c");
    }

    /// Перенос не теряет символы: сумма длин строк равна исходной
    /// минус съеденные пробелы-разделители.
    #[test]
    fn no_characters_lost() {
        let s = "alpha beta gamma delta epsilon";
        let w = wrap(s, 10);
        let joined: String = w.split('\n').collect::<Vec<_>>().join(" ");
        assert_eq!(joined, s);
    }
}
