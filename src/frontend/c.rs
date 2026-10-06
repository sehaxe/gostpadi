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
    let mut nodes = Vec::new();
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
    Ok(nodes)
}

/// Код C -> текст схемы .gvn (обратная совместимость с main.rs).
pub fn c_to_gvn(src: &str, labels: &str) -> Result<String, ParseError> {
    let nodes = parse_c_to_nodes(src, labels)?;
    let mut out = String::from("#gostpadi 1\n");
    emit_nodes(&nodes, 0, &mut out);
    Ok(out)
}

fn emit_nodes(nodes: &[Node], depth: usize, out: &mut String) {
    for n in nodes {
        pad(out, depth);
        match n.kind {
            NodeKind::Loop => {
                // keyword добавляем обратно: парсер .gvn снимает его сам
                out.push_str(if n.loop_kind == Some(LoopKind::While) {
                    "while "
                } else {
                    "for "
                });
                out.push_str(&n.text);
                out.push('\n');
                if let Some(body) = &n.body {
                    emit_stmts(body, depth + 1, out);
                }
                continue;
            }
            _ => {
                if n.switch_var.is_none() && n.text.starts_with("if (") && n.text.ends_with(')') {
                    out.push_str("if ");
                    out.push_str(&n.text[4..n.text.len() - 1]);
                } else {
                    out.push_str(&n.text);
                }
                out.push('\n');
            }
        }
        for br in &n.branches {
            pad(out, depth + 1);
            // switch: в IR метка уже с префиксом «svar = » — в текст .gvn
            // пишем сырое значение, иначе gvn::parse навесит префикс дважды
            let label = match &n.switch_var {
                Some(sv) => br
                    .label
                    .strip_prefix(sv.as_str())
                    .and_then(|r| r.strip_prefix(" = "))
                    .unwrap_or(&br.label),
                None => br.label.as_str(),
            };
            out.push_str(label.trim());
            out.push_str(":\n");
            emit_stmts(&br.stmts, depth + 1, out);
        }
    }
}

fn emit_stmts(stmts: &[Stmt], depth: usize, out: &mut String) {
    for s in stmts {
        match s {
            Stmt::Tile { text, .. } | Stmt::Return(text) => {
                pad(out, depth);
                out.push_str(text);
                out.push('\n');
            }
            Stmt::Break => {
                pad(out, depth);
                out.push_str("break\n");
            }
            Stmt::Continue => {
                pad(out, depth);
                out.push_str("continue\n");
            }
            Stmt::Node(n) => emit_nodes(std::slice::from_ref(n), depth, out),
        }
    }
}

fn pad(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

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
                nd.lang = self.labels.to_string();
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
                nd.lang = self.labels.to_string();
                nd.body = Some(self.block_stmt(&f.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::DoWhile(d) => {
                Err(self.err("в коде цикл do-while — перепиши на while", d.span.start))
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
        nd.lang = self.labels.to_string();
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
        nd.lang = self.labels.to_string();
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
        loop {
            match &cur.node {
                Statement::Labeled(l) => {
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
                            return Err(
                                self.err("диапазон case «a ... b» не поддерживается", l.span.start)
                            )
                        }
                        Label::Identifier(_) => false,
                    };
                    if !chained {
                        break;
                    }
                    cur = l.node.statement.as_ref();
                }
                _ => break,
            }
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

fn switch_extend_last(
    branches: &mut Vec<Branch>,
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
            for k in i..j {
                // \n сохраняем: иначе номера строк ошибок съезжают
                out.push(if chars[k] == '\n' { '\n' } else { ' ' });
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
fn expr_text(src: &str, e: &LangNode<Expression>) -> String {
    let s = src.get(e.span.start..e.span.end).unwrap_or_default();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// scanf("%d", &x) -> scanf(...) — форматные строки в условиях не нужны.
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
                    out.push_str(&name);
                    out.push_str("(...)");
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

/// Длинные printf("…") сокращаем до printf("Начало фразы...") —
/// как принято в учебных схемах; условия и присваивания не трогаем.
fn abbrev_stmt(s: &str) -> String {
    if s.chars().count() <= 26 {
        return s.to_string();
    }
    for w in ["printf", "puts", "print", "echo", "write"] {
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
        let cut15: String = content.chars().take(15).collect();
        let cut = match cut15.rfind(' ') {
            Some(p) => &cut15[..p],
            None => &cut15[..],
        };
        return format!("{w}(\"{cut}...\")");
    }
    s.to_string()
}

/// инструкция return (в ветке — тупик)

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
    use crate::style::Style;

    fn parse_ok(src: &str) -> Vec<Node> {
        match parse_c_to_nodes(src, "en") {
            Ok(nodes) => nodes,
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
        // длинный printf сокращён по _abbrev_stmt
        assert!(
            tiles.iter().any(|t| t == "printf(\"Введите номер...\")"),
            "{:?}",
            tiles
        );
        // условие: scanf(...) != 1 || ... по _shorten_calls
        let dec = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Decision && n.switch_var.is_none())
            .unwrap();
        assert_eq!(dec.text, "if (scanf(...) != 1 || month < 1 || month > 12)");
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

    #[test]
    fn gvn_roundtrip_parses() {
        let src = include_str!("../../examples/main.c");
        let gvn = c_to_gvn(src, "en").unwrap();
        let style = Style::default();
        let nodes = crate::frontend::gvn::parse(&gvn, &style, "en").unwrap();
        assert_eq!(nodes[0].kind, NodeKind::Term);
        assert_eq!(nodes.last().unwrap().kind, NodeKind::Term);
        // Start + End + if + switch + 6 printf + (return 1 считается текстом ветки)
        let decisions = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Decision)
            .count();
        assert_eq!(decisions, 2);
        let io = gvn
            .lines()
            .filter(|l| l.trim().starts_with("printf("))
            .count();
        assert_eq!(io, 6, "{}", gvn);
    }

    /// Префикс «svar = » навешивает только gvn::parse: c_to_gvn пишет
    /// сырые значения, иначе раундтрип даёт «month / 3 = month / 3 = 1».
    #[test]
    fn roundtrip_switch_labels_single_prefix() {
        let src = include_str!("../../examples/main.c");
        let gvn = c_to_gvn(src, "en").unwrap();
        assert!(
            !gvn.contains("month / 3 = month"),
            "двойной префикс в gvn:\n{gvn}"
        );
        let nodes = crate::frontend::gvn::parse(&gvn, &Style::default(), "en").unwrap();
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
        let nodes = parse_c_to_nodes("int main(void) { if (a > 0) printf(\"1\"); }", "ru").unwrap();
        assert_eq!(nodes[0].branches[0].label, "да");
        assert_eq!(nodes[0].branches[1].label, "нет");
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
    fn for_header_keeps_keyword_in_gvn() {
        let gvn = c_to_gvn(
            "int main(void) { for (int i = 0; i < 5; i = i + 1) { } }",
            "en",
        )
        .unwrap();
        assert!(gvn.contains("for i = 0; i < 5; i = i + 1\n"), "{}", gvn);
    }

    #[test]
    fn dowhile_and_goto_rejected() {
        let e =
            parse_c_to_nodes("int main(void) { do { x = 1; } while (x < 3); }", "en").unwrap_err();
        assert!(e.msg.contains("do-while"), "{}", e.msg);
        assert_eq!(e.line, Some(1));
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
        assert_eq!(nodes[0].text, "if (scanf(...))");
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

    #[test]
    fn abbrev_long_printf() {
        assert_eq!(
            abbrev_stmt("printf(\"Введите номер месяца (1-12): \")"),
            "printf(\"Введите номер...\")"
        );
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

    #[test]
    fn shorten_calls_only_string_first_arg() {
        assert_eq!(shorten_calls("scanf(\"%d\", &x) != 1"), "scanf(...) != 1");
        assert_eq!(shorten_calls("f(x) != 1"), "f(x) != 1");
        assert_eq!(shorten_calls("a || b"), "a || b");
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

    /// те же примеры C, что гоняет selftest.py, — через полный
    /// путь c_to_gvn -> gvn::parse, как это делает main.rs
    #[test]
    fn python_selftest_c_sources() {
        let style = Style::default();
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
            let gvn = match c_to_gvn(src, "en") {
                Ok(g) => g,
                Err(e) => panic!("{}: c_to_gvn failed: {}", name, e),
            };
            for w in *wants {
                assert!(gvn.contains(w), "{}: lost {:?} in:\n{}", name, w, gvn);
            }
            let nodes = match crate::frontend::gvn::parse(&gvn, &style, "en") {
                Ok(n) => n,
                Err(e) => panic!("{}: gvn reparse failed: {}\n{}", name, e, gvn),
            };
            assert_eq!(nodes[0].kind, NodeKind::Term, "{}", name);
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
