//! Фронтенд C99 -> FlowIR, замена pycparser (gostpadi.py c_to_gvn).
//! Грамматику читает lang-c; препроцессор (комментарии и директивы)
//! снимается сам — с сохранением нумерации строк, как в Python-версии.

use crate::error::ParseError;
use crate::frontend::tile_kind;
use crate::ir::{Branch, LoopKind, Node, NodeKind, Stmt};
use lang_c::ast::*;
use lang_c::driver::{parse_preprocessed, Config, Flavor, SyntaxError};
use lang_c::span::Node as LangNode;

/// Код C -> узлы схемы (без Start/End, их добавляет layout).
/// labels: "en" -> yes/no, "ru" -> да/нет.
pub fn parse_c_to_nodes(src: &str, labels: &str) -> Result<Vec<Node>, ParseError> {
    let prepared = prepare(src);
    let config = Config {
        cpp_command: String::new(),
        cpp_options: Vec::new(),
        flavor: Flavor::StdC11,
    };
    let ast = match parse_preprocessed(&config, prepared.clone()) {
        Ok(p) => p,
        Err(e) => return Err(syntax_err(e, src)),
    };
    let main = ast.unit.0.iter().find_map(|ext| match &ext.node {
        ExternalDeclaration::FunctionDefinition(f) => {
            let name = declarator_name(&f.node.declarator)?;
            if name == "main" {
                Some(f)
            } else {
                None
            }
        }
        _ => None,
    });
    let main = match main {
        Some(f) => f,
        None => return Err(ParseError::new("в коде не найден int main(...)")),
    };
    let body = match &main.node.statement.node {
        Statement::Compound(items) => items,
        _ => return Err(ParseError::new("тело main должно быть блоком в скобках")),
    };
    let ctx = Ctx {
        src: &prepared,
        orig: src,
        labels,
    };
    let stmts = ctx.items(body)?;
    // Терминаторы ставит фронтенд, а не раскладка: без них схема
    // лишена «начала» и «конца», и по ГОСТ она не схема. Раньше их
    // добавлял удалённый парсер `.gvn`.
    let (start_txt, end_txt) = match labels {
        "ru" => ("начало", "конец"),
        _ => ("Start", "End"),
    };
    let mut nodes = vec![Node::new(NodeKind::Term, start_txt)];
    for s in stmts {
        match s {
            // верхнеуровневый return не рисуем: терминатор «конец» и так завершает схему
            Stmt::Return(_) => {}
            Stmt::Tile { kind, text } => nodes.push(Node::new(NodeKind::from(kind), text)),
            Stmt::Break => nodes.push(Node::new(NodeKind::Act, "break")),
            Stmt::Continue => nodes.push(Node::new(NodeKind::Act, "continue")),
            Stmt::Node(n) => nodes.push(*n),
        }
    }
    nodes.push(Node::new(NodeKind::Term, end_txt));
    Ok(nodes)
}

// (здесь был gvn-писатель: C -> узлы -> текст -> gvn::parse -> узлы.
// Round-trip сериализации удалён вместе с форматом .gvn: узлы из
// parse_c_to_nodes идут в layout напрямую, без потери контекста.)

struct Ctx<'a> {
    /// подготовленный исходник (spans lang-c указывают в него)
    src: &'a str,
    /// исходник для .locate у ошибок
    orig: &'a str,
    labels: &'a str,
}

impl<'a> Ctx<'a> {
    fn err(&self, msg: impl Into<String>, off: usize) -> ParseError {
        ParseError::new(msg)
            .with_line(line_of(self.src, off))
            .with_col(col_of(self.src, off))
            .locate(self.orig)
    }

    fn items(&self, block: &[LangNode<BlockItem>]) -> Result<Vec<Stmt>, ParseError> {
        let mut out = Vec::new();
        for it in block {
            match &it.node {
                BlockItem::Declaration(d) => out.extend(self.decl(d)),
                BlockItem::StaticAssert(_) => {}
                BlockItem::Statement(s) => out.extend(self.stmt(s)?),
            }
        }
        Ok(out)
    }

    /// тело ветки/цикла: блок в скобках или одна инструкция без них
    fn block_stmt(&self, st: &LangNode<Statement>) -> Result<Vec<Stmt>, ParseError> {
        match &st.node {
            Statement::Compound(items) => self.items(items),
            _ => self.stmt(st),
        }
    }

    /// объявление без значения не рисуем; с ним — как присваивание
    fn decl(&self, d: &LangNode<Declaration>) -> Vec<Stmt> {
        d.node
            .declarators
            .iter()
            .filter_map(|idt| {
                let name = declarator_name(&idt.node.declarator)?;
                let init = idt.node.initializer.as_ref()?;
                let text = abbrev_stmt(&format!("{} = {}", name, self.init_text(init)));
                Some(Stmt::Tile {
                    kind: tile_kind(&text),
                    text,
                })
            })
            .collect()
    }

    fn init_text(&self, init: &LangNode<Initializer>) -> String {
        match &init.node {
            Initializer::Expression(e) => expr_text(self.src, e),
            Initializer::List(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|it| self.init_text(&it.node.initializer))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
        }
    }

    fn stmt(&self, st: &LangNode<Statement>) -> Result<Vec<Stmt>, ParseError> {
        match &st.node {
            Statement::If(i) => Ok(vec![Stmt::Node(Box::new(self.if_node(i)?))]),
            Statement::Switch(s) => Ok(vec![Stmt::Node(Box::new(self.switch_node(s)?))]),
            Statement::While(w) => {
                let mut nd = Node::new(
                    NodeKind::Loop,
                    shorten_calls(&expr_text(self.src, &w.node.expression)),
                );
                nd.loop_kind = Some(LoopKind::While);
                nd.body = Some(self.block_stmt(&w.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::For(f) => {
                let init = match &f.node.initializer.node {
                    ForInitializer::Empty | ForInitializer::StaticAssert(_) => String::new(),
                    ForInitializer::Expression(e) => expr_text(self.src, e),
                    ForInitializer::Declaration(d) => {
                        // тип счётчика в заголовке не нужен
                        d.node
                            .declarators
                            .iter()
                            .filter_map(|idt| {
                                let name = declarator_name(&idt.node.declarator)?;
                                Some(match &idt.node.initializer {
                                    Some(init) => {
                                        format!("{} = {}", name, self.init_text(init))
                                    }
                                    None => name,
                                })
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                };
                let cond = f
                    .node
                    .condition
                    .as_ref()
                    .map(|e| shorten_calls(&expr_text(self.src, e)))
                    .unwrap_or_default();
                let step = f
                    .node
                    .step
                    .as_ref()
                    .map(|e| expr_text(self.src, e))
                    .unwrap_or_default();
                let mut nd = Node::new(NodeKind::Loop, format!("{}; {}; {}", init, cond, step));
                nd.loop_kind = Some(LoopKind::For);
                nd.body = Some(self.block_stmt(&f.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::DoWhile(d) => {
                // тело выполняется до проверки условия, поэтому условие
                // уходит в заголовок трапеции с пометкой «do while», а
                // тело — в её содержимое: рисуется тем же циклом, что и
                // остальные, но подпись видна на схеме
                let mut nd = Node::new(
                    NodeKind::Loop,
                    shorten_calls(&expr_text(self.src, &d.node.expression)),
                );
                nd.loop_kind = Some(LoopKind::DoWhile);
                nd.body = Some(self.block_stmt(&d.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::Goto(g) => Err(self.err("в коде goto — не поддерживается", g.span.start)),
            Statement::Return(e) => Ok(vec![Stmt::Return(match e {
                Some(x) => format!("return {}", expr_text(self.src, x)),
                None => "return".to_string(),
            })]),
            Statement::Break => Ok(vec![Stmt::Break]),
            Statement::Continue => Ok(vec![Stmt::Continue]),
            Statement::Compound(items) => self.items(items),
            Statement::Expression(Some(e)) => {
                let text = expr_text(self.src, e);
                Ok(if text.is_empty() {
                    vec![]
                } else {
                    let text = abbrev_stmt(&text);
                    vec![Stmt::Tile {
                        kind: tile_kind(&text),
                        text,
                    }]
                })
            }
            Statement::Expression(None) => Ok(vec![]),
            Statement::Labeled(l) => self.labeled(l),
            Statement::Asm(_) => Err(self.err("inline asm не поддерживается", st.span.start)),
        }
    }

    fn labeled(&self, l: &LangNode<LabeledStatement>) -> Result<Vec<Stmt>, ParseError> {
        match &l.node.label.node {
            Label::Identifier(id) => {
                let text = match &l.node.statement.node {
                    Statement::Expression(Some(e)) => {
                        format!("{}: {}", id.node.name, expr_text(self.src, e))
                    }
                    Statement::Expression(None) => format!("{}:", id.node.name),
                    _ => return Err(self.err("метка перед блоком не поддерживается", l.span.start)),
                };
                Ok(vec![Stmt::Tile {
                    kind: tile_kind(&text),
                    text,
                }])
            }
            Label::Case(_) | Label::CaseRange(_) | Label::Default => {
                Err(self.err("метка case вне switch", l.span.start))
            }
        }
    }

    fn if_node(&self, i: &LangNode<IfStatement>) -> Result<Node, ParseError> {
        let (yes_w, no_w) = lang_tags(self.labels);
        let mut nd = Node::new(
            NodeKind::Decision,
            format!(
                "if ({})",
                shorten_calls(&expr_text(self.src, i.node.condition.as_ref()))
            ),
        );
        let no_stmts = match &i.node.else_statement {
            Some(e) => self.block_stmt(e)?,
            None => Vec::new(),
        };
        nd.branches = vec![
            Branch {
                label: yes_w.to_string(),
                stmts: self.block_stmt(i.node.then_statement.as_ref())?,
                to_end: false,
                link: None,
            },
            Branch {
                label: no_w.to_string(),
                stmts: no_stmts,
                to_end: false,
                link: None,
            },
        ];
        Ok(nd)
    }

    fn switch_node(&self, s: &LangNode<SwitchStatement>) -> Result<Node, ParseError> {
        let var = expr_text(self.src, &s.node.expression);
        let mut nd = Node::new(NodeKind::Decision, format!("switch ({})", var));
        nd.switch_var = Some(var.clone());
        let items = match &s.node.statement.node {
            Statement::Compound(items) => items,
            _ => return Err(self.err("тело switch должно быть блоком в скобках", s.span.start)),
        };
        let mut branches: Vec<Branch> = Vec::new();
        for it in items {
            match &it.node {
                BlockItem::Statement(st) => self.switch_item(st, &var, &mut branches, s)?,
                BlockItem::Declaration(d) => {
                    let stmts = self.decl(d);
                    if stmts.is_empty() {
                        continue;
                    }
                    switch_extend_last(&mut branches, stmts, || {
                        self.err("объявление в switch до первой метки case", s.span.start)
                    })?;
                }
                BlockItem::StaticAssert(_) => {}
            }
        }
        if branches.is_empty() {
            return Err(self.err("в switch нет веток case", s.span.start));
        }
        // case 1: case 2: body — «case 1» алиас, метки склеиваются
        super::merge_case_aliases(&mut branches);
        nd.branches = branches;
        Ok(nd)
    }

    fn switch_item(
        &self,
        st: &LangNode<Statement>,
        var: &str,
        branches: &mut Vec<Branch>,
        s: &LangNode<SwitchStatement>,
    ) -> Result<(), ParseError> {
        // Vec, а не срез: switch_item дописывает новые ветки, а не только
        // продлевает последнюю — срез не даст push.
        if !matches!(&st.node, Statement::Labeled(_)) {
            let stmts = self.stmt(st)?;
            return switch_extend_last(branches, stmts, || {
                self.err("в switch инструкция до первой метки case", s.span.start)
            });
        }
        // цепочка меток case 1: case 2: тело — последнее значение получает
        // тело, предыдущие остаются пустыми ветками (алиасами)
        let mut labs: Vec<String> = Vec::new();
        let mut cur = st;
        while let Statement::Labeled(l) = &cur.node {
            let chained = match &l.node.label.node {
                Label::Case(e) => {
                    labs.push(format!("{} = {}", var, expr_text(self.src, e.as_ref())));
                    true
                }
                Label::Default => {
                    labs.push("default".to_string());
                    true
                }
                Label::CaseRange(_) => {
                    return Err(self.err("диапазон case «a ... b» не поддерживается", l.span.start));
                }
                Label::Identifier(_) => false,
            };
            if !chained {
                break;
            }
            cur = l.node.statement.as_ref();
        }
        if labs.is_empty() {
            // goto-метка внутри switch — обычная инструкция
            let stmts = self.stmt(st)?;
            return switch_extend_last(branches, stmts, || {
                self.err("в switch инструкция до первой метки case", s.span.start)
            });
        }
        let last = labs.pop().unwrap_or_default();
        for lab in labs {
            branches.push(Branch {
                label: lab,
                stmts: Vec::new(),
                to_end: false,
                link: None,
            });
        }
        let stmts = self.stmt(cur)?;
        branches.push(Branch {
            label: last,
            stmts,
            to_end: false,
            link: None,
        });
        Ok(())
    }
}

/// Продлить содержимое последней ветки switch; пустой список — ошибка.
/// Срез здесь и достаточен: функция ничего не добавляет.
fn switch_extend_last(
    branches: &mut [Branch],
    stmts: Vec<Stmt>,
    err: impl FnOnce() -> ParseError,
) -> Result<(), ParseError> {
    match branches.last_mut() {
        Some(b) => {
            b.stmts.extend(stmts);
            Ok(())
        }
        None => Err(err()),
    }
}

fn lang_tags(labels: &str) -> (&'static str, &'static str) {
    if labels == "ru" {
        ("да", "нет")
    } else {
        ("yes", "no")
    }
}

fn declarator_name(d: &LangNode<Declarator>) -> Option<String> {
    match &d.node.kind.node {
        DeclaratorKind::Identifier(id) => Some(id.node.name.clone()),
        DeclaratorKind::Declarator(inner) => declarator_name(inner),
        DeclaratorKind::Abstract => None,
    }
}

/// Забивает /* */ и // пробелами, строковые литералы не трогает.
/// Комментарий заменяется пробелами той же длины: номера строк и
/// столбцов ошибок остаются координатами исходника.
fn strip_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut quote: Option<char> = None;
    while i < n {
        let c = chars[i];
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' && i + 1 < n {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < n && (chars[i + 1] == '/' || chars[i + 1] == '*') {
            let j = if chars[i + 1] == '/' {
                chars[i..]
                    .iter()
                    .position(|&x| x == '\n')
                    .map_or(n, |p| i + p)
            } else {
                let mut k = i + 2;
                while k + 1 < n && !(chars[k] == '*' && chars[k + 1] == '/') {
                    k += 1;
                }
                if k + 1 < n {
                    k + 2
                } else {
                    n
                }
            };
            for c in &chars[i..j] {
                // \n сохраняем: иначе номера строк ошибок съезжают
                out.push(if *c == '\n' { '\n' } else { ' ' });
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// pyconverter-подготовка: снять комментарии и строки препроцессора.
/// Директивы заменяются пустыми строками — нумерация строк сохраняется.
fn prepare(src: &str) -> String {
    let src = src.trim_start_matches('\u{feff}');
    strip_comments(src)
        .lines()
        .map(|l| {
            if l.trim_start().starts_with('#') {
                String::new()
            } else {
                expand_stdbool(l)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Вшитый контракт <stdbool.h> — те же три объектных макроса, что
/// объявляет заголовок у GCC/Clang и фейковая libc pycparser
/// (питоновская версия gostpadi опиралась именно на неё). lang-c
/// препроцессор не запускает, поэтому тип `bool` для него неизвестен:
/// https://github.com/sehaxe/gostpadi — репорт: файл с `bool keep = true;`
/// падал с «неожидаемый токен». Подстановка по границам слова, строковые
/// и символьные литералы не трогаем. Функциональные макросы и #if из
/// пользовательского кода не исполняются — осознанная граница фазы.
const STDBOOL: [(&str, &str); 3] = [("bool", "_Bool"), ("true", "1"), ("false", "0")];

fn expand_stdbool(line: &str) -> String {
    if line.trim_start().starts_with('#') {
        return line.to_string();
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 8);
    let mut in_str = false; // "..."
    let mut in_chr = false; // '...'
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (in_str || in_chr) && c == '\\' {
            out.push(c);
            if let Some(&next) = chars.get(i + 1) {
                out.push(next);
                i += 1;
            }
        } else if c == '"' && !in_chr {
            in_str = !in_str;
            out.push(c);
        } else if c == '\'' && !in_str {
            in_chr = !in_chr;
            out.push(c);
        } else if !in_str && !in_chr && (c.is_ascii_alphabetic() || c == '_') {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match STDBOOL.iter().find(|(w, _)| *w == word) {
                Some((_, rep)) => out.push_str(rep),
                None => out.push_str(&word),
            }
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

fn line_of(src: &str, off: usize) -> usize {
    src[..off.min(src.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

fn col_of(src: &str, off: usize) -> usize {
    src[..off.min(src.len())]
        .chars()
        .rev()
        .take_while(|&c| c != '\n')
        .count()
        + 1
}

/// Узел AST -> однострочный текст C: срез исходника по span,
/// внутренние скобки и переносы сохраняются как написал автор.
/// AST -> однострочный текст C.
///
/// Пробелы схлопываются, а вокруг операторов приводятся к одному с
/// каждой стороны. Автор может написать `a= a` или `a =a`, и на схеме
/// это читалось как опечатка в программе, а не как авторская запись.
/// Схлопывание не трогает строковые литералы: они приходят из исходника
/// как есть, а `split_whitespace` по ним не ходит.
fn expr_text(src: &str, e: &LangNode<Expression>) -> String {
    let s = src.get(e.span.start..e.span.end).unwrap_or_default();
    space_operators(&s.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Пробел вокруг бинарных операторов и после запятой. Сравнения,
/// присваивания и арифметика на блок-схеме читаются глазом, а `a<a`
/// или `x,y` сливаются в одно слово.
///
/// Пробел ставится с ОБЕИХ сторон: слева, если перед оператором уже есть
/// операнд, и справа всегда. Односторонний пробел давал `i- =1` и
/// `p- >x` вместо `i -= 1` и `p->x`.
///
/// Не трогаем: `++`/`--` (инкремент), `->` (разыменование), унарный знак,
/// содержимое строковых литералов.
/// Пробел вокруг бинарных операторов и после запятой.
///
/// Разбор идёт «островами»: сначала копируется операнд, потом оператор,
/// и только между ними вставляется ровно один пробел. Проверка «а
/// операнд ли перед оператором» делается по последнему непробельному
/// символу накопленного — иначе `a =b` путал оператор с пробелом и
/// оставлял `a =b` как есть.
///
/// Не трогаем `++`/`--`, `->` и унарный знак: они не разрываются.
fn space_operators(s: &str) -> String {
    const BINARY: [char; 12] = ['=', '<', '>', '+', '-', '*', '/', '%', '&', '|', '^', '!'];
    const COMPOUND: [&str; 14] = [
        "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "->", "++", "--",
    ];
    let c: Vec<char> = s.chars().collect();
    let n = c.len();
    let mut out = String::with_capacity(n + 8);
    let mut i = 0;
    while i < n {
        let ch = c[i];
        if ch == ' ' {
            i += 1;
            continue;
        }
        if ch == ',' {
            // Запятая в C всегда разделяет аргументы, значит пробел после
            // неё обязателен. lang-c иногда склеивает их ещё в span
            // (`&m,&d`), поэтому пробел ставим безусловно, а не только
            // если справа уже был.
            while out.ends_with(' ') {
                out.pop();
            }
            out.push_str(", ");
            i += 1;
            continue;
        }
        // строковый литерал целиком: пробелы внутри — часть текста
        if ch == '"' {
            out.push(ch);
            i += 1;
            while i < n {
                out.push(c[i]);
                if c[i] == '\\' && i + 1 < n {
                    out.push(c[i + 1]);
                    i += 2;
                    continue;
                }
                if c[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let two: String = c[i..(i + 2).min(n)].iter().collect();
        if COMPOUND.contains(&two.as_str()) {
            // `++`/`--`/`->` приклеены к операнду, остальные — нет
            let tight = two == "++" || two == "--" || two == "->";
            if !tight {
                out.push(' ');
            }
            out.push_str(&two);
            if !tight {
                out.push(' ');
            }
            i += 2;
            continue;
        }
        if BINARY.contains(&ch) {
            // Перед оператором должен стоять операнд — иначе это унарный
            // знак (`&x`, `-1`, `!ok`), и он приклеивается к операнду.
            let last = out.chars().rev().find(|p| !p.is_whitespace());
            let binary = last.is_some_and(|p| {
                p.is_alphanumeric() || p == '_' || p == ')' || p == ']' || p == '}'
            });
            if binary {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(' ');
                out.push(ch);
                out.push(' ');
            } else {
                out.push(ch);
            }
            i += 1;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out.trim_end().to_string()
}

/// Сокращение середины: голова и хвост остаются, режется середина.
///
/// Так сокращённый текст остаётся РАЗЛИЧИМЫМ между ветками. Раньше
/// усечение всегда отбрасывало хвост, и четыре кейса
/// `printf("Введите номер месяца (1-12): ")` … `(13-24)` … `(25-36)` …
/// «неверно» давали на схеме четыре одинаковых `printf("Введите номер...")`
/// — различить их было нечем. Хвост как раз и нёс различие.
fn ellipsize(s: &str, budget: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= budget || budget < 5 {
        return s.to_string();
    }
    let head = (budget - 3) / 2;
    let mut tail = budget - 3 - head;
    // Грань хвоста не должна попадать внутрь слова: `...13-24)` без
    // открывающей скобки читается хуже, чем `...(13-24)`. Если начало
    // хвоста оказалось внутри токена, сдвигаем его назад до границы.
    while c[c.len() - tail].is_alphanumeric() {
        let prev = c.len() - tail - 1;
        if prev == 0 || c[prev].is_whitespace() {
            break;
        }
        tail += 1;
    }
    let mut out: String = c[..head].iter().collect();
    while out.ends_with(char::is_whitespace) {
        out.pop();
    }
    out.push_str("...");
    out.extend(c[c.len() - tail..].iter());
    out
}

/// Список аргументов вызова в условии: умещаем в `BUDGET`, ни одной
/// переменной не выбрасывая.
///
/// Что режется — решает ХВОСТ, а не голова. У `scanf` хвост это адреса
/// (`&radius, &chek, 1`): смысл там, голова — форматный шум, и режется
/// он целиком. У `vvedi("Vvedite chislo A: ", &a)` смысл в строке, и три
/// вызова с разными подсказками обязаны остаться разными — иначе схема
/// не говорит, что увидит пользователь (инвариант коммита 0c6cc78).
///
/// Старый `ellipsize` резал середину и ронял переменную:
/// `scanf_s("%lf%c", &radius, &chek, 1)` -> `scanf_s("%lf%c",...&chek, 1)`,
/// то есть по схеме читалось, будто `radius` не читается. Умолчание о
/// выброшенной переменной хуже переноса: перенос читается, а потеря
/// переменной — нет.
/// Строковый литерал ужимается ВНУТРИ кавычек. `ellipsize` режет
/// середину строки, и на литерале `"\"xxxxxxx\", %d"` он давал
/// `"\"xx..., %d"` — нечётное число кавычек, оборванный литерал и
/// потерянный следом аргумент. Тот же приём, что в `abbrev_stmt`.
fn shorten_literal(lit: &str, budget: usize) -> String {
    let Some(inner) = lit.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return ellipsize(lit, budget);
    };
    if inner.chars().count() <= budget {
        return lit.to_string();
    }
    format!("\"{}\"", ellipsize(inner, budget))
}

fn shorten_args(args: &str) -> String {
    const BUDGET: usize = 20;
    if args.chars().count() <= BUDGET {
        return args.to_string();
    }
    // конец литерала ищем по символам, а не поиском подстроки: внутри
    // формата встречается экранированная кавычка (`"%d\"x", %d"`), и
    // `find("\", ")` цеплялся за неё — на схему попадал обрывок
    // формата вроде `%d",`. Тот же обход, что в call_close.
    let chars: Vec<char> = args.chars().collect();
    let mut i = 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => break,
            _ => i += 1,
        }
    }
    // не строковый первый аргумент — нечего определять: режем середину
    if i >= chars.len() || chars.get(i + 1) != Some(&',') {
        return ellipsize(args, BUDGET);
    }
    let head: String = chars[..=i].iter().collect();
    let tail: String = chars[i + 2..].iter().collect();
    let tail = tail.trim_start();
    // хвост влезает в бюджет — ужимаем строку и оставляем хвост целиком.
    // `, ` между ними занимает два символа, не один.
    let room = BUDGET.saturating_sub(tail.chars().count() + 2);
    if room >= 5 {
        return format!("{}, {tail}", shorten_literal(&head, room));
    }
    // хвост сам не влезает: адреса и есть смысл, строка уходит целиком.
    // ponytail: длинный список переменных раздувает блок и жмёт лист —
    // это видно и честно; молча выбросить переменную нельзя.
    format!("... {tail}")
}

fn shorten_calls(cond: &str) -> String {
    let chars: Vec<char> = cond.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(cond.len());
    let mut i = 0;
    while i < n {
        if chars[i].is_ascii_alphanumeric() || chars[i] == '_' {
            let start = i;
            while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            if i + 1 < n && chars[i] == '(' && chars[i + 1] == '"' {
                if let Some(close) = call_close(&chars, i) {
                    let args: String = chars[i + 1..close].iter().collect();
                    out.push_str(&name);
                    out.push('(');
                    out.push_str(&shorten_args(&args));
                    out.push(')');
                    i = close + 1;
                    continue;
                }
            }
            out.push_str(&name);
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn call_close(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut k = open;
    while k < chars.len() {
        let c = chars[k];
        if in_str {
            if c == '\\' {
                k += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
        } else {
            match c {
                '"' => in_str = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(k);
                    }
                }
                _ => {}
            }
        }
        k += 1;
    }
    None
}

/// Длинные printf("…") сокращаем, СОХРАНЯЯ хвост строки: различия
/// между кейсами обычно в конце («(1-12)» против «(13-24)»), а резать
/// надо середину. Бюджет 19 символов на содержимое — столько же, сколько
/// занимало прежнее усечение до 15 символов плюс «...».
fn abbrev_stmt(s: &str) -> String {
    const CONTENT_BUDGET: usize = 19;
    // аргументы после строки сохраняем: printf("%d", n) короче бюджета
    // и должно остаться целиком. Ведущая запятая — часть разделителя
    // аргументов, а формат ниже ставит свою: без trim_start_matches(',')
    // печать давала `printf("...", , n)`.
    let args = |q: usize| -> String {
        s[q + 1..s.len() - 1]
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_string()
    };
    // суффиксные варианты MSVC (`printf_s`) — тот же вывод, значит и то же
    // сокращение: без них длинная строка ввода/вывода занимала две строки
    // плитки и раздувала лист ниже читаемого кегля
    for w in ["printf", "printf_s", "puts", "print", "echo", "write"] {
        let Some(rest) = s.strip_prefix(w) else {
            continue;
        };
        let Some(after_open) = rest.strip_prefix("(\"") else {
            continue;
        };
        if !s.ends_with(')') {
            continue;
        }
        let content_start = s.len() - after_open.len();
        let Some(q) = s.rfind('"') else {
            continue;
        };
        if q < content_start {
            continue;
        }
        let tail = &s[q + 1..s.len() - 1];
        let tail = tail.trim();
        if !tail.is_empty() && !tail.starts_with(',') {
            continue;
        }
        let content = &s[content_start..q];
        if content.chars().count() <= CONTENT_BUDGET {
            return s.to_string();
        }
        let a = args(q);
        return if a.is_empty() {
            format!("{w}(\"{}\")", ellipsize(content, CONTENT_BUDGET))
        } else {
            format!("{w}(\"{}\", {})", ellipsize(content, CONTENT_BUDGET), a)
        };
    }
    s.to_string()
}

fn syntax_err(e: SyntaxError, orig: &str) -> ParseError {
    let mut exp: Vec<&str> = e.expected.iter().copied().collect();
    exp.sort_unstable();
    let list: Vec<String> = exp.iter().map(|t| format!("'{}'", t)).collect();
    ParseError::new(format!(
        "в коде C: неожидаемый токен, ожидалось: {}",
        list.join(", ")
    ))
    .with_line(e.line)
    .with_col(e.column)
    .locate(orig)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Узлы тела main БЕЗ терминаторов: тесты этого модуля проверяют
    /// разбор операторов, а не оформление схемы. Фронтенд добавляет
    /// «начало»/«конец» — инвариант схемы, проверяется в layout.
    fn parse_ok(src: &str) -> Vec<Node> {
        match parse_c_to_nodes(src, "en") {
            Ok(nodes) => nodes
                .into_iter()
                .filter(|n| n.kind != NodeKind::Term)
                .collect(),
            Err(e) => panic!("parse failed: {}", e),
        }
    }

    fn texts(nodes: &[Node], kind: NodeKind) -> Vec<String> {
        nodes
            .iter()
            .filter(|n| n.kind == kind)
            .map(|n| n.text.clone())
            .collect()
    }

    /// все плоские текстовые плитки, включая вложенные в ветки и циклы
    fn all_tiles(nodes: &[Node]) -> Vec<String> {
        fn push_inner<'a>(n: &'a Node, work: &mut Vec<&'a Stmt>) {
            for b in &n.branches {
                work.extend(b.stmts.iter());
            }
            if let Some(body) = &n.body {
                work.extend(body.iter());
            }
        }
        let mut out = Vec::new();
        let mut work: Vec<&Stmt> = Vec::new();
        for n in nodes {
            match n.kind {
                NodeKind::Decision | NodeKind::Loop => push_inner(n, &mut work),
                _ => out.push(n.text.clone()),
            }
        }
        while let Some(s) = work.pop() {
            match s {
                Stmt::Tile { text: t, .. } | Stmt::Return(t) => out.push(t.clone()),
                Stmt::Break => out.push("break".to_string()),
                Stmt::Continue => out.push("continue".to_string()),
                Stmt::Node(inner) => match inner.kind {
                    NodeKind::Decision | NodeKind::Loop => push_inner(inner, &mut work),
                    _ => out.push(inner.text.clone()),
                },
            }
        }
        out
    }

    #[test]
    fn seasons_main_c() {
        let src = include_str!("../../examples/main.c");
        let nodes = parse_ok(src);
        // ни один printf не потерян: верх + ветка if + 4 case = 6 вызовов
        let tiles = all_tiles(&nodes);
        let io: Vec<&String> = tiles.iter().filter(|t| t.starts_with("printf(")).collect();
        assert_eq!(io.len(), 6, "io tiles: {:?}", tiles);
        for word in ["Введите", "Ошибка", "Весна", "Лето", "Осень", "Зима"]
        {
            assert!(tiles.iter().any(|t| t.contains(word)), "lost: {}", word);
        }
        // длинный printf сокращён по _abbrev_stmt, но ХВОСТ сохранён
        assert!(
            tiles
                .iter()
                .any(|t| t.contains("Введите") && t.contains("1-12")),
            "хвост с диапазоном должен сохраниться: {:?}",
            tiles
        );
        // условие: формат scanf сокращён, переменная сохранена
        let dec = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Decision && n.switch_var.is_none())
            .unwrap();
        assert_eq!(
            dec.text,
            "if (scanf(\"%d\", &month) != 1 || month < 1 || month > 12)"
        );
        // return 1 в ветке присутствует как текст (тупик)
        assert_eq!(dec.branches[0].label, "yes");
        assert!(
            dec.branches[0]
                .stmts
                .iter()
                .any(|s| matches!(s, Stmt::Return(t) if t == "return 1")),
            "return 1 lost in yes branch"
        );
        // switch month / 3: 4 ветки
        let sw = nodes.iter().find(|n| n.switch_var.is_some()).unwrap();
        assert_eq!(sw.switch_var.as_deref(), Some("month / 3"));
        let labels: Vec<&str> = sw.branches.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["month / 3 = 1", "month / 3 = 2", "month / 3 = 3", "default"]
        );
        // верхнеуровневый return 0 не нарисован
        assert!(!tiles.iter().any(|t| t.contains("return 0")));
    }

    /// Метки case несут префикс переменной ровно один раз:
    /// «month / 3 = 1», а не «month / 3 = month / 3 = 1». Склейка
    /// алиасов (case 1: case 2:) этот префикс тоже не дублирует.
    #[test]
    fn switch_labels_single_prefix() {
        let src = include_str!("../../examples/main.c");
        let nodes = parse_ok(src);
        let sw = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Decision && n.switch_var.is_some())
            .unwrap();
        let labels: Vec<&str> = sw.branches.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["month / 3 = 1", "month / 3 = 2", "month / 3 = 3", "default"]
        );
    }

    /// Многострочный /* */ не должен съедать \n: иначе строки ошибок
    /// после комментария указывают выше реальных.
    #[test]
    fn multiline_comment_keeps_line_numbers() {
        let src = "int main(void) {\n/* comment\n   spanning\n   lines */ int = 5;\n}";
        let e = parse_c_to_nodes(src, "en").unwrap_err();
        assert_eq!(e.line, Some(4), "строка ошибки: {:?}", e.msg);
        let out = strip_comments(src);
        assert_eq!(out.matches('\n').count(), src.matches('\n').count());
    }

    #[test]
    fn declarations() {
        let src = "int main(void) {\n    int n = 5;\n    float a, b;\n    int arr[3] = {1, 2, 3};\n    printf(\"%d\", n);\n}";
        let nodes = parse_ok(src);
        let acts = texts(&nodes, NodeKind::Act);
        assert!(acts.contains(&"n = 5".to_string()), "{:?}", acts);
        assert!(acts.contains(&"arr = {1, 2, 3}".to_string()), "{:?}", acts);
        // float a, b — без init, не рисуем
        assert_eq!(acts.len(), 2, "{:?}", acts);
        assert_eq!(texts(&nodes, NodeKind::Io), vec!["printf(\"%d\", n)"]);
    }

    #[test]
    fn if_without_else() {
        let nodes = parse_ok("int main(void) { if (a > 0) printf(\"плюс\"); }");
        let dec = &nodes[0];
        assert_eq!(dec.kind, NodeKind::Decision);
        assert_eq!(dec.text, "if (a > 0)");
        assert_eq!(dec.branches.len(), 2);
        assert_eq!(dec.branches[0].label, "yes");
        assert_eq!(dec.branches[0].stmts.len(), 1);
        assert_eq!(dec.branches[1].label, "no");
        assert!(dec.branches[1].stmts.is_empty());
    }

    #[test]
    fn ru_labels() {
        let src = "int main(void) { if (a > 0) printf(\"1\"); }";
        let nodes = parse_c_to_nodes(src, "ru").unwrap();
        let dec = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Decision)
            .expect("ветвление не разобрано");
        assert_eq!(dec.branches[0].label, "да");
        assert_eq!(dec.branches[1].label, "нет");
    }

    #[test]
    fn else_if_chain() {
        let src = "int main(void) { if (a < 0) printf(\"neg\"); else if (a == 0) printf(\"zero\"); else printf(\"pos\"); }";
        let nodes = parse_ok(src);
        let d = &nodes[0];
        let nested = match &d.branches[1].stmts[0] {
            Stmt::Node(n) => n,
            other => panic!("expected nested decision, got {:?}", other),
        };
        assert_eq!(nested.text, "if (a == 0)");
        assert!(
            matches!(&nested.branches[1].stmts[0], Stmt::Tile { text: t, .. } if t.contains("pos")),
            "else branch lost"
        );
    }

    #[test]
    fn loops() {
        let src =
            "int main(void) { for (int i = 0; i < 5; i++) { s = s + i; } while (n > 0) { n--; } }";
        let nodes = parse_ok(src);
        assert_eq!(nodes.len(), 2);
        let f = &nodes[0];
        assert_eq!(f.kind, NodeKind::Loop);
        assert_eq!(f.loop_kind, Some(LoopKind::For));
        // i++ остаётся как есть, тип счётчика вырезан
        assert_eq!(f.text, "i = 0; i < 5; i++");
        assert!(
            matches!(f.body.as_deref().and_then(|b| b.first()), Some(Stmt::Tile { text: t, .. }) if t == "s = s + i"),
            "for body lost"
        );
        let w = &nodes[1];
        assert_eq!(w.loop_kind, Some(LoopKind::While));
        assert_eq!(w.text, "n > 0");
    }

    #[test]
    fn for_header_keeps_init_cond_step() {
        let nodes = parse_ok("int main(void) { for (int i = 0; i < 5; i = i + 1) { x = 1; } }");
        assert_eq!(nodes[0].text, "i = 0; i < 5; i = i + 1");
    }

    /// do-while рисуется как цикл: тело в содержимом трапеции,
    /// условие — в заголовке с пометкой «do while». Раньше он
    /// отвергался с требованием переписать на while, что меняло
    /// семантику: тело do выполняется до проверки.
    #[test]
    fn dowhile_is_a_loop_with_body_and_marked_header() {
        let nodes = parse_ok("int main(void) { do { x = x + 1; } while (x < 3); }");
        let loop_node = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Loop)
            .expect("do-while должен стать узлом-циклом");
        assert_eq!(loop_node.loop_kind, Some(LoopKind::DoWhile));
        let body = loop_node.body.as_ref().expect("тело do-while");
        assert!(
            body.iter()
                .any(|s| matches!(s, Stmt::Tile { text, .. } if text.contains("x = x + 1"))),
            "тело do-while должно попасть в содержимое трапеции: {body:?}"
        );
        // на схеме видно, что это именно do-while
        assert_eq!(loop_node.loop_label(), "do while x < 3");
    }

    /// Вид цикла виден в IR и в подписи на схеме: без этого цикл
    /// терял бы различие между while / for / do while.
    #[test]
    fn loop_kind_and_label_survive() {
        for (src, kind, label) in [
            (
                "int main(void){ do { x++; } while (x<3); }",
                LoopKind::DoWhile,
                "do while x < 3",
            ),
            (
                "int main(void){ while (x<3) x++; }",
                LoopKind::While,
                "while x < 3",
            ),
            (
                "int main(void){ for(int i=0;i<3;i++) x++; }",
                LoopKind::For,
                "for i = 0; i < 3; i++",
            ),
        ] {
            let nodes = parse_ok(src);
            let lp = nodes
                .iter()
                .find(|n| n.kind == NodeKind::Loop)
                .expect("цикл не разобран");
            assert_eq!(lp.loop_kind, Some(kind), "вид цикла потерян");
            assert_eq!(lp.loop_label(), label);
            assert!(lp.body.is_some(), "тело цикла потерялось");
        }
    }

    /// goto по-прежнему не поддерживается — в отличие от do-while,
    /// который рисовать можно честно.
    #[test]
    fn goto_still_rejected() {
        let e2 = parse_c_to_nodes("int main(void) {\n  goto end;\n  end: ;\n}", "en").unwrap_err();
        assert!(e2.msg.contains("goto"), "{}", e2.msg);
        assert_eq!(e2.line, Some(2));
    }

    #[test]
    fn switch_with_return_in_case() {
        let src = "int main(void) { switch (x) { case 1: printf(\"one\"); return 1; case 2: break; default: printf(\"other\"); } }";
        let nodes = parse_ok(src);
        let sw = &nodes[0];
        assert_eq!(sw.branches.len(), 3);
        let b1 = &sw.branches[0];
        assert_eq!(b1.label, "x = 1");
        assert_eq!(b1.stmts.len(), 2);
        assert!(matches!(&b1.stmts[1], Stmt::Return(t) if t == "return 1"));
        assert!(matches!(&sw.branches[1].stmts[0], Stmt::Break));
        assert_eq!(sw.branches[2].label, "default");
    }

    #[test]
    fn switch_case_fallthrough() {
        let src = "int main(void) { switch (x) { case 1: case 2: printf(\"low\"); break; case 3: printf(\"hi\"); break; } }";
        let nodes = parse_ok(src);
        let sw = &nodes[0];
        let labels: Vec<&str> = sw.branches.iter().map(|b| b.label.as_str()).collect();
        // case 1 — алиас case 2: метки склеены, пустой ветки нет
        assert_eq!(labels, vec!["x = 1, 2", "x = 3"]);
        assert!(sw.branches.iter().all(|b| !b.stmts.is_empty()));
    }

    /// Последняя пустая ветка (пустой case в конце switch) — не алиас:
    /// сливаться не с кем, остаётся рельсой обхода.
    #[test]
    fn switch_trailing_empty_case_stays() {
        let src = "int main(void) { switch (x) { case 1: printf(\"low\"); break; case 2: ; } }";
        let nodes = parse_ok(src);
        let sw = &nodes[0];
        let labels: Vec<&str> = sw.branches.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["x = 1", "x = 2"]);
        assert!(sw.branches[1].stmts.is_empty());
    }

    #[test]
    fn scanf_cond_shortened() {
        let nodes = parse_ok("int main(void) { if (scanf(\"%d\", &x)) printf(\"ok\"); }");
        // форматная строка — шум, переменная — нет
        assert_eq!(nodes[0].text, "if (scanf(\"%d\", &x))");
    }

    /// scanf с несколькими переменными: усечение условия не вправе
    /// ВЫБРОСИТЬ переменную. Раньше резалась середина списка аргументов,
    /// и `scanf_s("%lf%c", &radius, &chek, 1)` доходил до схемы как
    /// `scanf_s("%lf%c",...&chek, 1)` — по схеме выглядело, будто
    /// `radius` не читается. Умолчание хуже переноса.
    #[test]
    fn scanf_cond_keeps_every_variable() {
        let nodes = parse_ok(
            "int main(void) { if (scanf_s(\"%lf%c\", &radius, &chek, 1) != 2) return 1; }",
        );
        let cond = &nodes[0].text;
        assert!(cond.contains("&radius"), "потеряна &radius: {cond}");
        assert!(cond.contains("&chek"), "потеряна &chek: {cond}");
        assert!(!cond.contains("%lf"), "форматная строка — шум: {cond}");
    }

    /// printf_s — тот же вывод, что printf, значит и сокращается так же.
    /// Сверяем с printf на ОДИНАКОВОЙ строке: раньше printf_s не сокращался
    /// вовсе, и одинаковый вывод давал разные плитки.
    #[test]
    fn printf_s_is_abbreviated_like_printf() {
        let arg = "plashad shara : %.2f\\n";
        let io = |f: &str| {
            let src = format!("int main(void) {{ {f}(\"{arg}\", s); }}");
            texts(&parse_ok(&src), NodeKind::Io)
        };
        let (a, b) = (io("printf"), io("printf_s"));
        assert_eq!(a.len(), 1, "{a:?}");
        // различается только имя функции — сравниваем хвост плитки
        let tail = |s: &str| s[s.find('"').unwrap_or(0)..].to_string();
        assert_eq!(
            tail(&b[0]),
            tail(&a[0]),
            "printf_s сокращён иначе, чем printf"
        );
        assert!(b[0].contains("..."), "строка не сокращена: {b:?}");
    }

    /// Два вызова с РАЗНЫМИ строками обязаны остаться разными: сокращать
    /// строку до `...` нельзя. На этом стоит коммит 0c6cc78, и этим же
    /// ловится ошибка «схема не говорит, что увидит пользователь».
    #[test]
    fn different_prompt_strings_stay_distinguishable() {
        let src = "int main(void) {
            if (vvedi(\"Vvedite chislo A: \", &a) != 1) return 1;
            if (vvedi(\"Vvedite chislo B: \", &a) != 2) return 2;
        }";
        let conds: Vec<String> = parse_ok(src)
            .iter()
            .filter(|n| n.kind == NodeKind::Decision)
            .map(|n| n.text.clone())
            .collect();
        assert_eq!(conds.len(), 2, "{conds:?}");
        assert_ne!(conds[0], conds[1], "подсказки слились: {conds:?}");
    }

    /// Хвост аргументов не должен получить вторую запятую: разделитель
    /// аргументов и запятая из формата складывались в `printf("...", , n)`.
    #[test]
    fn no_double_comma_in_abbreviated_printf() {
        let nodes = parse_ok("int main(void) { printf(\"plashad shara : %.2f\\n\", s); }");
        let io = texts(&nodes, NodeKind::Io);
        assert!(!io[0].contains(", ,"), "двойная запятая: {io:?}");
        assert!(io[0].ends_with(", s)"), "аргумент потерян: {io:?}");
    }

    /// Экранированная кавычка внутри формата: конец литерала ищется по
    /// символам, а ужимается он ВНУТРИ кавычек. Обе ошибки рвали
    /// литерал: поиск подстроки `", ` цеплялся за `\"`, а `ellipsize`
    /// резал середину и давал нечётное число кавычек. Инвариант —
    /// кавычки сбалансированы и ни одна переменная не потеряна.
    #[test]
    fn escaped_quote_in_format_keeps_literal_balanced() {
        let nodes = parse_ok(
            "int main(void) { if (scanf(\"\\\"xxxxxxx\\\", %d\", &a, &b) != 2) return 1; }",
        );
        let cond = &nodes[0].text;
        assert_eq!(
            cond.matches('"').count() % 2,
            0,
            "литерал разорван, кавычек нечётно: {cond}"
        );
        assert!(
            cond.contains("&a") && cond.contains("&b"),
            "аргумент потерян: {cond}"
        );
    }

    #[test]
    fn preprocessor_and_comments() {
        let src = "#include <stdio.h>\n/* comment\n   spanning lines */ int main(void) {\n#define N 10\n    a = N; // tail\n    printf(\"%d\", a);\n}";
        let nodes = parse_ok(src);
        let acts = texts(&nodes, NodeKind::Act);
        assert!(acts.contains(&"a = N".to_string()), "{:?}", acts);
    }

    #[test]
    fn syntax_error_line() {
        let src = "int main(void) {\n    int = 5;\n}";
        let e = parse_c_to_nodes(src, "en").unwrap_err();
        assert_eq!(e.line, Some(2));
        assert!(e.msg.contains("в коде C"), "{}", e.msg);
        assert!(e.src.as_deref().unwrap_or("").contains("int = 5"));
    }

    #[test]
    fn nested_if_in_loop_body() {
        let src =
            "int main(void) { while (a < 5) { if (a % 2 == 0) { printf(\"even\"); } a = a + 1; } }";
        let nodes = parse_ok(src);
        let w = &nodes[0];
        let body = w.body.as_deref().unwrap_or_default();
        assert_eq!(body.len(), 2);
        let dec = match &body[0] {
            Stmt::Node(n) => n,
            other => panic!("expected decision in loop body, got {:?}", other),
        };
        assert_eq!(dec.text, "if (a % 2 == 0)");
    }

    /// Усечение сохраняет ХВОСТ строки: различия между кейсами обычно
    /// в конце («(1-12)» против «(13-24)»), а резать надо середину.
    /// Раньше четыре кейса давали четыре одинаковых
    /// `printf("Введите номер...")`.
    #[test]
    fn abbrev_long_printf_keeps_the_tail() {
        let cases = [
            "printf(\"Введите номер месяца (1-12): \")",
            "printf(\"Введите номер месяца (13-24): \")",
            "printf(\"Введите номер месяца (25-36): \")",
        ];
        let got: Vec<String> = cases.iter().map(|c| abbrev_stmt(c)).collect();
        for g in &got {
            println!("{g}");
        }
        // все три различимы: в каждом виден свой диапазон
        assert!(got[0].contains("(1-12)"), "{}", got[0]);
        assert!(got[1].contains("(13-24)"), "{}", got[1]);
        assert!(got[2].contains("(25-36)"), "{}", got[2]);
        // и ни одна не равна другой
        assert_ne!(got[0], got[1]);
        assert_ne!(got[1], got[2]);
        // хвост не режется посреди числа: скобка на месте
        assert!(got[1].contains("13-24)"), "скобка потеряна: {}", got[1]);

        // короткие и не-printf не трогаем
        assert_eq!(abbrev_stmt("printf(\"Весна\\n\")"), "printf(\"Весна\\n\")");
        assert_eq!(
            abbrev_stmt("a = 111111 + 222222 + 333333"),
            "a = 111111 + 222222 + 333333"
        );
        assert_eq!(
            abbrev_stmt("scanf(\"%d %d\", &a, &b)"),
            "scanf(\"%d %d\", &a, &b)"
        );
    }

    /// Условие с вызовом: форматная строка — шум, переменная — нет.
    /// Раньше отбрасывался весь список аргументов, и разные переменные
    /// давали одинаковый `scanf(...)`.
    #[test]
    fn shorten_calls_keeps_the_target_variable() {
        let a = shorten_calls("scanf(\"%d\", &month) != 1");
        let b = shorten_calls("scanf(\"%d\", &day) != 1");
        assert!(a.contains("month"), "{a}");
        assert!(b.contains("day"), "{b}");
        assert_ne!(a, b, "разные переменные должны различаться");
        assert_eq!(shorten_calls("f(x) != 1"), "f(x) != 1");
        assert_eq!(shorten_calls("a || b"), "a || b");
    }

    /// Пробелы вокруг бинарных операторов: автор может написать
    /// `a= a` или `a =a`, и на схеме это читалось как опечатка.
    /// `++`/`--`/`->` приклеены к операнду, унарные знаки — тоже.
    #[test]
    fn space_operators_normalises_spacing() {
        for (src, want) in [
            ("a= a", "a = a"),
            ("a =a", "a = a"),
            ("a = a", "a = a"),
            ("x=1", "x = 1"),
            ("a<b", "a < b"),
            ("a % b", "a % b"),
            ("b = a*2+1", "b = a * 2 + 1"),
            ("a++", "a++"),
            ("a--", "a--"),
            ("p->x", "p->x"),
            ("&month, &day", "&month, &day"),
        ] {
            assert_eq!(space_operators(src), want, "{src:?}");
        }
        // содержимое строкового литерала не трогаем
        assert_eq!(
            space_operators("q = \"x =y\""),
            "q = \"x =y\"",
            "операторы внутри строки не нормализуются"
        );
    }

    #[test]
    fn strip_comments_keeps_strings() {
        let src = "a = \"http://x\"; // c\nb = 1; /* m */ c = 2;";
        let out = strip_comments(src);
        assert!(out.contains("\"http://x\""));
        assert!(!out.contains("// c"));
        assert!(!out.contains("/* m */"));
        assert_eq!(out.len(), src.len());
    }

    /// те же примеры C, что гоняет selftest.py, — через полный путь
    /// C -> узлы, как это делает main.rs
    #[test]
    fn python_selftest_c_sources() {
        let cases: &[(&str, &str, &[&str])] = &[
            (
                "switch с return в кейсе",
                "#include <stdio.h>\nint main(void) {\n    int a;\n    scanf(\"%d\", &a);\n    switch (a) {\n        case 1:\n            printf(\"раз\");\n            return 2;\n        case 2:\n            printf(\"два\");\n            break;\n        default:\n            a = 9;\n    }\n    printf(\"%d\", a);\n    return 0;\n}",
                &["printf(\"раз\")", "printf(\"два\")", "a = 9"],
            ),
            (
                "объявления и инициализация",
                "#include <stdio.h>\nint main(void) {\n    float a, b;\n    int n = 5;\n    double x = 1.5;\n    a = n * 2;\n    printf(\"%f\", x);\n    return 0;\n}",
                &["n = 5", "x = 1.5", "a = n * 2", "printf(\"%f\", x)"],
            ),
            (
                "if без else",
                "#include <stdio.h>\nint main(void) {\n    int a;\n    scanf(\"%d\", &a);\n    if (a > 0)\n        printf(\"плюс\");\n    printf(\"готово\");\n    return 0;\n}",
                &["printf(\"плюс\")", "printf(\"готово\")"],
            ),
            (
                "циклы и вложенность",
                "#include <stdio.h>\nint main(void) {\n    int s = 0;\n    for (int i = 0; i < 5; i = i + 1) {\n        if (i % 2 == 0) {\n            s = s + i;\n        } else {\n            s = s + 1;\n        }\n    }\n    while (s > 3) {\n        s = s - 1;\n    }\n    printf(\"%d\", s);\n    return 0;\n}",
                &["s = s + i", "s = s + 1", "s = s - 1", "printf(\"%d\", s)"],
            ),
        ];
        for (name, src, wants) in cases {
            let nodes = match parse_c_to_nodes(src, "en") {
                Ok(n) => n,
                Err(e) => panic!("{}: parse failed: {}", name, e),
            };
            let tiles = all_tiles(&nodes);
            for w in *wants {
                assert!(
                    tiles.iter().any(|t| t.contains(w)),
                    "{}: lost {:?} in {:?}",
                    name,
                    w,
                    tiles
                );
            }
        }
    }

    /// stdbool-фаза: вшитый контракт <stdbool.h> — bool/true/false
    /// подставляются, литералы и чужие слова не тронуты.
    #[test]
    fn stdbool_expand_word_boundaries_and_literals() {
        assert_eq!(
            expand_stdbool("bool keep = true; if (!flag_bool) x = trueish;"),
            "_Bool keep = 1; if (!flag_bool) x = trueish;"
        );
        assert_eq!(
            expand_stdbool("printf(\"bool true false\"); c = 'x';"),
            "printf(\"bool true false\"); c = 'x';",
            "строковые литералы не подменяются"
        );
        assert_eq!(
            expand_stdbool("char q = '\\\"'; bool b = false;"),
            "char q = '\\\"'; _Bool b = 0;",
            "экранированные кавычки не ломают скан"
        );
        assert_eq!(
            expand_stdbool("#define bool int"),
            "#define bool int",
            "директивы не трогаем"
        );
    }

    /// Файл лёши: stdbool.h + bool + while(true) + scanf_s должен
    /// строиться целиком (репорт: «4.c почему-то не рисует»).
    #[test]
    fn stdbool_program_parses() {
        let src = "#include <stdio.h>\n#include <stdbool.h>\nint main() {\n    bool keep = true;\n    while (true) {\n        int a;\n        printf(\"num: \");\n        if (scanf_s(\"%d\", &a) != 1) {\n            break;\n        }\n        if (a > 10 || a < 0) {\n            continue;\n        }\n        keep = false;\n    }\n    return 0;\n}";
        let nodes = parse_ok(src);
        let tiles = all_tiles(&nodes);
        assert!(
            tiles.iter().any(|t| t == "keep = 1"),
            "инициализация с подстановкой: {:?}",
            tiles
        );
        assert!(tiles.iter().any(|t| t == "keep = 0"), "false -> 0");
        let loop_text = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Loop)
            .map(|n| n.text.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            loop_text,
            vec!["1"],
            "while (true) с подстановкой true -> 1"
        );
    }
}
