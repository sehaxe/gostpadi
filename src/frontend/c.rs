//! Фронтенд C99 -> FlowIR, замена pycparser (gostpadi.py c_to_gvn).
//! Грамматику читает lang-c; препроцессор (комментарии и директивы)
//! снимается сам — с сохранением нумерации строк, как в Python-версии.

use crate::error::{ParseError, ParseErrorKind};
use crate::frontend::{cpp, tile_kind, Lang, MARKER};
use crate::ir::{Branch, LoopKind, Node, NodeKind, Stmt};
use lang_c::ast::*;
use lang_c::driver::{parse_preprocessed, Config, Flavor, SyntaxError};
use lang_c::span::Node as LangNode;

/// Во что prepare() переписывает `// перенос`: легальный вызов. В IR
/// его узнаёт stmt() по точному тексту выражения.
const MARKER_CALL: &str = "__gsplit__ ()";

/// Код C -> узлы схемы без Start/End: их добавляет `CodeFrontend`.
/// labels: "en" -> yes/no, "ru" -> да/нет.
pub fn parse_c_to_nodes(src: &str, labels: &str) -> Result<Vec<Node>, ParseError> {
    parse_code_to_nodes(src, labels, Lang::C)
}

/// Код C++ -> узлы `main` (для тестов и обратной совместимости).
pub fn parse_cpp_to_nodes(src: &str, labels: &str) -> Result<Vec<Node>, ParseError> {
    parse_code_to_nodes(src, labels, Lang::Cpp)
}

fn parse_code_to_nodes(src: &str, labels: &str, lang: Lang) -> Result<Vec<Node>, ParseError> {
    let prepared = prepare(src, lang);
    let config = Config {
        cpp_command: String::new(),
        cpp_options: Vec::new(),
        flavor: Flavor::StdC11,
    };
    let ast = match parse_preprocessed(&config, prepared.parse.clone()) {
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
        src: &prepared.parse,
        text: &prepared.text,
        orig: src,
        labels,
    };
    let nodes = nodes_from_stmts(ctx.items(body)?);
    if nodes.is_empty() {
        return Err(ParseError::empty(
            "в main нет рисуемых операторов — рисовать нечего (тело пусто \
             или состоит из одних return/объявлений)",
        ));
    }
    Ok(nodes)
}

/// Верхний уровень функции -> узлы схемы. `return` на верхнем уровне не
/// рисуется: терминатор «конец» и так завершает схему.
fn nodes_from_stmts(stmts: Vec<Stmt>) -> Vec<Node> {
    let mut nodes = Vec::new();
    for s in stmts {
        match s {
            Stmt::Return(_) => {}
            Stmt::Tile { kind, text } => nodes.push(Node::new(NodeKind::from(kind), text)),
            Stmt::Break => nodes.push(Node::new(NodeKind::Act, "break")),
            Stmt::Continue => nodes.push(Node::new(NodeKind::Act, "continue")),
            Stmt::Node(n) => nodes.push(*n),
        }
    }
    nodes
}

/// Код C -> текст схемы .gvn: отладочный/экспортный путь, пайплайн
/// им больше не пользуется.
pub fn c_to_gvn(src: &str, labels: &str) -> Result<String, ParseError> {
    let nodes = parse_c_to_nodes(src, labels)?;
    let mut out = String::from("#gostpadi 1\n");
    emit_nodes(&nodes, 0, &mut out);
    Ok(out)
}

/// Код C -> по схеме .gvn на КАЖДУЮ функцию с телом: (имя, gvn).
/// Раньше рисовался только main: файл с helper-функциями терял их
/// молча, а файл без main отказ. Теперь все функции — своя схема,
/// имя несёт «файл#функция».
pub fn c_to_gvn_functions(src: &str, labels: &str) -> Result<Vec<(String, String)>, ParseError> {
    Ok(c_to_gvn_functions_full(src, labels)?
        .into_iter()
        .map(|(name, _, gvn)| (name, gvn))
        .collect())
}

/// Код C/C++ -> узлы каждой функции с телом: (имя, сигнатура, узлы).
/// Это основной путь пайплайна: без сериализации в `.gvn` и обратно.
pub fn parse_functions_full(
    src: &str,
    labels: &str,
    lang: Lang,
) -> Result<Vec<(String, String, Vec<Node>)>, ParseError> {
    let prepared = prepare(src, lang);
    let config = Config {
        cpp_command: String::new(),
        cpp_options: Vec::new(),
        flavor: Flavor::StdC11,
    };
    let ast = match parse_preprocessed(&config, prepared.parse.clone()) {
        Ok(p) => p,
        Err(e) => return Err(syntax_err(e, src)),
    };
    let mut out = Vec::new();
    for ext in &ast.unit.0 {
        let ExternalDeclaration::FunctionDefinition(f) = &ext.node else {
            continue;
        };
        let Some(name) = declarator_name(&f.node.declarator) else {
            continue;
        };
        let signature = declarator_signature(&f.node.declarator, &prepared.text)
            .unwrap_or_else(|| format!("{name}(void)"));
        let Statement::Compound(items) = &f.node.statement.node else {
            continue;
        };
        let ctx = Ctx {
            src: &prepared.parse,
            text: &prepared.text,
            orig: src,
            labels,
        };
        let nodes = nodes_from_stmts(ctx.items(items)?);
        // пустое тело (одни объявления/return) рисует «Start → End» —
        // схему без содержимого. Она не несёт информации и выглядит
        // ошибкой движка, поэтому такую функцию пропускаем. Если в
        // файле не осталось ни одной — отказ (honest > пустая схема).
        if nodes.is_empty() {
            continue;
        }
        out.push((name, signature, nodes));
    }
    if out.is_empty() {
        return Err(ParseError::empty(
            "в коде нет рисуемых операторов — рисовать нечего (тела функций \
             пусты или состоят из одних return/объявлений)",
        ));
    }
    Ok(out)
}

/// То же, но с текстом `.gvn` на функцию — для отладки и обратной
/// совместимости. Пайплайн этим путём больше не ходит.
pub fn c_to_gvn_functions_full(
    src: &str,
    labels: &str,
) -> Result<Vec<(String, String, String)>, ParseError> {
    Ok(parse_functions_full(src, labels, Lang::C)?
        .into_iter()
        .map(|(name, signature, nodes)| {
            let mut gvn = String::from("#gostpadi 1\n");
            emit_nodes(&nodes, 0, &mut gvn);
            (name, signature, gvn)
        })
        .collect())
}

fn emit_nodes(nodes: &[Node], depth: usize, out: &mut String) {
    for n in nodes {
        pad(out, depth);
        match n.kind {
            NodeKind::Loop => {
                // keyword добавляем обратно: парсер .gvn снимает его сам
                out.push_str(match n.loop_kind {
                    Some(LoopKind::While) => "while ",
                    Some(LoopKind::For) => "for ",
                    // дефис: парсер .gvn режет ключевое слово по первому
                    // пробелу/скобке, «do while i < 5» иначе распалось
                    // бы на kw=«do» и хвост
                    Some(LoopKind::DoWhile) => "do-while ",
                    None => "",
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
    /// подготовленный исходник для lang-c (spans указывают в него)
    src: &'a str,
    /// тот же исходник без C++-масок: из него берётся текст подписей
    text: &'a str,
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
            Initializer::Expression(e) => expr_text(self.text, e),
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
                    shorten_calls(&expr_text(self.text, &w.node.expression)),
                );
                nd.loop_kind = Some(LoopKind::While);
                nd.lang = self.labels.to_string();
                nd.body = Some(self.block_stmt(&w.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::For(f) => {
                let init = match &f.node.initializer.node {
                    ForInitializer::Empty | ForInitializer::StaticAssert(_) => String::new(),
                    ForInitializer::Expression(e) => expr_text(self.text, e),
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
                    .map(|e| shorten_calls(&expr_text(self.text, e)))
                    .unwrap_or_default();
                let step = f
                    .node
                    .step
                    .as_ref()
                    .map(|e| expr_text(self.text, e))
                    .unwrap_or_default();
                let mut nd = Node::new(NodeKind::Loop, format!("{}; {}; {}", init, cond, step));
                nd.loop_kind = Some(LoopKind::For);
                nd.lang = self.labels.to_string();
                nd.body = Some(self.block_stmt(&f.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::DoWhile(d) => {
                // тело выполняется ДО проверки условия — в отличие от
                // while/for, где проверка первая. Слово «do while» несёт
                // loop_label: шестиугольник один на все три вида цикла,
                // и без подписи он врал бы, что проверка идёт первой.
                let mut nd = Node::new(
                    NodeKind::Loop,
                    shorten_calls(&expr_text(self.text, &d.node.expression)),
                );
                nd.loop_kind = Some(LoopKind::DoWhile);
                nd.lang = self.labels.to_string();
                nd.body = Some(self.block_stmt(&d.node.statement)?);
                Ok(vec![Stmt::Node(Box::new(nd))])
            }
            Statement::Goto(g) => Err(self
                .err("в коде goto — не поддерживается", g.span.start)
                .with_kind(ParseErrorKind::Unsupported)),
            Statement::Return(e) => Ok(vec![Stmt::Return(match e {
                Some(x) => format!("return {}", expr_text(self.text, x)),
                None => "return".to_string(),
            })]),
            Statement::Break => Ok(vec![Stmt::Break]),
            Statement::Continue => Ok(vec![Stmt::Continue]),
            Statement::Compound(items) => self.items(items),
            Statement::Expression(Some(e)) => {
                let text = expr_text(self.text, e);
                // маркер ручного переноса: prepare() свёл `// перенос`
                // к вызову `__gsplit__ ()`, здесь он становится узлом
                if text == MARKER_CALL {
                    return Ok(vec![Stmt::Node(Box::new(Node::new(
                        NodeKind::Conn,
                        MARKER,
                    )))]);
                }
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
            Statement::Asm(_) => Err(self
                .err("inline asm не поддерживается", st.span.start)
                .with_kind(ParseErrorKind::Unsupported)),
        }
    }

    fn labeled(&self, l: &LangNode<LabeledStatement>) -> Result<Vec<Stmt>, ParseError> {
        match &l.node.label.node {
            Label::Identifier(id) => {
                let text = match &l.node.statement.node {
                    Statement::Expression(Some(e)) => {
                        format!("{}: {}", id.node.name, expr_text(self.text, e))
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
                shorten_calls(&expr_text(self.text, i.node.condition.as_ref()))
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
        let var = expr_text(self.text, &s.node.expression);
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
                            labs.push(format!("{} = {}", var, expr_text(self.text, e.as_ref())));
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

/// Первый «функциональный» кусок декларатора (указатель на функцию —
/// вложенный декларатор, поэтому спускаемся внутрь).
fn function_declarator(d: &LangNode<Declarator>) -> Option<&LangNode<FunctionDeclarator>> {
    for dd in &d.node.derived {
        if let DerivedDeclarator::Function(f) = &dd.node {
            return Some(f);
        }
    }
    match &d.node.kind.node {
        DeclaratorKind::Declarator(inner) => function_declarator(inner),
        _ => None,
    }
}

/// Сигнатура функции по исходнику: имя + список параметров, как их
/// написал студент. Пробелы внутри параметра схлопываются (перенос
/// строки, табы), сам текст типа не сочиняем — срез AST-спана.
/// Без параметров — `имя(void)`, как требует C.
fn declarator_signature(d: &LangNode<Declarator>, src: &str) -> Option<String> {
    let f = function_declarator(d)?;
    let mut parts: Vec<String> = f
        .node
        .parameters
        .iter()
        .filter_map(|p| src.get(p.span.start..p.span.end))
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .collect();
    if parts.is_empty() {
        parts.push("void".into());
    }
    if f.node.ellipsis == Ellipsis::Some {
        parts.push("...".into());
    }
    Some(format!("{}({})", declarator_name(d)?, parts.join(", ")))
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

/// Подготовленный вход: `parse` читает lang-c, `text` — тот же текст для
/// подписей блоков. Длины совпадают байт в байт: C++-обвязка маскируется
/// пробелами, поэтому span'ы AST годятся для обоих срезов. В `text`
/// остаются `&`, `string`, `auto` и `using`; `std::` в подписи опускается.
struct Prepared {
    parse: String,
    text: String,
}

impl Prepared {
    /// Инвариант C++-масок: `parse` и `text` совпадают байт в байт,
    /// иначе span'ы lang-c указывают в разные места. Ловим это здесь,
    /// а не в случайном тесте раскладки.
    fn new(parse: String, text: String) -> Self {
        assert_eq!(
            parse.len(),
            text.len(),
            "C++-маска изменила длину строки: {parse:?} vs {text:?}"
        );
        Self { parse, text }
    }
}

/// pyconverter-подготовка: снять комментарии и строки препроцессора.
/// Директивы заменяются пустыми строками — нумерация строк сохраняется.
///
/// Континуации (`#define X \` + следующая строка) склеиваются в одну:
/// раньше продолжение оставалась кодом и роняло парсер («неожидаемый
/// токен do_something()»). Складка сохраняет число строк: продолжения
/// уходят пустыми строками, иначе номера строк в ошибках съезжают.
///
/// Выключенные ветки (`#if 0 … #endif`) вырезаются вместе с телом —
/// это закомментированный код, рисовать его нельзя. Ветки `#ifdef /
/// #ifndef / #if 1` считаются включёнными: заголовочные файлы студент
/// underscope подключает осознанно.
fn prepare(src: &str, lang: Lang) -> Prepared {
    prepare_impl(src, crate::frontend::autofix_enabled(), lang)
}

fn prepare_impl(src: &str, autofix: bool, lang: Lang) -> Prepared {
    #[cfg(feature = "autofix")]
    let src = if autofix {
        crate::frontend::fix::normalize(src)
    } else {
        src.to_string()
    };
    #[cfg(not(feature = "autofix"))]
    let src = {
        let _ = autofix;
        src.to_string()
    };
    let src = src.trim_start_matches('\u{feff}').to_string();
    // 1) склеить континуации директив (число строк сохраняется)
    let joined = join_continuations(&src).join("\n");
    // 1.5) маркер ручного переноса — в легальный C-стейтмент на той же
    //      позиции: strip_comments его уже не увидит, \n остаются на месте
    let marked = rewrite_markers(&joined);
    // 2) снять комментарии — по ВСЕМУ тексту: /* */ живёт через
    //    построчно строки, и на перловой строке его не видно
    let stripped = strip_comments(&marked);
    // 2.5) namespace X { ... } — обёртка C++; функции внутри остаются
    //      обычными функциями файла. Для C этот проход не нужен.
    let stripped = if lang == Lang::Cpp {
        cpp::strip_namespaces(&stripped)
    } else {
        stripped
    };
    // 3) вырезать выключенные ветки и сами директивы
    let no_comments: Vec<String> = stripped.split('\n').map(str::to_string).collect();
    // стек условий препроцессора: (ветка_живая, была_ли_else)
    let mut cond: Vec<(bool, bool)> = Vec::new();
    let disabled = |cond: &Vec<(bool, bool)>| cond.iter().any(|(on, _)| !on);
    let mut out: Vec<(String, String)> = Vec::with_capacity(no_comments.len());
    for line in &no_comments {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix('#') else {
            if disabled(&cond) {
                out.push((String::new(), String::new()));
            } else {
                let text = expand_stdbool(line);
                let (parse, display) = match lang {
                    Lang::C => (text.clone(), text),
                    Lang::Cpp => (cpp::normalize_line(&text), cpp::display_line(&text)),
                };
                out.push((parse, display));
            }
            continue;
        };
        let rest = rest.trim();
        let word: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        match word.as_str() {
            "if" | "ifdef" | "ifndef" => {
                let parent_on = !disabled(&cond);
                // литеральное условие: #if 0 выключен; #if 1/defined(X) включён
                let on = match word.as_str() {
                    "if" => {
                        let arg = rest.strip_prefix("if").unwrap_or("").trim();
                        !(arg == "0" || arg.is_empty())
                    }
                    _ => true, // ifdef/ifndef — включаем (заголовок есть)
                };
                cond.push((parent_on && on, false));
            }
            "else" => {
                // по правилам C ветка else включается, когда if выключен:
                // #if 0 … #else … #endif — тело else это ЖИВОЙ код
                if let Some(top) = cond.last_mut() {
                    if !top.1 {
                        top.0 = !top.0;
                        top.1 = true;
                    }
                }
            }
            "elif" => {
                if let Some(top) = cond.last_mut() {
                    if !top.1 {
                        top.0 = true;
                    }
                }
            }
            "endif" => {
                cond.pop();
            }
            _ => {}
        }
        out.push((String::new(), String::new()));
    }
    Prepared::new(
        out.iter()
            .map(|(p, _)| p.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        out.iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// `// перенос` -> `__gsplit__ ();` на той же позиции строки: маркер
/// доезжает до AST обычным вызовом. Строк не прибавляет и не убавляет,
/// поэтому номера строк в ошибках остаются координатами исходника.
fn rewrite_markers(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 16);
    for (i, line) in src.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match marker_comment_at(line) {
            Some(p) => {
                out.push_str(&line[..p]);
                out.push_str(MARKER_CALL);
                out.push(';');
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Позиция комментария ровно `// перенос` в строке. Строгое равенство:
/// «переносить», «переносы» маркером не являются.
fn marker_comment_at(line: &str) -> Option<usize> {
    let p = line.find("//")?;
    (line[p + 2..].trim() == MARKER).then_some(p)
}

/// Склейка континуций: строка директивы, кончающаяся `\`, продолжается
/// следующими физическими строками. Число строк сохраняется — каждой
/// склеенной строке на выходе соответствует столько же строк входа
/// (первая несёт склеенный текст, остальные пустые), иначе номера
/// строк в сообщениях об ошибках съезжают.
fn join_continuations(src: &str) -> Vec<String> {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let mut acc = lines[i].to_string();
        // только директивы продолжаются: '#' + ... + '\'
        if acc.trim_start().starts_with('#') {
            let mut consumed = 0;
            while acc.trim_end().ends_with('\\') && i + 1 < lines.len() {
                acc.pop(); // снять '\'
                i += 1;
                consumed += 1;
                acc.push(' ');
                acc.push_str(lines[i].trim_end());
            }
            out.push(acc);
            // поглощённые строки уходят пустыми: нумерация цела
            out.extend(std::iter::repeat(String::new()).take(consumed));
        } else {
            out.push(acc);
        }
        i += 1;
    }
    out
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
    fn prep_c(src: &str) -> Prepared {
        prepare(src, Lang::C)
    }

    fn prep_cpp(src: &str) -> Prepared {
        prepare(src, Lang::Cpp)
    }

    use crate::style::Style;

    fn parse_ok(src: &str) -> Vec<Node> {
        match parse_c_to_nodes(src, "en") {
            Ok(nodes) => nodes,
            Err(e) => panic!("parse failed: {}", e),
        }
    }

    fn parse_cpp_ok(src: &str) -> Vec<Node> {
        match parse_cpp_to_nodes(src, "en") {
            Ok(nodes) => nodes,
            Err(e) => panic!("cpp parse failed: {}", e),
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

    /// Маркер ручного переноса: `// перенос` становится легальным
    /// C-стейтментом на той же строке (число строк цело); отдельный
    /// случай — маркер в хвосте строки с кодом; «переносить» не маркер.
    #[test]
    fn prepare_rewrites_transfer_marker() {
        let src = concat!(
            "int main(void) {\n",
            "    int a = 1; // перенос\n",
            "    // перенос\n",
            "    a = 2;\n",
            "    // переносить\n",
            "}\n",
        );
        let p = prep_c(src).parse;
        assert_eq!(p.matches("__gsplit__ ();").count(), 2, "{p}");
        assert!(
            p.contains("    int a = 1; __gsplit__ ();"),
            "хвостовой маркер: {p}"
        );
        assert!(p.contains("    __gsplit__ ();"), "маркер-строка: {p}");
        assert_eq!(
            p.matches('\n').count(),
            src.matches('\n').count(),
            "число строк обязано сохраниться: {p}"
        );
    }

    /// Маркер доезжает по C-пути до узла схемы: conn «перенос».
    #[test]
    fn marker_statement_becomes_conn_node() {
        let nodes = parse_ok("int main(void) {\n    int a = 1;\n    // перенос\n    a = 2;\n}");
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == NodeKind::Conn && n.text == MARKER),
            "{nodes:?}"
        );
        assert_eq!(
            texts(&nodes, NodeKind::Act),
            vec!["a = 1".to_string(), "a = 2".to_string()]
        );
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
    /// do-while — полноценный цикл (не отказ, решение 28.09.2026,
    /// восстановлено после откатa): рисуется той же парой трапеций,
    /// но заголовок несёт «do while» — проверка идёт ПОСЛЕ тела.
    fn dowhile_is_a_loop_and_goto_still_rejected() {
        let nodes = parse_ok("int main(void) { do { x = 1; } while (x < 3); }");
        let l = &nodes[0];
        assert_eq!(l.kind, NodeKind::Loop);
        assert_eq!(l.loop_kind, Some(LoopKind::DoWhile));
        assert_eq!(l.loop_label(), "do while x < 3");
        assert!(l.body.is_some(), "тело цикла на месте");
        // goto по-прежнему отказ: развернуть его в схему нельзя
        let e2 = parse_c_to_nodes("int main(void) {\n  goto end;\n  end: ;\n}", "en").unwrap_err();
        assert!(e2.msg.contains("goto"), "{}", e2.msg);
        assert_eq!(e2.line, Some(2));
    }

    /// break/continue внутри do-while — те же правила, что в while:
    /// break рельсой к выходу цикла, continue рельсой к началу итерации.
    #[test]
    fn dowhile_exits_behave_like_while() {
        let src = "int main(void) { do { if (a > 5) break; if (a < 0) continue; a = a + 1; } while (a < 10); }";
        let nodes = parse_ok(src);
        let l = &nodes[0];
        assert_eq!(l.loop_kind, Some(LoopKind::DoWhile));
        let body = l.body.as_ref().unwrap();
        // break/continue сидят внутри веток if — так же, как в while
        let nested: Vec<&Stmt> = body
            .iter()
            .filter_map(|s| match s {
                Stmt::Node(n) if n.kind == NodeKind::Decision => Some(&n.branches),
                _ => None,
            })
            .flatten()
            .flat_map(|b| b.stmts.iter())
            .collect();
        assert!(
            nested.iter().any(|s| matches!(s, Stmt::Break)),
            "break внутри ветки do-while типизирован"
        );
        assert!(
            nested.iter().any(|s| matches!(s, Stmt::Continue)),
            "continue внутри ветки do-while типизирован"
        );
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

    /// Пустое тело: main из одних return — отказ, а не схема «Start → End».
    /// Раньше такая схема рисовалась и выглядела ошибкой движка.
    #[test]
    fn main_with_only_return_is_refused() {
        let e = parse_c_to_nodes("int main(void) { return 0; }", "en").unwrap_err();
        assert!(e.msg.contains("нет рисуемых операторов"), "{}", e.msg);
        assert_eq!(e.kind, ParseErrorKind::Empty);
        // и с выражениями в return — тоже: return что-то считает
        let e2 = parse_c_to_nodes("int main(void) { return a + 1; }", "en").unwrap_err();
        assert!(e2.msg.contains("нет рисуемых операторов"), "{}", e2.msg);
        assert_eq!(e2.kind, ParseErrorKind::Empty);
        // объявления без инициализаторов не рисуются — тот же отказ
        let e3 = parse_c_to_nodes("int main(void) { int a; float b; }", "en").unwrap_err();
        assert!(e3.msg.contains("нет рисуемых операторов"), "{}", e3.msg);
        assert_eq!(e3.kind, ParseErrorKind::Empty);
    }

    /// `goto` — не «пусто», а осознанно неподдерживаемая конструкция.
    #[test]
    fn goto_is_unsupported_kind() {
        let e = parse_c_to_nodes("int main(void) { goto end; end: return 0; }", "en").unwrap_err();
        assert_eq!(e.kind, ParseErrorKind::Unsupported, "{}", e.msg);
    }

    /// Функция-пустышка пропускается, main с телом рисуется: пустышка не
    /// порождает отдельный файл «Start → End».
    #[test]
    fn empty_helper_is_skipped_main_drawn() {
        let src = "int add(int a, int b) { return a + b; }\nint main(void) { printf(\"%d\", 1); return 0; }";
        let fs = c_to_gvn_functions(src, "en").unwrap();
        assert_eq!(fs.len(), 1, "пустышка add пропущена: {:?}", fs);
        assert_eq!(fs[0].0, "main");
    }

    /// Функция с телом из одного вложенного цикла НЕ пуста (у цикла есть
    /// тело) — рисуется, даже если плиток в нём нет. Ловит излишне жадный
    /// фильтр: цикл do {} while(0) остаётся на схеме.
    #[test]
    fn helper_with_only_loop_is_drawn() {
        let src = "void spin(void) { while (1) { } }\nint main(void) { spin(); return 0; }";
        let fs = c_to_gvn_functions(src, "en").unwrap();
        let names: Vec<&str> = fs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"spin"),
            "цикл функции остаётся: {:?}",
            names
        );
        assert!(names.contains(&"main"), "main на месте: {:?}", names);
    }

    /// Все функции файла — пустышки: отказ по всему файлу (не пустая пачка).
    #[test]
    fn file_of_empty_functions_is_refused() {
        let src =
            "int add(int a, int b) { return a + b; }\nint sub(int a, int b) { return a - b; }";
        let e = c_to_gvn_functions(src, "en").unwrap_err();
        assert!(e.msg.contains("нет рисуемых операторов"), "{}", e.msg);
        assert_eq!(e.kind, ParseErrorKind::Empty);
    }

    /// Подпись функции — сигнатура из C как её написал студент:
    /// параметры со спецификаторами и массивами, без параметров — (void).
    #[test]
    fn function_signature_from_source() {
        let src = include_str!("../../corpus/oap-lab3.c");
        let fs = c_to_gvn_functions_full(src, "en").unwrap();
        let sig = |n: &str| {
            fs.iter()
                .find(|(name, _, _)| name == n)
                .map(|(_, s, _)| s.as_str())
                .unwrap_or_else(|| panic!("нет функции {n}: {fs:?}"))
        };
        assert_eq!(sig("sum_neg"), "sum_neg(int a[], int n)");
        assert_eq!(sig("print_mas"), "print_mas(const int a[], int n)");
        assert_eq!(sig("main"), "main(void)");

        let fs = c_to_gvn_functions_full(
            "int Min(int x, int y) { printf(\"%d\", x); return x; }\n\
             void f(void) { printf(\"x\"); }\n\
             int g() { printf(\"y\"); }\n\
             int *h(char **v, ...) { printf(\"z\"); }",
            "en",
        )
        .unwrap();
        let sig = |n: &str| {
            fs.iter()
                .find(|(name, _, _)| name == n)
                .map(|(_, s, _)| s.to_string())
                .unwrap_or_default()
        };
        assert_eq!(sig("Min"), "Min(int x, int y)");
        assert_eq!(sig("f"), "f(void)");
        assert_eq!(sig("g"), "g(void)");
        assert_eq!(sig("h"), "h(char **v, ...)");
    }

    /// Декларация со значением — рисуемая, поэтому main с одной такой
    /// строкой не пуст.
    #[test]
    fn main_with_only_initialized_declaration_draws() {
        let nodes = parse_c_to_nodes("int main(void) { int n = 5; return 0; }", "en").unwrap();
        assert!(
            nodes.iter().any(|n| n.text.contains("n = 5")),
            "{:?}",
            nodes.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
    }

    /// Продолжение директивы: `#define X \` + следующая строка склеиваются,
    /// продолжение не остаётся кодом (раньше парсер падал на нём).
    #[test]
    fn directive_continuation_is_joined() {
        let src = "#define MAX(a, b) \\\n    ((a) > (b) ? (a) : (b))\nint main(void) { int m = MAX(1, 2); return 0; }";
        let nodes = parse_ok(src);
        assert!(
            nodes.iter().any(|n| n.text.contains("m = MAX")),
            "{:?}",
            nodes.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
        // нумерация строк сохраняется: продолжения уходят пустыми строками
        let prepared = prep_c(src).parse;
        assert_eq!(prepared.lines().count(), src.lines().count());
    }

    /// Выключенная ветка `#if 0 … #endif` вырезается целиком, включая
    /// вложенные #if. Ветка `#else` при этом ЖИВАЯ — по правилам C
    /// препроцессора тело else включается, когда if выключен.
    #[test]
    fn if0_block_is_stripped() {
        let src = "#include <stdio.h>\nint main(void) {\n#if 0\n    printf(\"dead\");\n#if 1\n    printf(\"dead nested\");\n#endif\n#endif\n    printf(\"live\");\n    return 0;\n}";
        let nodes = parse_ok(src);
        let texts: Vec<&str> = nodes.iter().map(|n| n.text.as_str()).collect();
        assert!(texts.contains(&"printf(\"live\")"), "{:?}", texts);
        assert!(!texts.iter().any(|t| t.contains("dead")), "{:?}", texts);
    }

    /// `#if 0 … #else … #endif`: по правилам C ветка else — живой код.
    #[test]
    fn if0_else_branch_is_live() {
        let src = "int main(void) {\n#if 0\n    printf(\"dead\");\n#else\n    printf(\"live else\");\n#endif\n    return 0;\n}";
        let nodes = parse_ok(src);
        let texts: Vec<&str> = nodes.iter().map(|n| n.text.as_str()).collect();
        assert!(texts.contains(&"printf(\"live else\")"), "{:?}", texts);
        assert!(!texts.iter().any(|t| t.contains("dead")), "{:?}", texts);
    }

    /// `#ifdef`/`#ifndef` считаются включёнными: заголовок студент
    /// подключает осознанно, ветка с телом рисуется.
    #[test]
    fn ifdef_branch_is_kept() {
        let src =
            "int main(void) {\n#ifdef _DEBUG\n    printf(\"debug\");\n#endif\n    return 0;\n}";
        let nodes = parse_ok(src);
        assert!(
            nodes.iter().any(|n| n.text.contains("debug")),
            "{:?}",
            nodes.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
    }

    /// Включённая ветка с else: первая ветка живая, вторая выключена.
    #[test]
    fn if1_else_picks_live_branch() {
        let src = "int main(void) {\n#if 1\n    printf(\"first\");\n#else\n    printf(\"second\");\n#endif\n    return 0;\n}";
        let nodes = parse_ok(src);
        let texts: Vec<&str> = nodes.iter().map(|n| n.text.as_str()).collect();
        assert!(texts.contains(&"printf(\"first\")"), "{:?}", texts);
        assert!(!texts.iter().any(|t| t.contains("second")), "{:?}", texts);
    }

    /// Ошибка после вырезанного блока указывает на настоящую строку:
    /// нумерация не съезжает из-за вырезанных строк.
    #[test]
    fn error_line_survives_if0_strip() {
        // строка 2..4 — выключенный блок; строка 5 — реальная ошибка
        let src = "int main(void) {\n#if 0\n    dead = 1;\n#endif\n    a = ;\n    return 0;\n}";
        let e = parse_c_to_nodes(src, "en").unwrap_err();
        assert_eq!(e.line, Some(5), "ошибка на строке 5: {:?}", e);
    }

    /// Кривое форматирование даёт ту же схему, что и аккуратное:
    /// табы, CRLF, BOM, пробелы в директиве, `if(...)...else` в одну строку.
    #[cfg(feature = "autofix")]
    #[test]
    fn autofix_ugly_matches_clean() {
        let clean = "int main(void) {\n    int a;\n    scanf(\"%d\", &a);\n    if (a > 0) {\n        printf(\"плюс\");\n    } else {\n        printf(\"минус\");\n    }\n    return 0;\n}\n";
        let ugly = "\u{feff}# include < stdio . h >\r\nint main(void)\r\n{\r\n\tint a;\r\n\tscanf(\"%d\",&a);\r\n\tif(a > 0){printf(\"плюс\");}else{printf(\"минус\");}\r\n\treturn 0;\r\n}\r\n";
        let want = parse_c_to_nodes(clean, "ru").unwrap();
        let got = parse_c_to_nodes(ugly, "ru").unwrap();
        let kinds = |ns: &[Node]| ns.iter().map(|n| n.kind.clone()).collect::<Vec<_>>();
        assert_eq!(kinds(&want), kinds(&got), "набор блоков изменился");
        assert_eq!(want.len(), got.len());
    }

    /// Отступы в глубине (2 пробела/табы вперемешку) не меняют число блоков.
    #[cfg(feature = "autofix")]
    #[test]
    fn autofix_mixed_indent_keeps_blocks() {
        let clean = "int main(void) {\n    int s = 0;\n    for (int i = 0; i < 3; i++) {\n        s += i;\n    }\n    printf(\"%d\", s);\n    return 0;\n}\n";
        let ugly = "int main(void){\n \t int s=0;\n\tfor(int i=0;i<3;i++){\n  \t\ts+=i;\n }printf(\"%d\",s);\nreturn 0;}\n";
        let a = parse_c_to_nodes(clean, "en").unwrap();
        let b = parse_c_to_nodes(ugly, "en").unwrap();
        assert_eq!(a.len(), b.len(), "число блоков изменилось");
        let kinds = |ns: &[Node]| ns.iter().map(|n| n.kind.clone()).collect::<Vec<_>>();
        assert_eq!(kinds(&a), kinds(&b));
    }

    /// Строгий режим (--no-fix / --no-default-features) оставляет CR-только
    /// файл как есть — а это не C: парсер на нём падает.
    #[cfg(feature = "autofix")]
    #[test]
    fn strict_mode_keeps_cr_only_file_unparseable() {
        let src = "int main(void)\r{\r// сумма\rint a = 1;\rprintf(\"%d\", a);\rreturn 0;\r}\r";
        assert!(parse_c_to_nodes(src, "en").is_ok(), "фикс должен спасти");
        let strict = prepare_impl(src, false, Lang::C).parse;
        assert!(strict.contains('\r'), "строгий режим не должен трогать CR");
        let config = Config {
            cpp_command: String::new(),
            cpp_options: Vec::new(),
            flavor: Flavor::StdC11,
        };
        assert!(
            parse_preprocessed(&config, strict).is_err(),
            "строгий разбор CR-только файла обязан падать"
        );
    }

    /// C-вход не проходит C++-нормализацию: `constexpr` — обычное имя.
    #[test]
    fn c_code_is_not_cpp_normalized() {
        let p = prep_c("int main() { int constexpr = 1; printf(\"%d\", constexpr); }");
        assert!(p.parse.contains("constexpr"), "{}", p.parse);
        assert_eq!(p.parse, p.text);
    }

    /// `using namespace std;` и `std::` — не C. Маскируются пробелами:
    /// lang-c видит чистый C, а подпись блока остаётся исходной.
    #[test]
    fn cpp_using_and_scope_are_masked() {
        let src = concat!(
            "#include <iostream>\n",
            "using namespace std;\n",
            "using std::endl;\n",
            "int main() {\n",
            "    std::cout << \"hi\" << endl;\n",
            "    return 0;\n",
            "}\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("using"), "{}", p.parse);
        assert!(!p.parse.contains("std::"), "{}", p.parse);
        assert!(
            p.text.contains("cout << \"hi\" << endl"),
            "подпись без std::: {}",
            p.text
        );
        assert!(!p.text.contains("std::"), "{}", p.text);
        assert_eq!(p.parse.lines().count(), src.lines().count());
        assert_eq!(p.parse.len(), p.text.len(), "span'ы общие для двух срезов");
    }

    /// `using namespace std::chrono;` — тоже using-директива, а не код.
    #[test]
    fn cpp_using_qualified_namespace_is_masked() {
        let p = prep_cpp("using namespace std::chrono;\nint main() { return 0; }");
        assert!(!p.parse.contains("using"), "{}", p.parse);
    }

    /// Ссылка в объявлении: для разбора `*`, для схемы — исходный `&`.
    #[test]
    fn cpp_references_parse_as_pointers() {
        let src = concat!(
            "void swap(int &a, int &b) {\n",
            "    int t = a;\n",
            "    a = b;\n",
            "    b = t;\n",
            "}\n",
            "int main() {\n",
            "    int x = 1, y = 2;\n",
            "    swap(x, y);\n",
            "    return 0;\n",
            "}\n",
        );
        let p = prep_cpp(src);
        assert!(p.parse.contains("int *a, int *b"), "{}", p.parse);
        assert!(p.text.contains("int &a, int &b"), "{}", p.text);
        let schemes = parse_functions_full(src, "en", Lang::Cpp).expect("C++ с ссылками");
        let swap = schemes.iter().find(|(n, _, _)| n == "swap").unwrap();
        assert_eq!(swap.1, "swap(int &a, int &b)");
    }

    /// `&` в выражении и адрес переменной — не ссылки: их трогать нельзя.
    #[test]
    fn cpp_bitwise_and_and_address_of_are_kept() {
        let src = concat!(
            "int main() {\n",
            "    int a = 6, b = 3;\n",
            "    int c = a & b;\n",
            "    int *p = &a;\n",
            "    printf(\"%d %d\", c, *p);\n",
            "    return 0;\n",
            "}\n",
        );
        let p = prep_cpp(src);
        assert!(p.parse.contains("a & b"), "{}", p.parse);
        assert!(p.parse.contains("&a"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// `string` — не тип C: для разбора `int`, подпись остаётся `string s`.
    #[test]
    fn cpp_string_type_is_replaced_for_parser() {
        let src = "int main() {\n    string s = \"hi\";\n    std::cout << s;\n    return 0;\n}\n";
        let p = prep_cpp(src);
        assert!(p.parse.contains("int    s = \"hi\""), "{}", p.parse);
        assert!(p.text.contains("string s = \"hi\""), "{}", p.text);
        assert_eq!(p.parse.len(), p.text.len());
        let nodes = parse_cpp_ok(src);
        let tiles = texts(&nodes, NodeKind::Act);
        assert!(tiles.iter().any(|t| t == "s = \"hi\""), "{tiles:?}");
        let io = texts(&nodes, NodeKind::Io);
        assert!(io.iter().any(|t| t == "cout << s"), "{io:?}");
    }

    /// `string` как обычный идентификатор (`string = 1`) — не тип.
    #[test]
    fn cpp_string_variable_is_kept() {
        let p = prep_cpp("int main() { int string = 1; printf(\"%d\", string); }");
        assert!(p.parse.contains("int string = 1"), "{}", p.parse);
    }

    /// `cpplabs::readNumber` — `::` в C нет; для разбора `__`, подпись
    /// остаётся с `::` (span начинается на первом имени).
    #[test]
    fn cpp_scope_is_replaced_for_parser() {
        let src = "int main() { int n = cpplabs::readNumber(\"n: \"); printf(\"%d\", n); }";
        let p = prep_cpp(src);
        assert!(p.parse.contains("cpplabs__readNumber"), "{}", p.parse);
        assert!(p.text.contains("cpplabs::readNumber"), "{}", p.text);
        parse_cpp_ok(src);
    }

    /// `auto x = ...` — для разбора `int`; `auto [a, b]` не трогаем.
    #[test]
    fn cpp_auto_is_replaced() {
        let p = prep_cpp("int main() { auto x = 1; auto& r = x; printf(\"%d\", r); }");
        assert!(p.parse.contains("int  x = 1"), "{}", p.parse);
        assert!(p.parse.contains("int * r"), "{}", p.parse);
        assert!(
            prep_cpp("int main() { auto [a, b] = f(); }")
                .parse
                .contains("auto [a, b]"),
            "structured binding не трогаем"
        );
    }

    /// `{}` и `{expr}` в объявлениях — C++11; для разбора `= 0`/`= expr`.
    #[test]
    fn cpp_brace_initializers() {
        let p = prep_cpp(concat!(
            "int main() {\n",
            "    double a{};\n",
            "    bool b = {};\n",
            "    double c{1.5};\n",
            "    int d{foo(1, 2)};\n",
            "    printf(\"%f %d %f %d\", a, b, c, d);\n",
            "}\n",
        ));
        assert!(p.parse.contains("double a  ;"), "{}", p.parse);
        assert!(p.parse.contains("_Bool b = 0 ;"), "{}", p.parse);
        assert!(p.parse.contains("double c=1.5;;"), "{}", p.parse);
        assert!(p.parse.contains("int d=foo(1, 2);;"), "{}", p.parse);
    }

    /// `std::vector` — шаблон, вне поддержки: остаётся честная ошибка,
    /// а не молчаливая кривая схема.
    #[test]
    fn cpp_stl_template_is_not_silently_mangled() {
        assert!(parse_c_to_nodes("int main() { std::vector<int> v; return 0; }", "en").is_err());
    }

    /// `namespace X { ... }` — обёртка: функции внутри становятся
    /// функциями файла, `X::f` для разбора — обычное имя.
    #[test]
    fn cpp_namespace_wrapper_is_stripped() {
        let src = concat!(
            "namespace lab {\n",
            "int add(int a, int b) { int s = a + b; return s; }\n",
            "}\n",
            "int main() { printf(\"%d\", lab::add(1, 2)); return 0; }\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("namespace"), "{}", p.parse);
        assert_eq!(p.parse.lines().count(), src.lines().count());
        let schemes = parse_functions_full(src, "en", Lang::Cpp).expect("namespace-обёртка");
        assert!(schemes.iter().any(|(n, _, _)| n == "add"), "{schemes:?}");
        parse_cpp_ok(src);
    }

    /// `namespace fs = std::filesystem;` — алиас, тоже обёртка.
    #[test]
    fn cpp_namespace_alias_is_stripped() {
        let p = prep_cpp("namespace fs = std::filesystem;\nint main() { return 0; }");
        assert!(!p.parse.contains("namespace"), "{}", p.parse);
        assert!(!p.parse.contains("std::"), "{}", p.parse);
    }

    /// `static_cast<T>(x)` — для разбора `(T)(x)`, подпись исходная.
    #[test]
    fn cpp_static_cast_is_rewritten() {
        let src = "int main() { double x = 1.5; int n = static_cast<int>(x); printf(\"%d\", n); }";
        let p = prep_cpp(src);
        assert!(!p.parse.contains("static_cast"), "{}", p.parse);
        assert!(p.text.contains("static_cast<int>(x)"), "{}", p.text);
        parse_cpp_ok(src);
    }

    /// Остальные C++-касты и `std::string` внутри каста.
    #[test]
    fn cpp_other_casts_are_rewritten() {
        let src = concat!(
            "int main() {\n",
            "    long long n = 1;\n",
            "    char *p = reinterpret_cast<char *>(n);\n",
            "    string s = static_cast<std::string>(p);\n",
            "    printf(\"%s %p\", s, (void *)p);\n",
            "    return 0;\n",
            "}\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("reinterpret_cast"), "{}", p.parse);
        assert!(!p.parse.contains("static_cast"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// `using ::std::cout;` — ведущий `::` в using-декларации.
    #[test]
    fn cpp_using_leading_scope_is_masked() {
        let p = prep_cpp("using ::std::cout;\nint main() { cout << 1; }");
        assert!(!p.parse.contains("using"), "{}", p.parse);
    }

    /// `auto int x` — storage class C, а не вывод типа: не трогаем.
    #[test]
    fn cpp_auto_storage_class_is_kept() {
        let p = prep_cpp("int main() { auto int x = 1; printf(\"%d\", x); }");
        assert!(p.parse.contains("auto int x"), "{}", p.parse);
    }

    /// Блочные `{}` и списки инициализации не принимаются за `{expr}`.
    #[test]
    fn cpp_block_braces_are_not_initializers() {
        let src = concat!(
            "struct P { int x; };\n",
            "enum E { A };\n",
            "int main() {\n",
            "    int a[] = {1, 2};\n",
            "    if (a[0]) { printf(\"x\"); } else { printf(\"y\"); }\n",
            "    return 0;\n",
            "}\n",
        );
        let p = prep_cpp(src);
        assert!(p.parse.contains("int a[] = {1, 2}"), "{}", p.parse);
        assert!(p.parse.contains("if (a[0]) {"), "{}", p.parse);
        assert!(p.parse.contains("struct P { int x; }"), "{}", p.parse);
        assert!(p.parse.contains("enum E { A }"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// `std::size_t` и потоки — типы без C-аналога.
    #[test]
    fn cpp_std_scalar_and_stream_types() {
        let p = prep_cpp(
            "int main() { std::size_t n = 0; std::istream& in = std::cin; printf(\"%zu\", n); }",
        );
        assert!(!p.parse.contains("std::size_t"), "{}", p.parse);
        assert!(!p.parse.contains("std::istream"), "{}", p.parse);
        assert!(!p.parse.contains("std::cin"), "{}", p.parse);
    }

    /// `namespace a::b` и `inline namespace` — тоже обёртки.
    #[test]
    fn cpp_namespace_qualified_and_inline() {
        let src = concat!(
            "inline namespace lab {\n",
            "int f() { return 1; }\n",
            "}\n",
            "namespace a::b {\n",
            "int g() { return 2; }\n",
            "}\n",
            "int main() { printf(\"%d %d\", lab::f(), a::b::g()); return 0; }\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("namespace"), "{}", p.parse);
        assert!(!p.parse.contains("inline"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// `namespace X` и `{` на разных строках — обычное форматирование.
    #[test]
    fn cpp_namespace_brace_on_next_line() {
        let src = concat!(
            "namespace lab\n",
            "{\n",
            "int f() { return 1; }\n",
            "}\n",
            "int main() { printf(\"%d\", lab::f()); return 0; }\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("namespace"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// `namespace X { ... };` — лишняя `;` после `}` тоже снимается.
    #[test]
    fn cpp_namespace_trailing_semicolon_is_stripped() {
        let src = concat!(
            "namespace lab {\n",
            "int add(int a, int b) { int s = a + b; return s; }\n",
            "};\n",
            "int main() { printf(\"%d\", lab::add(1, 2)); return 0; }\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("namespace"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// Вложенный namespace: маскируются оба заголовка и обе `}`.
    #[test]
    fn cpp_nested_namespaces_are_stripped() {
        let src = concat!(
            "namespace a {\n",
            "namespace b {\n",
            "int f() { return 1; }\n",
            "}\n",
            "}\n",
            "int main() { printf(\"%d\", a::b::f()); return 0; }\n",
        );
        let p = prep_cpp(src);
        assert!(!p.parse.contains("namespace"), "{}", p.parse);
        parse_cpp_ok(src);
    }

    /// Строковые литералы неприкосновенны: внутри них нет ни using, ни std::.
    #[test]
    fn cpp_literals_are_untouched() {
        let src = "int main() { printf(\"using namespace std; std::cout & int\"); }";
        let p = prep_cpp(src);
        assert!(
            p.parse.contains("using namespace std; std::cout & int"),
            "{}",
            p.parse
        );
    }

    /// `using` — обычное имя в C, если за ним не директива.
    #[test]
    fn cpp_using_as_identifier_is_kept() {
        let p = prep_cpp("int main() { int using = 1; printf(\"%d\", using); }");
        assert!(p.parse.contains("int using = 1"), "{}", p.parse);
    }

    /// `constexpr` и `return {};` — обычный C++ в лабораторных заготовках.
    #[test]
    fn cpp_constexpr_and_empty_return() {
        let src = concat!(
            "constexpr int N = 3;\n",
            "int f(int x) {\n",
            "    if (x > N) {\n",
            "        return {};\n",
            "    }\n",
            "    return x;\n",
            "}\n",
            "int main() { printf(\"%d\", f(1)); return 0; }\n",
        );
        let p = prep_cpp(src);
        assert!(p.parse.contains("const     int N = 3;"), "{}", p.parse);
        assert!(p.parse.contains("return 0 ;"), "{}", p.parse);
        parse_cpp_ok(src);
    }
}
