//! Frontend C на tree-sitter: не падает на битом коде.
//!
//! lang-c — строгий C99: одна незакрытая скобка, и схема не строится
//! вовсе. Студенческий код с `scanf_s`, макросами MSVC и потерянной
//! скобкой не рисуется вообще. tree-sitter разбирает ЛЮБОЙ текст и
//! помечает непонятное как ERROR; фронтенд рисует такой узел обычным
//! блоком со сырым текстом. Схема получается всегда.
//!
//! Текст ноды берётся из исходника по span'у, а не печатается заново:
//! значит сохраняются пробелы, кавычки и `&radius` — те самые, что
//! рвались при усечении.

use crate::frontend::{merge_case_aliases, tile_kind};
use crate::ir::{Branch, LoopKind, Node, NodeKind, Stmt};
use tree_sitter::{Node as Ts, Parser};

#[derive(Default)]
pub struct CParser {
    parser: Parser,
}

impl CParser {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("tree-sitter-c");
        Self { parser }
    }

    /// Код C -> узлы схемы с терминаторами. Никогда не падает на
    /// синтаксисе: непонятное становится блоком с сырым текстом.
    pub fn parse(&mut self, src: &str, labels: &str) -> Vec<Node> {
        let tree = self
            .parser
            .parse(src, None)
            .expect("tree-sitter вернул None без таймаута");
        let mut nodes: Vec<Node> = Vec::new();
        let root = tree.root_node();

        // При битом коде грамматика может не собрать function_definition
        // вовсе: `int main() { int a = ; return }` — прерванное объявление,
        // и функция теряется целиком. Тогда ищем тело main по тексту: блок
        // разбирается, схема рисуется частично. Без этого был пустой лист
        // с внятным, но бесполезным «main не найден».
        let main_fn = find_function(root, src, "main");
        let body = main_fn
            .and_then(|f| {
                f.child_by_field_name("body")
                    .or_else(|| child_of_kind(f, "compound_statement"))
            })
            .or_else(|| salvage_body(root, src));
        let ctx = Ctx { src };
        match body {
            Some(body) => {
                for child in named_children(body) {
                    ctx.statement(child, &mut nodes, labels);
                }
                // Верхнеуровневый return не рисуем: терминатор «конец» и
                // так завершает схему. Внутри if/цикла он остаётся —
                // там он конец ветки, и без него она не закрыта.
                if let Some(pos) = nodes.iter().rposition(|n| n.kind == NodeKind::Return) {
                    if nodes[pos + 1..].iter().all(|n| n.kind == NodeKind::Term) {
                        nodes.remove(pos);
                    }
                }
            }
            None => {
                // main найден, но тело не разобралось: рисуем хоть что
                // есть, иначе схема молча пустая
                if let Some(f) = main_fn {
                    for child in named_children(f) {
                        if child.kind() != "identifier" && child.kind() != "primitive_type" {
                            ctx.statement(child, &mut nodes, labels);
                        }
                    }
                }
            }
        }

        // Терминаторы ставит фронтенд: без них схема по ГОСТ не схема.
        let (start, end) = match labels {
            "ru" => ("начало", "конец"),
            _ => ("Start", "End"),
        };
        let mut out = vec![Node::new(NodeKind::Term, start)];
        out.append(&mut nodes);
        out.push(Node::new(NodeKind::Term, end));
        out
    }
}

/// Что разбирать, когда main не собрался функцией. При битом коде
/// грамматика кладёт в ERROR всю функцию, тело main не появляется как
/// compound_statement вовсе. Возвращаем узел, ЧЬИХ ДЕТЕЙ нужно разобрать:
/// и блок тела, и сам ERROR.
fn salvage_body<'t>(root: Ts<'t>, src: &str) -> Option<Ts<'t>> {
    // 1. блок тела, если всё-таки есть
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        for c in named_children(cur) {
            if c.kind() == "compound_statement" && precedes_main(c, src) {
                return Some(c);
            }
            stack.push(c);
        }
    }
    // 2. ERROR-узел, внутри которого в исходнике есть `main(`
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        for c in named_children(cur) {
            if c.kind() == "ERROR" && precedes_main(c, src) {
                return Some(c);
            }
            stack.push(c);
        }
    }
    None
}

/// В узле или перед ним в исходнике встречается `main(` — значит это
/// оно. При битом коде в узле может лежать вся функция, а перед блоком
/// тела `main` стоит уже не внутри узла.
fn precedes_main(n: Ts, src: &str) -> bool {
    let inside = n
        .utf8_text(src.as_bytes())
        .unwrap_or_default()
        .contains("main(");
    if inside {
        return true;
    }
    let before = &src[..n.start_byte().min(src.len())];
    before
        .rmatch_indices("main")
        .next()
        .is_some_and(|(at, _)| before[at + 4..].trim_start().starts_with('('))
}

/// Узел функции по имени: main — точка входа схемы.
fn find_function<'t>(root: Ts<'t>, src: &str, name: &str) -> Option<Ts<'t>> {
    named_children(root).into_iter().find(|n| {
        n.kind() == "function_definition" && declarator_name(*n, src).as_deref() == Some(name)
    })
}

/// Имя функции: первый `identifier` под function_declarator.
/// `int main(void)` -> "main". Поиск по виду, а не по вложенности
/// declarator'ов: у дерева нет поля «имя», и структура declarator'а
/// меняется от `*f()` до `(*f)()`, а первый идентификатор — всегда имя.
fn declarator_name(f: Ts, src: &str) -> Option<String> {
    // function_definition -> function_declarator -> identifier. Промежуточного
    // `declarator` в этой грамматике нет, но у указателя на функцию он
    // появляется (pointer_declarator), поэтому ищем в обоих местах.
    let fd = child_of_kind(f, "function_declarator")
        .or_else(|| {
            child_of_kind(f, "declarator").and_then(|d| child_of_kind(d, "function_declarator"))
        })
        .or_else(|| {
            // `int (*f)(void)`: function_declarator лежит глубже
            first_function_declarator(f)
        });
    fd.and_then(first_identifier)
        .map(|id| text(id, src).to_string())
}

fn first_function_declarator<'t>(n: Ts<'t>) -> Option<Ts<'t>> {
    if n.kind() == "function_declarator" {
        return Some(n);
    }
    for c in named_children(n) {
        if let Some(f) = first_function_declarator(c) {
            return Some(f);
        }
    }
    None
}

/// Первый `identifier` в поддереве в глубину, слева направо.
fn first_identifier<'t>(n: Ts<'t>) -> Option<Ts<'t>> {
    if n.kind() == "identifier" {
        return Some(n);
    }
    for c in named_children(n) {
        if let Some(found) = first_identifier(c) {
            return Some(found);
        }
    }
    None
}

struct Ctx<'a> {
    src: &'a str,
}

impl Ctx<'_> {
    /// Разбор одного оператора в список узлов верхнего уровня.
    fn statement(&self, n: Ts, out: &mut Vec<Node>, labels: &str) {
        match n.kind() {
            "if_statement" => self.if_stmt(n, out, labels),
            "while_statement" => self.loop_stmt(n, out, labels, LoopKind::While),
            "for_statement" => self.loop_stmt(n, out, labels, LoopKind::For),
            "do_statement" => self.loop_stmt(n, out, labels, LoopKind::DoWhile),
            "switch_statement" => self.switch_stmt(n, out, labels),
            "return_statement" => {
                // Ключевое слово — часть текста блока: `return` без
                // значения на схеме неотличим от числа.
                let value = named_children(n)
                    .into_iter()
                    .find(|c| c.kind() != "return")
                    .map(|v| flatten(text(v, self.src), self.src))
                    .unwrap_or_default();
                let t = if value.is_empty() {
                    "return".to_string()
                } else {
                    format!("return {value}")
                };
                out.push(Node::new(NodeKind::Return, t));
            }
            "break_statement" => out.push(Node::new(NodeKind::Act, "break")),
            "continue_statement" => out.push(Node::new(NodeKind::Act, "continue")),
            "compound_statement"
            | "init_declarator"
            | "assignment_expression"
            | "binary_expression"
            | "update_expression"
            | "argument_list"
            | "parameter_list"
            | "comma_expression"
            | "cast_expression"
            | "subscript_expression"
            | "pointer_expression"
            | "field_expression" => {
                // Обёртки: у них нет своего смысла на схеме, содержимое
                // рисуется как обычные блоки. Перечислены явно, а не
                // ловится общим правилом — иначе `a` и `a;` рисуются
                // одинаково, а по схеме это разные вещи.
                for c in named_children(n) {
                    self.statement(c, out, labels);
                }
            }
            "labeled_statement" => {
                // метка case/default разбирает switch; прочие (goto-метки)
                // рисуем как блок, чтобы не потерять
                let inner = child_of_kind(n, "statement");
                match inner {
                    Some(_) if n.child_by_field_name("value").is_none() => {
                        let t = text(n, self.src).to_string();
                        out.push(Node::new(tile_node_kind(&t), t));
                    }
                    _ => {
                        if let Some(s) = inner {
                            self.statement(s, out, labels);
                        }
                    }
                }
            }
            "case_statement" => {
                // вне switch (бывает при битом коде): рисуем содержимое
                for c in named_children(n) {
                    if c.kind() != "case" && !c.is_named() {
                        continue;
                    }
                    if c.kind() == "case" || text(c, self.src) == "default" {
                        continue;
                    }
                    self.statement(c, out, labels);
                }
            }
            "comment"
            | "preproc_include"
            | "preproc_def"
            | "preproc_ifdef"
            | "preproc_function_def"
            | "preproc_call"
            | "preproc_if" => {}
            "declaration" => {
                // `int a = 1;` рисуем инициализатором: `int a` без значения
                // на схеме ничего не делает
                for d in named_children(n) {
                    if d.kind() != "init_declarator" {
                        continue;
                    }
                    let name = child_of_kind(d, "identifier")
                        .or_else(|| first_identifier(d))
                        .map(|i| text(i, self.src).to_string())
                        .unwrap_or_default();
                    let init = child_of_kind(d, "initializer")
                        .or_else(|| {
                            named_children(d)
                                .into_iter()
                                .find(|c| !c.is_named() || c.kind() != "identifier")
                        })
                        .map(|i| flatten(text(i, self.src), self.src))
                        .unwrap_or_default();
                    if init.is_empty() {
                        continue;
                    }
                    let t = format!("{name} = {init}");
                    out.push(Node::new(tile_node_kind(&t), t));
                }
            }
            "expression_statement" => {
                let t = flatten(text(n, self.src), self.src);
                if !t.is_empty() {
                    out.push(Node::new(tile_node_kind(&t), t));
                }
            }
            "ERROR" => {
                // Битый узел. Сначала пробуем разобрать содержимое: при
                // `for (;;` грамматика оборачивает в ERROR всю функцию
                // вместе с разобранным началом, и обход по детям
                // возвращает то, что удалось понять. Не разобралось —
                // рисуем сырой текст блоком.
                let before = out.len();
                for c in named_children(n) {
                    self.statement(c, out, labels);
                }
                if out.len() == before {
                    let t = flatten(text(n, self.src), self.src);
                    if !t.is_empty() {
                        out.push(Node::new(tile_node_kind(&t), t));
                    }
                }
            }
            // Сигнатура функции, разобранная внутри ERROR: тела нет,
            // а объявление само по себе не рисуется.
            "primitive_type" | "function_declarator" => {}
            _ => {
                // неизвестная конструкция: содержимое внутрь, если есть
                let kids = named_children(n);
                if kids.is_empty() {
                    let t = flatten(text(n, self.src), self.src);
                    if !t.is_empty() {
                        out.push(Node::new(tile_node_kind(&t), t));
                    }
                } else {
                    for c in kids {
                        self.statement(c, out, labels);
                    }
                }
            }
        }
    }

    fn if_stmt(&self, n: Ts, out: &mut Vec<Node>, labels: &str) {
        let cond = n
            .child_by_field_name("condition")
            .map(|c| unparen(text(c, self.src)))
            .unwrap_or_default();

        let yes_label = if labels == "ru" { "да" } else { "yes" };
        let no_label = if labels == "ru" { "нет" } else { "no" };

        let mut branches: Vec<Branch> = Vec::new();
        let mut node = Node::new(NodeKind::Decision, format!("if ({cond})"));

        let consequence = n.child_by_field_name("consequence");
        let alternative = n.child_by_field_name("alternative");

        let mut yes = Vec::new();
        if let Some(c) = consequence {
            self.branches_of(c, &mut yes, labels);
        }
        branches.push(Branch {
            label: yes_label.into(),
            stmts: yes,
            to_end: false,
            link: None,
        });

        // Ветка «нет» есть ВСЕГДА, даже при `if (x);` без else: ромбу
        // нужен второй выход, иначе стрелка «да» уходит в никуда.
        // Иначе раскладка рисует висящую ветку, и это отдельное правило
        // в iftop.rs — ровно та кейс-логика, от которой уходим.
        let mut no = Vec::new();
        if let Some(c) = alternative {
            self.branches_of(c, &mut no, labels);
        }
        branches.push(Branch {
            label: no_label.into(),
            stmts: no,
            to_end: false,
            link: None,
        });

        node.branches = branches;
        out.push(node);
    }

    /// Содержимое ветки: и `else if` (вложенное решение), и блок.
    fn branches_of(&self, n: Ts, out: &mut Vec<Stmt>, labels: &str) {
        // `else if (...)` — alternative это if_statement, но с меткой else
        if n.kind() == "else_clause" {
            for c in named_children(n) {
                self.branches_of(c, out, labels);
            }
            return;
        }
        match n.kind() {
            "if_statement" => {
                // вложенный if становится узлом внутри ветки
                let mut sub = Vec::new();
                self.if_stmt(n, &mut sub, labels);
                for nd in sub {
                    out.push(Stmt::Node(Box::new(nd)));
                }
            }
            "compound_statement" => {
                for c in named_children(n) {
                    let mut sub = Vec::new();
                    self.statement(c, &mut sub, labels);
                    out.extend(to_stmts(sub));
                }
            }
            _ => {
                let mut sub = Vec::new();
                self.statement(n, &mut sub, labels);
                out.extend(to_stmts(sub));
            }
        }
    }

    fn loop_stmt(&self, n: Ts, out: &mut Vec<Node>, labels: &str, kind: LoopKind) {
        let header = match kind {
            LoopKind::For => {
                let parts: Vec<String> = ["initializer", "condition", "update"]
                    .iter()
                    .map(|f| {
                        n.child_by_field_name(f)
                            .map(|c| flatten(text(c, self.src), self.src))
                            .unwrap_or_default()
                    })
                    .collect();
                parts.join("; ")
            }
            // у do-while условие лежит в теле: field condition у do_statement
            _ => n
                .child_by_field_name("condition")
                .map(|c| unparen(text(c, self.src)))
                .unwrap_or_default(),
        };

        let mut body = Vec::new();
        // у do_statement тело — первое поле body
        let body_node = n.child_by_field_name("body").or_else(|| {
            named_children(n)
                .into_iter()
                .find(|c| c.kind() == "compound_statement")
        });
        if let Some(b) = body_node {
            let mut sub = Vec::new();
            self.branches_of(b, &mut sub, labels);
            body = sub;
        }

        let mut nd = Node::new(NodeKind::Loop, header);
        nd.loop_kind = Some(kind);
        nd.body = Some(body);
        out.push(nd);
    }

    fn switch_stmt(&self, n: Ts, out: &mut Vec<Node>, labels: &str) {
        let cond = n
            .child_by_field_name("condition")
            .map(|c| unparen(text(c, self.src)))
            .unwrap_or_default();
        let var = cond.clone();

        let body = child_of_kind(n, "compound_statement");
        let mut branches: Vec<Branch> = Vec::new();
        if let Some(b) = body {
            for c in named_children(b) {
                if c.kind() != "case_statement" {
                    // оператор вне case: рисуем в последнюю ветку
                    let mut sub = Vec::new();
                    self.statement(c, &mut sub, labels);
                    if !sub.is_empty() {
                        if let Some(last) = branches.last_mut() {
                            last.stmts.extend(to_stmts(sub));
                        }
                    }
                    continue;
                }
                let label = case_label(c, self.src, &var);
                let mut stmts = Vec::new();
                // Тело case — всё, кроме значения метки. Значение лежит
                // в первом поле `value`, остальное — операторы.
                let value = c.child_by_field_name("value");
                for k in named_children(c) {
                    if Some(k) == value || k.kind() == "case" {
                        continue;
                    }
                    let mut sub = Vec::new();
                    self.branches_of(k, &mut sub, labels);
                    stmts.extend(sub);
                }
                branches.push(Branch {
                    label,
                    stmts,
                    to_end: false,
                    link: None,
                });
            }
        }

        let mut fixed = branches;
        merge_case_aliases(&mut fixed);

        let mut nd = Node::new(NodeKind::Decision, format!("switch ({cond})"));
        nd.switch_var = Some(var);
        nd.branches = fixed;
        out.push(nd);
    }
}

/// Метка кейса: `case 1` -> "k = 1", `default` -> "k = default".
/// Префикс переменной переносит IR в layout: метки разных case не
/// склеиваются в «1, 2» мимо значения k.
fn case_label(c: Ts, src: &str, var: &str) -> String {
    // Значение метки лежит в поле `value`; брать текст всего узла нельзя
    // — туда попадает тело кейса, и метка становилась «1: printf(...)».
    let raw = match c.child_by_field_name("value") {
        Some(v) => flatten(text(v, src), src),
        // `default:` значения не имеет
        None => "default".to_string(),
    };
    // диапазон `case 1 ... 5` берём левой границей
    let left = raw.split("...").next().unwrap_or(&raw).trim().to_string();
    if left.is_empty() || left == "default" {
        format!("{var} = default")
    } else {
        format!("{var} = {left}")
    }
}

/// Читаемый вид оператора для блока схемы: схлопнутые пробелы,
/// отбитые операторы, без хвостовой `;`.
///
/// Строковый литерал копируется ДОСЛОВНО. Он не оператор: `%lf%c`
/// внутри `"%lf%c"` разбивался на `% lf % c`, и формат на схеме
/// врал. Внутри литерала не трогаем ничего — включая экранированные
/// кавычки, которые иначе ломают счётчик.
fn flatten(s: &str, _src: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                // Литерал копируется дословно и операторов внутри не
                // ищем: `%lf%c` — это формат, а не `% lf % c`.
                out.push(c);
                copy_literal(&mut chars, &mut out, c);
            }
            _ if c.is_whitespace() => {
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
            }
            _ => push_op(&mut out, &mut chars, c),
        }
    }
    let t = out.trim();
    t.strip_suffix(';').unwrap_or(t).trim_end().to_string()
}

/// Оператор с отбитыми пробелами: `a=1` -> `a = 1`.
///
/// Двухзначные операторы остаются слитными: `!=`, `<=`, `>=`, `==`, `&&`,
/// `||`, `+=`, `++`, `--`. Раньше проверка исключала `!=` из пары и
/// разбивала его на `! =` — на схеме это читалось как отрицание
/// присваивания, то есть совсем другой смысл.
fn push_op(out: &mut String, chars: &mut std::iter::Peekable<std::str::Chars>, c: char) {
    const OPS: &str = "=+-*/<>%!&|^";
    if !OPS.contains(c) {
        out.push(c);
        return;
    }
    // Двухзначный оператор ПЕРВЫМ: иначе префиксная ветка ниже съест
    // `!` у `!=`, и на схеме получится `! =`.
    if let Some(&next) = chars.peek() {
        if OPS.contains(next) && (next == c || next == '=') {
            out.push(c);
            out.push(next);
            chars.next();
            return;
        }
    }
    // Префикс `!`, `*`, `&`, `%`: `!flag`, `*p`, `&x` — без пробела справа.
    if matches!(c, '!' | '&' | '*' | '%')
        && !out.ends_with(|p: char| p.is_alphanumeric() || p == '_')
    {
        out.push(c);
        return;
    }
    if out.ends_with(' ') {
        out.push(c);
        return;
    }
    out.push(' ');
    out.push(c);
    if chars.peek().is_some_and(|n| !n.is_whitespace()) {
        out.push(' ');
    }
}

/// Копирует тело литерала до закрывающей кавычки, сохраняя экранированные
/// символы. Незакрытый литерал (битый код) доходит до конца строки.
fn copy_literal(chars: &mut std::iter::Peekable<std::str::Chars>, out: &mut String, quote: char) {
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
            continue;
        }
        if c == quote {
            return;
        }
    }
}

fn unparen(s: &str) -> String {
    let t = s.trim();
    match t.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        Some(inner) => flatten(inner, ""),
        None => flatten(s, ""),
    }
}

/// Узлы вложенной области -> stmts. Обычный блок без ветвления — это
/// плитка, а не узел схемы: узлами становятся только if/цикл/switch.
///
/// break и continue — отдельные виды stmts, а не плитки: раскладка по
/// ним знает, что continue возвращает в начало цикла и мёртвый код
/// после него не рисуется. Плитка с текстом «continue» этого не даёт.
fn to_stmts(nodes: Vec<Node>) -> Vec<Stmt> {
    nodes
        .into_iter()
        .map(|n| match (&n.kind, n.text.as_str()) {
            (NodeKind::Act, "break") => Stmt::Break,
            (NodeKind::Act, "continue") => Stmt::Continue,
            // Плитка — всё, что раскладка рисует «просто блоком».
            // Ввод-вывод тоже: внутри case он плитка, а не отдельный
            // узел, иначе кейс рисуется как цикл.
            (NodeKind::Act, _) | (NodeKind::Io, _) => Stmt::Tile {
                kind: tile_kind(&n.text),
                text: n.text,
            },
            // return обязан быть Stmt::Return, а не узлом: раскладка
            // рисует его отдельной формой, а узел без ветвления уехал
            // в трапеции цикла.
            (NodeKind::Return, _) => Stmt::Return(n.text),
            _ => Stmt::Node(Box::new(n)),
        })
        .collect()
}

fn tile_node_kind(text: &str) -> NodeKind {
    NodeKind::from(tile_kind(text))
}

fn text<'a>(n: Ts, src: &'a str) -> &'a str {
    n.utf8_text(src.as_bytes()).unwrap_or("")
}

fn named_children(n: Ts) -> Vec<Ts> {
    let mut c = n.walk();
    let out: Vec<Ts> = n.named_children(&mut c).collect();
    out
}

/// Прямой ребёнок по виду, включая НЕименованные узлы грамматики.
/// `declarator` в tree-sitter-c неименованный: искать его среди
/// named_children нельзя, и `int main(void)` не находился вовсе.
fn child_of_kind<'t>(n: Ts<'t>, kind: &str) -> Option<Ts<'t>> {
    let mut c = n.walk();
    let out: Vec<Ts> = n.children(&mut c).filter(|c| c.kind() == kind).collect();
    out.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(src: &str) -> Vec<Node> {
        CParser::new().parse(src, "en")
    }

    fn body(src: &str) -> Vec<Node> {
        nodes(src)
            .into_iter()
            .filter(|n| n.kind != NodeKind::Term)
            .collect()
    }

    fn texts(ns: &[Node]) -> Vec<String> {
        ns.iter().map(|n| n.text.clone()).collect()
    }

    #[test]
    fn main_body_is_parsed() {
        let ns = body("int main(void) { a = 1; b = 2; }");
        assert_eq!(texts(&ns), vec!["a = 1", "b = 2"]);
    }

    /// Терминаторы — инвариант схемы: без них она по ГОСТ не схема.
    #[test]
    fn terminators_wrap_the_body() {
        let ns = nodes("int main(void) { a = 1; }");
        assert_eq!(ns.first().unwrap().kind, NodeKind::Term);
        assert_eq!(ns.first().unwrap().text, "Start");
        assert_eq!(ns.last().unwrap().kind, NodeKind::Term);
        assert_eq!(ns.last().unwrap().text, "End");
    }

    #[test]
    fn ru_terminators_and_labels() {
        let ns = CParser::new().parse("int main(void) { if (a) b = 1; }", "ru");
        assert_eq!(ns[0].text, "начало");
        assert_eq!(ns[ns.len() - 1].text, "конец");
        let d = ns.iter().find(|n| n.kind == NodeKind::Decision).unwrap();
        assert_eq!(d.branches[0].label, "да");
        assert_eq!(d.branches[1].label, "нет");
    }

    #[test]
    fn declaration_with_initializer_is_drawn() {
        let ns = body("int main(void) { int a = 5; }");
        assert_eq!(texts(&ns), vec!["a = 5"]);
    }

    /// `int a;` без значения на схеме ничего не делает — не рисуем.
    #[test]
    fn bare_declaration_is_skipped() {
        assert!(body("int main(void) { int a; }").is_empty());
    }

    #[test]
    fn printf_is_io_scanf_is_io() {
        let ns = body("int main(void) { printf(\"p\"); scanf(\"%d\", &a); }");
        assert!(
            ns.iter().all(|n| n.kind == NodeKind::Io),
            "{:?}",
            texts(&ns)
        );
    }

    #[test]
    fn if_without_else_has_two_branches() {
        let d = body("int main(void) { if (a > 0) printf(\"p\"); }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert_eq!(d.text, "if (a > 0)");
        assert_eq!(d.branches.len(), 2);
        assert_eq!(d.branches[0].label, "yes");
        assert_eq!(d.branches[1].label, "no");
        assert!(d.branches[1].stmts.is_empty());
    }

    /// Пустая ветка if — ровно тот случай, который раньше требовал
    /// отдельного правила в раскладке.
    #[test]
    fn if_else_both_empty() {
        let d = body("int main(void) { if (a); }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert_eq!(d.branches.len(), 2);
        assert!(d.branches[0].stmts.is_empty());
    }

    #[test]
    fn else_if_is_nested_decision() {
        let d = body("int main(void) { if (a < 0) printf(\"n\"); else if (a == 0) printf(\"z\"); else printf(\"p\"); }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert_eq!(d.branches[0].label, "yes");
        assert_eq!(d.branches[1].label, "no");
        let nested = match &d.branches[1].stmts[0] {
            Stmt::Node(n) if n.kind == NodeKind::Decision => n,
            other => panic!("ожидался вложенный ромб, получили {other:?}"),
        };
        assert_eq!(nested.branches.len(), 2);
    }

    #[test]
    fn while_keeps_condition() {
        let l = body("int main(void) { while (a > 0) { a = a - 1; } }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Loop)
            .unwrap();
        assert_eq!(l.loop_kind, Some(LoopKind::While));
        assert_eq!(l.text, "a > 0");
        assert_eq!(l.loop_label(), "while a > 0");
        assert!(!l.body.as_ref().unwrap().is_empty());
    }

    #[test]
    fn for_keeps_init_cond_step() {
        let l = body("int main(void) { for (i = 0; i < 10; i++) { x = 1; } }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Loop)
            .unwrap();
        assert_eq!(l.loop_kind, Some(LoopKind::For));
        let label = l.loop_label();
        assert!(label.starts_with("for "), "{label}");
        assert!(label.contains("i = 0"), "{label}");
        assert!(label.contains("i < 10"), "{label}");
    }

    #[test]
    fn do_while_is_distinct_kind() {
        let l = body("int main(void) { do { a = 1; } while (a < 7); }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Loop)
            .unwrap();
        assert_eq!(l.loop_kind, Some(LoopKind::DoWhile));
        assert_eq!(l.loop_label(), "do while a < 7");
    }

    #[test]
    fn switch_cases_become_branches() {
        let d = body("int main(void) { switch (a) { case 1: printf(\"1\"); break; case 2: printf(\"2\"); break; } }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert_eq!(d.text, "switch (a)");
        assert_eq!(d.switch_var.as_deref(), Some("a"));
        assert_eq!(d.branches.len(), 2, "{:?}", d.branches);
        assert_eq!(d.branches[0].label, "a = 1");
        assert_eq!(d.branches[1].label, "a = 2");
    }

    #[test]
    fn switch_default_branch() {
        let d = body("int main(void) { switch (a) { case 1: x = 1; break; default: x = 2; } }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert_eq!(
            d.branches.len(),
            2,
            "{:?}",
            d.branches.iter().map(|b| &b.label).collect::<Vec<_>>()
        );
        assert_eq!(d.branches[1].label, "a = default");
    }

    /// case без break проваливается в следующий: две метки, одно тело.
    #[test]
    fn switch_fallthrough_aliases() {
        let d = body("int main(void) { switch (a) { case 1: case 2: printf(\"x\"); break; } }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert_eq!(d.branches.len(), 1, "пустая ветка должна схлопнуться");
        assert_eq!(d.branches[0].label, "a = 1, 2");
    }

    /// break/continue лежат в ТЕЛЕ цикла, а не на верхнем уровне: в IR
    /// это stmts тела, а не отдельные узлы схемы.
    #[test]
    fn break_and_continue_are_tiles() {
        let l = body("int main(void) { while (a) { break; continue; } }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Loop)
            .unwrap();
        let body = l.body.unwrap();
        // break и continue — отдельные виды stmts: раскладка знает, что
        // continue возвращает в начало цикла. Плиткой они были бы
        // обычным текстом, и мёртвый код после continue рисовался бы.
        assert!(
            matches!(body.first(), Some(Stmt::Break)),
            "ожидался Break, тело: {:?}",
            body.len()
        );
        assert!(
            matches!(body.get(1), Some(Stmt::Continue)),
            "ожидался Continue, тело: {:?}",
            body.len()
        );
    }

    /// ГЛАВНОЕ: битый C рисуется, а не отказывает. Строгий парсер
    /// на этом коде завершался ошибкой и схемы не было вовсе.
    #[test]
    fn broken_syntax_still_renders() {
        for src in [
            "int main(void) { if { printf(\"x\"); }",
            "int main(void) { scanf(\"%d\"",
            "int main() { int a = ; return }",
            "int main(void) { ) ) )",
            "int main(void) { for (;;",
        ] {
            let ns = nodes(src);
            assert_eq!(
                ns.first().map(|n| n.kind.clone()),
                Some(NodeKind::Term),
                "{src}"
            );
            assert_eq!(
                ns.last().map(|n| n.kind.clone()),
                Some(NodeKind::Term),
                "{src}"
            );
        }
    }

    /// Мусор внутри тела не теряется молча: блоки рисуются.
    #[test]
    fn broken_body_produces_blocks() {
        let ns = body("int main(void) { a = 1; if { b = 2; } }");
        assert!(!ns.is_empty(), "битое тело дало пустую схему");
    }

    /// `for (;;` без закрытия: грамматика теряет function_definition
    /// целиком. Схема всё равно рисуется — пустой лист с «main не
    /// найден» был бесполезен.
    #[test]
    fn unterminated_loop_does_not_lose_main() {
        let ns = body("int main(void) { x = 1; for (;;");
        assert!(
            ns.iter().any(|n| n.text.contains("x = 1")),
            "теряется разобранное начало: {:?}",
            texts(&ns)
        );
    }

    /// Прерванное объявление `int a = ;` не должно обнулять схему.
    #[test]
    fn broken_declaration_does_not_lose_main() {
        let ns = body("int main() { scanf(\"%d\", &a); int b = ; return }");
        assert!(
            ns.iter().any(|n| n.text.contains("scanf")),
            "теряется scanf: {:?}",
            texts(&ns)
        );
    }

    /// scanf_s с лишним аргументом и &radius не теряются: это ровно тот
    /// случай, что ронял усечение в парсере.
    #[test]
    fn scanf_s_keeps_every_variable() {
        let ns = body("int main(void) { scanf_s(\"%lf%c\", &radius, &chek, 1); }");
        let t = &ns[0].text;
        assert!(t.contains("radius"), "{t}");
        assert!(t.contains("chek"), "{t}");
        assert!(t.contains("%lf"), "{t}");
    }

    /// Литерал с экранированной кавычкой переносится ДОСЛОВНО.
    /// `\"` внутри строки — экранированный знак, а не закрытие: раньше
    /// счётчик кавычек на нём сбивался, и `"\"xx..., %d"` терял хвост.
    #[test]
    fn escaped_quote_literal_preserved_verbatim() {
        let t = &body("int main(void) { printf(\"a\\\"b %d\", n); }")[0].text;
        assert_eq!(t, "printf(\"a\\\"b %d\", n)");
    }

    /// Текст ноды — это срез исходника, поэтому пробелы и операторы
    /// не склеиваются в нечитаемое `a =a *2;`.
    #[test]
    fn operators_are_spaced() {
        let t = body("int main(void) { a=a*2; }")[0].text.clone();
        assert_eq!(t, "a = a * 2");
    }

    /// Пробел внутри строкового литерала не трогаем.
    #[test]
    fn string_literal_content_preserved() {
        let t = body("int main(void) { printf(\"hello   world\"); }")[0]
            .text
            .clone();
        assert!(t.contains("hello   world"), "{t}");
    }

    #[test]
    fn preprocessor_and_comments_skipped() {
        let ns = body("#include <stdio.h>\n// комментарий\n/* блок */\nint main(void) { a = 1; }");
        assert_eq!(texts(&ns), vec!["a = 1"]);
    }

    /// Неизвестный вызов — обычный процесс, а не потерянная строка.
    #[test]
    fn unknown_call_becomes_tile() {
        let ns = body("int main(void) { custom_fn(x, y); }");
        assert!(ns[0].text.contains("custom_fn"), "{:?}", texts(&ns));
        assert_eq!(ns[0].kind, NodeKind::Act);
    }

    /// Файл без main не рисуется пустым молча: терминаторы на месте,
    /// тела нет — CLI об этом говорит.
    #[test]
    fn file_without_main_is_empty_body() {
        let ns = body("int helper(void) { return 1; }");
        assert!(ns.is_empty());
    }

    /// Глубокая вложенность не падает и не теряет блоки.
    #[test]
    fn deep_nesting_survives() {
        let mut src = String::from("int main(void) {");
        for _ in 0..12 {
            src.push_str("if (a) {");
        }
        src.push_str("x = 1;");
        for _ in 0..12 {
            src.push('}');
        }
        src.push('}');
        let nodes = nodes(&src);
        let mut depth = 0usize;
        let mut cur: Vec<Node> = nodes;
        while let Some(d) = cur.iter().find(|n| n.kind == NodeKind::Decision).cloned() {
            depth += 1;
            cur = d.branches[0]
                .stmts
                .iter()
                .filter_map(|s| match s {
                    Stmt::Node(n) if n.kind == NodeKind::Decision => Some((**n).clone()),
                    _ => None,
                })
                .collect();
        }
        assert!(depth >= 8, "вложенность потеряна: {depth}");
    }

    /// Обёртки выражений не плодят лишних блоков: `a[i] = b.c;` — это
    /// один блок, а не три. Раньше каждый вложенный узел становился
    /// своей плиткой, и `a = a - 1` рисовалось тремя блоками.
    #[test]
    fn expression_wrappers_do_not_multiply_blocks() {
        let ns = body("int main(void) { a[i] = b.c; }");
        assert_eq!(texts(&ns), vec!["a[i] = b.c"]);
    }

    /// Приведение типа и тернарный оператор — по-прежнему одна плитка.
    #[test]
    fn cast_and_ternary_single_tile() {
        let ns = body("int main(void) { x = (int)y; z = a ? b : c; }");
        assert_eq!(texts(&ns), vec!["x = (int)y", "z = a ? b : c"]);
    }

    /// Двухзначные операторы не разваливаются: `!= 1` на схеме читалось
    /// как `! = 1`, то есть как отрицание присваивания — другой смысл.
    #[test]
    fn two_char_operators_stay_joined() {
        for (src, want) in [
            ("x = a != 1;", "x = a != 1"),
            ("x = a <= 1;", "x = a <= 1"),
            ("x = a >= 1;", "x = a >= 1"),
            ("x = a == 1;", "x = a == 1"),
            ("x = a && b;", "x = a && b"),
            ("x = a || b;", "x = a || b"),
            ("a += 1;", "a += 1"),
            ("a++;", "a++"),
            ("a--;", "a--"),
            ("x = a * b;", "x = a * b"),
            ("x = a / b;", "x = a / b"),
        ] {
            let got = body(&format!("int main(void) {{ int a = 0, b = 0; {src} }}"))
                .pop()
                .unwrap_or_else(|| panic!("{src}: пустое тело"));
            assert_eq!(got.text, want, "{src}");
        }
        // `a * b;` без присваивания не проверяем: tree-sitter (и сам C)
        // читает это как объявление указателя, а не выражение.
    }

    /// Унарные не отбиваются пробелом: `!flag`, `*p`, `&x`.
    #[test]
    fn unary_operators_not_spaced() {
        assert_eq!(body("int main(void) { x = !flag; }")[0].text, "x = !flag");
    }

    /// return в ветке — отдельный терминатор, не узел без ветвления.
    /// Как узел он уезжал в трапеции цикла, и на схеме появлялась
    /// пара блоков вместо одного.
    #[test]
    fn return_inside_branch_is_return_stmt() {
        let d = body("int main(void) { if (a) return 1; }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        match d.branches[0].stmts.first() {
            Some(Stmt::Return(t)) => assert_eq!(t, "return 1"),
            other => panic!("ожидался Stmt::Return, получили {other:?}"),
        }
    }

    /// return без значения сохраняет ключевое слово: голое число на
    /// схеме неотличимо от значения, ради которого ветка закрывается.
    #[test]
    fn bare_return_keeps_keyword() {
        let d = body("int main(void) { if (a) return; }")
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        match d.branches[0].stmts.first() {
            Some(Stmt::Return(t)) => assert_eq!(t, "return"),
            other => panic!("ожидался Stmt::Return, получили {other:?}"),
        }
    }

    /// Регрессия масштаба листа: условие в три строки печаталось как
    /// `if (scanf("%d", &month) ! = 1` / `|| month < 1 || month > 12)`.
    #[test]
    fn condition_keeps_operators_readable() {
        let src = "int main(void) {\n    int month;\n    if (scanf(\"%d\", &month) != 1 || month < 1) {\n        a = 1;\n    }\n}\n";
        let d = body(src)
            .into_iter()
            .find(|n| n.kind == NodeKind::Decision)
            .unwrap();
        assert!(d.text.contains("!= 1"), "оператор разорван: {}", d.text);
        assert!(!d.text.contains("! ="), "оператор разорван: {}", d.text);
    }
}
