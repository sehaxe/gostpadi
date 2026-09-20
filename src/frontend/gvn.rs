use crate::error::ParseError;
use crate::frontend::tile_kind;
use crate::ir::{Branch, LoopKind, Node, NodeKind, Stmt};
use crate::style::Style;

/// Строка ветки/тела → типизированный Stmt. Единственное место, где
/// текст ещё сниффится на выходы и ввод-вывод: дальше IR типизирован.
fn stmt_from_line(s: String) -> Stmt {
    let kw = s.trim();
    if kw == "break" {
        return Stmt::Break;
    }
    if kw == "continue" {
        return Stmt::Continue;
    }
    if is_return(kw) {
        return Stmt::Return(s);
    }
    Stmt::Tile {
        kind: tile_kind(&s),
        text: s,
    }
}

fn wrap(text: &str, limit: usize) -> String {
    let mut res: Vec<String> = Vec::new();
    for para in text.split('\n') {
        // работаем в char-домене: байтовые срезы рвут кириллицу
        let mut chars: Vec<char> = para.chars().collect();
        loop {
            if chars.len() <= limit {
                break;
            }
            let cut = chars[..limit]
                .iter()
                .rposition(|&c| c == ' ')
                .filter(|&p| p > 0)
                .unwrap_or(limit);
            let left: String = chars[..cut].iter().collect();
            res.push(left.trim_end().to_string());
            let rest: String = chars[cut..].iter().collect();
            chars = rest.trim_start().chars().collect();
            if chars.is_empty() {
                break;
            }
        }
        res.push(chars.iter().collect());
    }
    res.join("\n")
}

fn split_statements(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let ch = chars[i];
        if ch == '"' {
            let prev_is_bs = cur.chars().last() == Some('\\');
            if !prev_is_bs {
                in_str = !in_str;
            }
        }
        if !in_str {
            if ch == '(' || ch == '[' || ch == '{' {
                depth += 1;
            } else if ch == ')' || ch == ']' || ch == '}' {
                if depth > 0 {
                    depth -= 1;
                }
            } else if ch == ';' && depth == 0 {
                let t = cur.trim().to_string();
                if !t.is_empty() {
                    parts.push(t);
                }
                cur.clear();
                i += 1;
                continue;
            }
        }
        cur.push(ch);
        i += 1;
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        parts.push(t);
    }
    parts.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

fn cond_text(cond: &str) -> String {
    let c = cond.trim();
    if starts_with_word(c, "if") || starts_with_word(c, "switch") {
        return c.to_string();
    }
    format!("if ({})", c)
}

fn starts_with_word(s: &str, w: &str) -> bool {
    if let Some(rest) = s.strip_prefix(w) {
        if rest.is_empty() {
            return true;
        }
        let ch = rest.chars().next().unwrap();
        return !ch.is_alphanumeric() && ch != '_';
    }
    false
}

fn starts_with_if_or_switch(s: &str) -> bool {
    if let Some(r) = s.strip_prefix("if") {
        if r.is_empty() {
            return false;
        }
        let ch = r.chars().next().unwrap();
        return ch == ' ' || ch == '(';
    }
    if let Some(r) = s.strip_prefix("switch") {
        if r.is_empty() {
            return false;
        }
        let ch = r.chars().next().unwrap();
        return ch == ' ' || ch == '(';
    }
    false
}

fn starts_with_while_or_for(s: &str) -> bool {
    if let Some(r) = s.strip_prefix("while") {
        if r.is_empty() {
            return false;
        }
        let ch = r.chars().next().unwrap();
        return ch == ' ' || ch == '(';
    }
    if let Some(r) = s.strip_prefix("for") {
        if r.is_empty() {
            return false;
        }
        let ch = r.chars().next().unwrap();
        return ch == ' ' || ch == '(';
    }
    false
}

fn is_return(s: &str) -> bool {
    starts_with_word(s.trim_start(), "return")
}

fn strip_comments(line: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut in_str = false;
    let mut i = 0;
    while i < n {
        let ch = chars[i];
        if ch == '"' {
            let prev_is_bs = i > 0 && chars[i - 1] == '\\';
            if !prev_is_bs {
                in_str = !in_str;
            }
            out.push(ch);
            i += 1;
            continue;
        }
        if !in_str {
            if ch == '#' {
                break;
            }
            if ch == '/' && i + 1 < n && chars[i + 1] == '/' {
                break;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn indent_of(s: &str) -> usize {
    s.len() - s.trim_start_matches(' ').len()
}

fn find_branch_colon(s: &str) -> Option<usize> {
    for (idx, ch) in s.char_indices() {
        if ch == ':' {
            let left = &s[..idx];
            let left_trim = left.trim();
            if left_trim.is_empty() {
                continue;
            }
            if left_trim.contains(':')
                || left_trim.contains('(')
                || left_trim.contains(')')
                || left_trim.contains('"')
            {
                continue;
            }
            return Some(idx);
        }
    }
    None
}

fn strip_arrow_end(label: &str) -> Option<String> {
    let t = label.trim();
    if !t.ends_with("end") {
        return None;
    }
    let without_end = &t[..t.len() - 3].trim_end();
    if let Some(pos) = t.rfind("->") {
        let before_end_part = &t[pos + 2..t.len() - 3];
        if before_end_part.trim().is_empty() {
            let before = t[..pos].trim();
            if !before.is_empty() {
                return Some(before.to_string());
            }
        } else {
            // fallback for without_end handling
            let _ = without_end;
        }
    }
    None
}

fn extract_switch_var(cond: &str) -> Option<String> {
    if let Some(p) = cond.find("switch") {
        if let Some(rest) = cond[p..].strip_prefix("switch") {
            if let Some(l) = rest.find('(') {
                if let Some(r) = rest[l..].find(')') {
                    let inside = rest[l + 1..l + r].trim();
                    if !inside.is_empty() {
                        return Some(inside.to_string());
                    }
                }
            }
        }
    }
    None
}

fn lang_words(labels: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match labels {
        "ru" => ("начало", "конец", "да", "нет"),
        _ => ("Start", "End", "yes", "no"),
    }
}

struct Parser<'a> {
    lines: Vec<(usize, String)>,
    pos: usize,
    style: &'a Style,
    labels: String,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<(usize, &str)> {
        if self.pos < self.lines.len() {
            let (ln, s) = &self.lines[self.pos];
            Some((*ln, s.as_str()))
        } else {
            None
        }
    }
    fn take(&mut self) -> (usize, String) {
        let v = self.lines[self.pos].clone();
        self.pos += 1;
        v
    }

    fn block(&mut self, indent: usize) -> Result<Vec<Stmt>, ParseError> {
        let mut items: Vec<Stmt> = Vec::new();
        while let Some((_, line_ref)) = self.peek() {
            let ind = indent_of(line_ref);
            if ind < indent {
                break;
            }
            let (ln, line_str) = self.take();
            let ind_taken = indent_of(&line_str);
            if ind_taken > indent {
                return Err(ParseError::new("неожиданный отступ")
                    .with_line(ln)
                    .with_src(line_str.clone()));
            }
            let s = line_str.trim().to_string();
            if starts_with_if_or_switch(&s) {
                let node = self.decision(s, ln, indent)?;
                items.push(Stmt::Node(Box::new(node)));
            } else if starts_with_while_or_for(&s) {
                let node = self.cycle(s, ln, indent)?;
                items.push(Stmt::Node(Box::new(node)));
            } else {
                items.push(stmt_from_line(s));
            }
        }
        Ok(items)
    }

    fn decision(&mut self, s: String, lineno: usize, indent: usize) -> Result<Node, ParseError> {
        let kw_end = s.find(|c| c == '(' || c == ' ').unwrap_or(s.len());
        let kw = s[..kw_end].to_string();
        let rest = s.strip_prefix(&kw).unwrap_or(&s).trim().to_string();
        let cond_raw = if kw == "switch" {
            format!("switch {}", rest)
        } else {
            cond_text(&rest)
        };
        let cond = wrap(&cond_raw, self.style.cond_chars);

        let mut branches: Vec<Branch> = Vec::new();
        let mut cur_idx: Option<usize> = None;

        while let Some((_, top_line)) = self.peek() {
            let ind = indent_of(top_line);
            if ind < indent + 4 {
                break;
            }
            if ind > indent + 4 {
                let (ln, line_b) = self.peek().unwrap();
                return Err(ParseError::new("неожиданный отступ")
                    .with_line(ln)
                    .with_src(line_b.to_string()));
            }
            let (ln_b, line_b) = self.take();
            let s_b = line_b.trim().to_string();
            if let Some(colon_idx) = find_branch_colon(&s_b) {
                let label_part = s_b[..colon_idx].trim().to_string();
                let mut btext = s_b[colon_idx + 1..].trim().to_string();
                let mut to_end = false;
                let mut label = label_part.clone();
                if let Some(stripped) = strip_arrow_end(&label_part) {
                    label = stripped;
                    to_end = true;
                }
                if btext.ends_with("-> end") {
                    btext = btext[..btext.len() - "-> end".len()].trim_end().to_string();
                    to_end = true;
                }
                let stmts = split_statements(&btext)
                    .into_iter()
                    .map(stmt_from_line)
                    .collect();
                let br = Branch {
                    label,
                    stmts,
                    to_end,
                    link: None,
                };
                branches.push(br);
                cur_idx = Some(branches.len() - 1);
            } else {
                let idx = cur_idx.ok_or_else(|| {
                    ParseError::new("branch must look like «label: text»")
                        .with_line(ln_b)
                        .with_src(line_b.clone())
                })?;
                if starts_with_if_or_switch(&s_b) {
                    let node = self.decision(s_b, ln_b, indent + 4)?;
                    branches[idx].stmts.push(Stmt::Node(Box::new(node)));
                } else if starts_with_while_or_for(&s_b) {
                    let node = self.cycle(s_b, ln_b, indent + 4)?;
                    branches[idx].stmts.push(Stmt::Node(Box::new(node)));
                } else {
                    branches[idx].stmts.push(stmt_from_line(s_b));
                }
            }
        }

        // return handling and nested end check
        for br in branches.iter_mut() {
            let has_return = br.stmts.iter().any(|st| matches!(st, Stmt::Return(_)));
            if has_return {
                br.to_end = false;
            }
            no_end_inside(&br.stmts)?;
        }

        if branches.is_empty() {
            return Err(ParseError::new(format!("«{}» has no branches", kw))
                .with_line(lineno)
                .with_src(s.clone()));
        }

        if cond.starts_with("switch") {
            let svar = extract_switch_var(&cond);
            let mut fixed: Vec<Branch> = Vec::new();
            for mut br in branches {
                let low = br.label.trim().to_lowercase();
                if matches!(low.as_str(), "default" | "else" | "otherwise" | "иначе") {
                    br.label = "default".to_string();
                } else if let Some(ref sv) = svar {
                    // порт Python re.fullmatch(r"(?:case\s+)?(.+)"): срезается
                    // только нижнерегистровое «case» с обязательным пробелом
                    let trimmed = br.label.trim();
                    let rest = match trimmed.strip_prefix("case") {
                        Some(r) if r.chars().next().is_some_and(char::is_whitespace) => r.trim(),
                        _ => trimmed,
                    };
                    br.label = format!("{} = {}", sv, rest);
                }
                fixed.push(br);
            }
            // case-алиасы: «3:» с пустым телом перед «4: body» — метка «3, 4»
            if svar.is_some() {
                super::merge_case_aliases(&mut fixed);
            }
            branches = fixed;
        }

        if branches.len() == 1 && !branches[0].stmts.is_empty() && !cond.starts_with("switch") {
            let (_, _, _, no_word) = lang_words(&self.labels);
            branches.push(Branch {
                label: no_word.to_string(),
                stmts: Vec::new(),
                to_end: false,
                link: None,
            });
        }

        let mut nd = Node::new(NodeKind::Decision, cond.clone());
        nd.branches = branches;
        nd.lang = self.labels.clone();
        if cond.starts_with("switch") {
            nd.switch_var = extract_switch_var(&cond);
        }
        Ok(nd)
    }

    fn cycle(&mut self, s: String, _lineno: usize, indent: usize) -> Result<Node, ParseError> {
        let kw_end = s.find(|c| c == '(' || c == ' ').unwrap_or(s.len());
        let kw = s[..kw_end].to_string();
        let mut rest = s.strip_prefix(&kw).unwrap_or(&s).trim().to_string();
        if rest.starts_with('(') && rest.ends_with(')') && rest.len() >= 2 {
            rest = rest[1..rest.len() - 1].trim().to_string();
        }
        let text = wrap(&rest, self.style.max_chars);
        let loop_kind = if kw == "while" {
            Some(LoopKind::While)
        } else {
            Some(LoopKind::For)
        };
        let mut nd = Node::new(NodeKind::Loop, text);
        nd.loop_kind = loop_kind;
        nd.lang = self.labels.clone();

        if let Some((_, top_line)) = self.peek() {
            let ind = indent_of(top_line);
            if ind > indent {
                let body = self.block(ind)?;
                nd.body = Some(body);
            }
        }
        Ok(nd)
    }
}

fn no_end_inside(stmts: &[Stmt]) -> Result<(), ParseError> {
    for st in stmts {
        if let Stmt::Node(node) = st {
            match node.kind {
                NodeKind::Decision => {
                    for br in &node.branches {
                        if br.to_end {
                            return Err(ParseError::new(
                                "«-> конец» внутри вложенной ветки не поддерживается",
                            ));
                        }
                        no_end_inside(&br.stmts)?;
                    }
                }
                NodeKind::Loop => {
                    if let Some(body) = &node.body {
                        no_end_inside(body)?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn parse(text: &str, style: &Style, labels: &str) -> Result<Vec<Node>, ParseError> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines: Vec<(usize, String)> = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let lineno = idx + 1;
        let stripped = strip_comments(raw);
        let expanded = stripped.replace('\t', "    ");
        let body = expanded.trim_end().to_string();
        if body.trim().is_empty() {
            continue;
        }
        lines.push((lineno, body));
    }

    let mut parser = Parser {
        lines,
        pos: 0,
        style,
        labels: labels.to_string(),
    };

    let (start_txt, end_txt, _, _) = lang_words(labels);
    let mut nodes: Vec<Node> = Vec::new();
    nodes.push(Node::new(NodeKind::Term, start_txt));

    while parser.pos < parser.lines.len() {
        let (lineno, line) = parser.take();
        let stripped = line.trim().to_string();
        let indent = indent_of(&line);
        if stripped.starts_with('@') && indent == 0 {
            return Err(ParseError::new(format!("unknown directive «{}»", stripped))
                .with_line(lineno)
                .with_src(line.clone()));
        }
        if indent >= 4 {
            return Err(
                ParseError::new("indentation is only allowed inside an «if» block")
                    .with_line(lineno)
                    .with_src(line.clone()),
            );
        }
        if starts_with_if_or_switch(&stripped) {
            let nd = parser.decision(stripped, lineno, 0)?;
            nodes.push(nd);
            continue;
        }
        if is_return(&stripped) {
            nodes.push(Node::new(
                NodeKind::Return,
                wrap(&stripped, style.max_chars),
            ));
            continue;
        }
        if starts_with_while_or_for(&stripped) {
            let nd = parser.cycle(stripped, lineno, 0)?;
            nodes.push(nd);
            continue;
        }
        // input/output/action
        let mut handled = false;
        if let Some(rest) = stripped.strip_prefix("input") {
            if !rest.is_empty() && rest.chars().next().unwrap().is_whitespace() {
                let after = rest.trim_start();
                let first = after.chars().next();
                if let Some(c) = first {
                    if !matches!(c, '=' | '+' | '-' | '*' | '/') {
                        let txt = after.to_string();
                        nodes.push(Node::new(NodeKind::Io, wrap(&txt, style.max_chars)));
                        handled = true;
                    }
                } else {
                    handled = true;
                }
            }
        }
        if !handled {
            if let Some(rest) = stripped.strip_prefix("output") {
                if !rest.is_empty() && rest.chars().next().unwrap().is_whitespace() {
                    let after = rest.trim_start();
                    let first = after.chars().next();
                    if let Some(c) = first {
                        if !matches!(c, '=' | '+' | '-' | '*' | '/') {
                            let txt = after.to_string();
                            nodes.push(Node::new(NodeKind::Io, wrap(&txt, style.max_chars)));
                            handled = true;
                        }
                    } else {
                        handled = true;
                    }
                }
            }
        }
        if !handled {
            if let Some(rest) = stripped.strip_prefix("action") {
                if !rest.is_empty() && rest.chars().next().unwrap().is_whitespace() {
                    let after = rest.trim_start();
                    nodes.push(Node::new(NodeKind::Act, wrap(after, style.max_chars)));
                    handled = true;
                }
            }
        }
        if handled {
            continue;
        }
        if style.is_io(&stripped) {
            nodes.push(Node::new(NodeKind::Io, wrap(&stripped, style.max_chars)));
        } else {
            nodes.push(Node::new(NodeKind::Act, wrap(&stripped, style.max_chars)));
        }
    }

    nodes.push(Node::new(NodeKind::Term, end_txt));
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::TileKind;
    use crate::style::Style;

    /// Граница типизации: строки .gvn превращаются в типизированный Stmt
    /// ровно один раз — здесь.
    #[test]
    fn typed_exits_and_tiles_from_lines() {
        let st = Style::default();
        let text = "if a > 0\n    да:\n    break\n    нет:\n    printf(\"x\")\n";
        let nodes = parse(text, &st, "").unwrap();
        let d = nodes.iter().find(|n| n.kind == NodeKind::Decision).unwrap();
        assert!(
            matches!(d.branches[0].stmts[0], Stmt::Break),
            "break → Stmt::Break"
        );
        assert!(matches!(
            d.branches[1].stmts[0],
            Stmt::Tile {
                kind: TileKind::Io,
                ..
            }
        ));

        let text = "if a > 0\n    да:\n    return 1\n    нет:\n    b = 2\n";
        let nodes = parse(text, &st, "").unwrap();
        let d = nodes.iter().find(|n| n.kind == NodeKind::Decision).unwrap();
        assert!(matches!(d.branches[0].stmts[0], Stmt::Return(_)));
        assert!(!d.branches[0].to_end, "return отменяет «-> конец»");
        assert!(matches!(
            d.branches[1].stmts[0],
            Stmt::Tile {
                kind: TileKind::Act,
                ..
            }
        ));

        let text = "while a > 0\n    continue\noutput printf(1)\n";
        let nodes = parse(text, &st, "").unwrap();
        let body = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Loop)
            .unwrap()
            .body
            .as_ref()
            .unwrap();
        assert!(matches!(body[0], Stmt::Continue));
    }

    fn en_style() -> Style {
        Style::default()
    }

    const CASES: &[(&str, &str)] = &[
        ("линейная", "a = 1\nb = 2\nc = a + b"),
        ("ввод-вывод авто", "scanf(\"%d\", &x)\nprintf(\"%d\", x)"),
        ("if/else обе ветки", "if a > 0\n    да: printf(\"плюс\")\n    нет: printf(\"минус\")"),
        ("if одна ветка", "if a > 0\n    да: printf(\"плюс\")"),
        ("if -> конец + нет + поток", "input scanf(\"%d\", &a)\nif scanf != 1\n    да -> end: printf(\"err\"); return 1\n    нет:\n b = a * 2\noutput printf(\"%d\", b)"),
        ("switch 2", "if switch (x)\n    1: printf(\"один\")\n    2: printf(\"два\")"),
        ("switch 3", "if switch (a)\n    1: printf(y); break\n    2: printf(z); break\n    иначе: a = 10; break"),
        ("перенос длинных строк", "b = a / 100000 + a / 10000 % 10 + a / 1000 % 10 + a % 10"),
        ("комментарии", "// привет\ninput scanf(\"%d\", &a) # хвост\n action b = 1"),
        ("while", "a = 0\nwhile a < 5\n    a = a + 1\noutput printf(a)"),
        ("for", "for i = 0; i < 5; i = i + 1\n    s = s + i\noutput printf(s)"),
        ("цикл с if в теле", "s = 0\nfor i = 0; i < 5; i = i + 1\n    if i % 2 == 0\n        да: s = s + i\n        нет: s = s + 1\noutput printf(s)"),
        ("вложенный if в ветке", "input scanf(\"%d\", &a)\nif scanf != 1\n    да: printf(\"Ошибка ввода!\")\n    нет:\n b = a * 2\nif b > 10\n    да: printf(\"большое\")\n    нет: printf(\"маленькое\")\noutput printf(b)"),
        ("английские ключевые слова", "input scanf(\"%d\", &a)\nif a > 0\n    yes: printf(\"plus\")\noutput printf(a)"),
    ];

    #[test]
    fn parses_all_cases() {
        let style = en_style();
        for (name, text) in CASES {
            let res = parse(text, &style, "en");
            assert!(res.is_ok(), "case '{}' failed: {:?}", name, res.err());
            let nodes = res.unwrap();
            assert!(
                nodes.len() >= 2,
                "case '{}' should have at least Start/End",
                name
            );
            assert_eq!(nodes[0].kind, NodeKind::Term);
            assert_eq!(nodes.last().unwrap().kind, NodeKind::Term);
        }
    }

    #[test]
    fn russian_keywords_rejected_via_indent() {
        let style = en_style();
        let text = "если a > 0\n    да: printf(\"плюс\")\n";
        let res = parse(text, &style, "en");
        assert!(res.is_err(), "russian 'если' should cause indent error");
    }

    #[test]
    fn switch_no_branch_error() {
        let style = en_style();
        let text = "if switch (x)\n";
        let res = parse(text, &style, "en");
        assert!(res.is_err());
    }

    #[test]
    fn unknown_directive() {
        let style = en_style();
        let text = "@unknown\n";
        let res = parse(text, &style, "en");
        assert!(res.is_err());
    }

    #[test]
    fn bad_indent_at_top() {
        let style = en_style();
        let text = "    a = 1\n";
        let res = parse(text, &style, "en");
        assert!(res.is_err());
    }

    #[test]
    fn wrap_splits() {
        let w = wrap("a b c d e", 3);
        assert!(w.contains('\n'));
    }

    /// Кириллица: байтовый rfind(' ') в chars-срезе паниковал
    /// (byte index 58 при 30 символах) и рвал строки посреди символа.
    #[test]
    fn wrap_cyrillic_no_panic() {
        let s = "а".repeat(29) + " б";
        let w = wrap(&s, 30);
        for part in w.split('\n') {
            assert!(part.chars().count() <= 30, "часть длиннее лимита: {part:?}");
        }
        assert_eq!(w.split('\n').count(), 2, "{w:?}");
        assert_eq!(w, format!("{}\nб", "а".repeat(29)));
    }

    /// case-алиас «3:» с пустым телом перед «4: body» (схема
    /// 11-switch-alias.gvn): проваливание в C — метки склеиваются,
    /// пустой ветки и рельсы обхода нет. Обычный if не трогается.
    #[test]
    fn switch_case_alias_merges_into_next() {
        let style = en_style();
        let text = "if switch (k)\n    1: printf(\"раз\"); break\n    2: printf(\"два\"); break\n    3:\n    4: printf(\"три-четыре\"); break\n    иначе: printf(\"много\"); break\noutput printf(k)";
        let nodes = parse(text, &style, "en").unwrap();
        let sw = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Decision && n.switch_var.is_some())
            .unwrap();
        assert_eq!(sw.branches.len(), 4, "пустая «3» слилась с «4»");
        assert!(
            sw.branches.iter().any(|b| b.label == "k = 3, 4"),
            "метки склеены: {:?}",
            sw.branches.iter().map(|b| &b.label).collect::<Vec<_>>()
        );
        assert!(
            sw.branches.iter().all(|b| !b.stmts.is_empty()),
            "empty-веток у switch 0"
        );
        // обычный if: пустая «нет» — осмысленная рельса, не merge
        let ifs = parse("if a > 0\n    да: a = 1\n    нет:\n", &style, "en").unwrap();
        let d = ifs.iter().find(|n| n.kind == NodeKind::Decision).unwrap();
        assert_eq!(d.branches[1].label, "нет");
        assert!(d.branches[1].stmts.is_empty());
    }

    /// Порт Python `(?:case\s+)?`: «case» без пробела не метка кейса,
    /// регистр значим — «caseless» и «CASE 7» остаются значениями.
    #[test]
    fn switch_labels_case_prefix_exact() {
        let style = en_style();
        let text = "if switch (x)\n    caseless: a = 1\n    CASE 7: b = 2\n    case\t8: c = 3\n";
        let nodes = parse(text, &style, "en").unwrap();
        let sw = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Decision && n.switch_var.is_some())
            .unwrap();
        let labels: Vec<&str> = sw.branches.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["x = caseless", "x = CASE 7", "x = 8"]);
    }

    #[test]
    fn split_statements_basic() {
        let v = split_statements("a; b; c");
        assert_eq!(v, vec!["a", "b", "c"]);
        let v2 = split_statements("printf(\"a; b\"); return 1");
        assert_eq!(v2.len(), 2);
    }
}
