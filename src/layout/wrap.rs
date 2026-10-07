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

use crate::ir::{Branch, Node, Stmt, TileKind};
use crate::style::Style;

use super::measure::{kind_name, measure};

/// Длина самого длинного слова — нижняя граница лимита переноса.
fn longest_word(text: &str) -> usize {
    text.split(|c: char| c.is_whitespace())
        .map(|w| w.chars().count())
        .max()
        .unwrap_or(0)
}
/// Перенос текста: режем по последнему пробелу внутри окна, но не сразу
/// после оператора — `a *` / `b / 2` отрывает оператор от операнда.
/// Пробелов нет (длинный идентификатор) — режем по границе слова после
/// окна: жёсткий рез по лимиту склеивал `a =` и `long_name`.
pub fn wrap(text: &str, limit: usize) -> String {
    wrap_with(text, limit, false)
}

/// Перенос без разрубания слов: если единственный пробел окна оторвал бы
/// оператор от операнда, берём предыдущий пробел, а если пробелов в окне
/// нет — ищем за окном. Строка может выйти на пару символов длиннее
/// лимита, зато `month` не превращается в `m` / `onth`.
fn wrap_soft(text: &str, limit: usize) -> String {
    wrap_with(text, limit, true)
}

fn wrap_with(text: &str, limit: usize, soft: bool) -> String {
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
            let last = window.iter().rposition(|&c| c == ' ');
            let cut = match last {
                Some(at) if at > 0 && !after_op(&chars, start + at) => at,
                _ if soft => {
                    // ищем влево последний пробел, не отрывающий оператор
                    // от операнда; годится любой пробел; нет пробелов
                    // вовсе — берём следующий за окном
                    let alt = window
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|&(i, &c)| c == ' ' && i > 0 && !after_op(&chars, start + i))
                        .map(|(i, _)| i);
                    alt.or(last.filter(|&at| at > 0)).unwrap_or_else(|| {
                        chars[start + limit..]
                            .iter()
                            .position(|&c| c == ' ')
                            .map_or(limit, |p| p + limit)
                    })
                }
                _ => limit,
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
pub fn wrap_nodes(nodes: &mut [Node], st: &Style) {
    for nd in nodes.iter_mut() {
        wrap_node(nd, st);
    }
}

/// Перенос текста по фигуре; объектив зависит от рода фигуры.
///
/// Прямоугольным блокам (act/io/ret) одна строка на ширину листа
/// читается лучше перенесённой колонки: их высота от ширины не зависит
/// (`w.max(2h)`), а потолок ставит лист. Бывший здесь общий перебор
/// «где фигура уже» платил за узость высотой: `denominator = 1`
/// разносился на три строки, и нарезка рвала каждую схему.
///
/// У ромба другая арифметика: высота = ширина (aspect), и длинная
/// строка раздувает его в квадрат — 62 символа дают 513×513 pt, а
/// перенесённый на 2 строки текст — 285×285. Поэтому роман переносим
/// на узкие строки: перебор лимитов, где фигура уже.
fn wrap_best(kind: &str, text: &str, st: &Style) -> String {
    if kind != "if" {
        // потолок строки — книжная зона: и книжный, и альбомный лист
        // вмещают её целиком
        let page_limit = ((crate::sheet::Sheet::A4.text_w() - st.pad_x - st.text_pad) / st.char_w)
            .floor()
            .max(1.0) as usize;
        if text.chars().count() <= page_limit {
            return text.to_string();
        }
        return wrap_soft(text, page_limit);
    }
    let mut best = wrap_soft(text, st.max_chars);
    let mut best_w = measure(st, kind, &best).0;
    // Слово длиннее лимита перенос всё равно разрежет — тогда перебираем
    // все лимиты; иначе держимся не ниже самого длинного слова, чтобы
    // `month` не превратилось в `m` / `onth`.
    let longest = longest_word(text);
    let floor = if longest >= st.max_chars {
        1
    } else {
        longest.max(1)
    };
    // от длинных лимитов к коротким: при равной ширине остаётся
    // первое найденное, то есть лимит побольше и строк поменьше
    for limit in (floor..st.max_chars).rev() {
        let cand = wrap_soft(text, limit);
        if cand == best {
            continue;
        }
        let w = measure(st, kind, &cand).0;
        if w < best_w - 1e-9 {
            best_w = w;
            best = cand;
        }
    }
    best
}

fn wrap_node(nd: &mut Node, st: &Style) {
    // заголовок цикла несёт ключевое слово: переносим его вместе
    // с текстом, иначе «while» и условие окажутся на разных строках
    nd.text = wrap_best(kind_name(&nd.kind), &nd.text, st);
    if let Some(sv) = &nd.switch_var {
        nd.switch_var = Some(wrap(sv, st.max_chars));
    }
    for br in nd.branches.iter_mut() {
        wrap_branch(br, st);
    }
    if let Some(body) = nd.body.as_mut() {
        wrap_stmts(body, st);
    }
}

fn wrap_branch(br: &mut Branch, st: &Style) {
    br.label = wrap(&br.label, st.max_chars);
    wrap_stmts(&mut br.stmts, st);
}

fn wrap_stmts(stmts: &mut [Stmt], st: &Style) {
    for s in stmts.iter_mut() {
        match s {
            Stmt::Tile { kind, text } => {
                let k = match kind {
                    TileKind::Io => "io",
                    TileKind::Act => "act",
                };
                *text = wrap_best(k, text, st);
            }
            Stmt::Return(text) => *text = wrap_best("ret", text, st),
            Stmt::Node(nd) => wrap_node(nd, st),
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

    /// Мягкий перенос (его берёт подбор лимита) не рубит слова: если
    /// единственный годный пробел окна оторвал бы оператор от операнда,
    /// он уступает место предыдущему. Жёсткий `wrap` на этом же входе
    /// режет по лимиту — и `month` распадается на `m` / `onth`.
    #[test]
    fn soft_wrap_keeps_words_whole() {
        let src = "scanf(\"%d\", &month) != 1 || month < 1 || month > 12";
        let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let soft = wrap_soft(src, 17);
        assert_eq!(words(&soft.replace('\n', " ")), words(src), "{soft:?}");
        let hard = wrap(src, 17);
        assert_ne!(
            words(&hard.replace('\n', " ")),
            words(src),
            "жёсткий перенос обязан остаться жёстким: {hard:?}"
        );
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
