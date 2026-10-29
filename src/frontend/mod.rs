pub mod c;
pub mod cpp;
#[cfg(feature = "autofix")]
pub mod fix;
pub mod gvn;

use crate::error::ParseError;
use crate::ir::{Branch, Node, NodeKind, TileKind};
use crate::style::Style;
use std::sync::atomic::{AtomicBool, Ordering};

/// Язык кода, который читает C-фронтенд. C++ идёт через ту же грамматику
/// после нормализации, но набор эвристик у него свой.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    C,
    Cpp,
}

/// Что лежит на входе: текст схемы или код (C/C++).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Gvn,
    C,
    Cpp,
}

impl InputKind {
    pub fn lang(self) -> Option<Lang> {
        match self {
            InputKind::Gvn => None,
            InputKind::C => Some(Lang::C),
            InputKind::Cpp => Some(Lang::Cpp),
        }
    }
}

pub const C_EXTENSIONS: &[&str] = &["c", "h"];
pub const CPP_EXTENSIONS: &[&str] = &["cpp", "cc", "cxx", "c++", "hpp", "hxx", "hh", "h++"];

/// Расширение файла -> вид входа. Регистр не важен: на Windows и macOS
/// `.CPP` встречается так же часто.
pub fn input_kind(path: &str) -> InputKind {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if CPP_EXTENSIONS.contains(&ext.as_str()) {
        InputKind::Cpp
    } else if C_EXTENSIONS.contains(&ext.as_str()) {
        InputKind::C
    } else {
        InputKind::Gvn
    }
}

/// Расширения входа, которые разбираются как код C/C++ (остальное — .gvn).
pub fn is_code_path(path: &str) -> bool {
    input_kind(path) != InputKind::Gvn
}

/// Одна схема, которую отдал фронтенд: функция файла или весь `.gvn`.
/// `name`/`signature` — только у функций кода; у `.gvn` их нет.
pub struct ParsedFunction {
    pub name: Option<String>,
    pub signature: Option<String>,
    pub nodes: Vec<Node>,
}

/// Общий контракт фронтендов: код C/C++ и текст `.gvn` дают один IR.
pub trait Frontend {
    fn parse(
        &self,
        src: &str,
        style: &Style,
        labels: &str,
    ) -> Result<Vec<ParsedFunction>, ParseError>;
}

/// Код C/C++: все функции файла, у каждой своя схема.
pub struct CodeFrontend {
    pub lang: Lang,
}

impl Frontend for CodeFrontend {
    fn parse(
        &self,
        src: &str,
        style: &Style,
        labels: &str,
    ) -> Result<Vec<ParsedFunction>, ParseError> {
        let mut funcs = c::parse_functions_full(src, labels, self.lang)?;
        let (start, end, _, _) = gvn::lang_words(labels);
        for (_, _, nodes) in &mut funcs {
            gvn::wrap_nodes(nodes, style);
            let mut with_terms = Vec::with_capacity(nodes.len() + 2);
            with_terms.push(Node::new(NodeKind::Term, start));
            with_terms.append(nodes);
            with_terms.push(Node::new(NodeKind::Term, end));
            *nodes = with_terms;
        }
        Ok(funcs
            .into_iter()
            .map(|(name, signature, nodes)| ParsedFunction {
                name: Some(name),
                signature: Some(signature),
                nodes,
            })
            .collect())
    }
}

/// Текст `.gvn`: одна схема на файл.
pub struct GvnFrontend;

impl Frontend for GvnFrontend {
    fn parse(
        &self,
        src: &str,
        style: &Style,
        labels: &str,
    ) -> Result<Vec<ParsedFunction>, ParseError> {
        Ok(vec![ParsedFunction {
            name: None,
            signature: None,
            nodes: gvn::parse(src, style, labels)?,
        }])
    }
}

/// Мягкая нормализация входа: включена в сборке с feature `autofix` и до
/// первого `--no-fix`. Флаг, а не поле Options: он один на процесс и не
/// должен прорастать через пайплайн, serve и wasm.
static AUTOFIX: AtomicBool = AtomicBool::new(true);

/// `--no-fix` на один запуск: строгий разбор.
pub fn set_autofix(on: bool) {
    AUTOFIX.store(on, Ordering::Relaxed);
}

pub fn autofix_enabled() -> bool {
    cfg!(feature = "autofix") && AUTOFIX.load(Ordering::Relaxed)
}

/// Маркер ручного переноса листа: комментарий `// перенос` в C и строка
/// «перенос» в .gvn. В IR это conn-узел с этим текстом, в split — точка
/// форсированного разрыва; до SVG он не доезжает никогда.
pub const MARKER: &str = "перенос";

/// Единственная граница строкового распознавания ввода-вывода: оба
/// frontend'а зовут её при строительстве плитки; layout и measure
/// читают готовый kind и текст не смотрят.
pub(crate) fn tile_kind(text: &str) -> TileKind {
    if crate::style::Style::DEFAULT.is_io(text) {
        TileKind::Io
    } else {
        TileKind::Act
    }
}

/// C-проваливание: пустая ветка case перед непустой — алиас («3:» перед
/// «4: тело» выполняет тело, а не рисуется рельсой обхода). Метки
/// склеиваются через «, », пустая ветка исчезает. Последняя пустая
/// (или пустая с «-> конец») остаётся как была.
pub(crate) fn merge_case_aliases(branches: &mut Vec<Branch>) {
    for i in (0..branches.len().saturating_sub(1)).rev() {
        let empty_fall = branches[i].stmts.is_empty()
            && !branches[i].to_end
            && !branches[i + 1].stmts.is_empty();
        if empty_fall {
            let label = std::mem::take(&mut branches[i].label);
            let next = std::mem::take(&mut branches[i + 1].label);
            // одинаковый префикс «svar = » не дублируется: «k = 3» + «k = 4»
            // → «k = 3, 4», а не «k = 3, k = 4»
            branches[i + 1].label = match (label.split_once(" = "), next.split_once(" = ")) {
                (Some((p1, v1)), Some((p2, v2))) if p1 == p2 => format!("{p1} = {v1}, {v2}"),
                _ => format!("{label}, {next}"),
            };
            branches.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{input_kind, is_code_path, InputKind, Lang};

    /// C, C++ и заголовки — код; всё остальное, включая .gvn, — текст схемы.
    #[test]
    fn code_extensions() {
        for p in ["main.c", "main.h", "DIR/MAIN.C"] {
            assert!(is_code_path(p), "{p} обязан быть кодом");
            assert_eq!(input_kind(p), InputKind::C, "{p}");
        }
        for p in [
            "main.cpp",
            "main.cc",
            "main.cxx",
            "main.c++",
            "main.hpp",
            "main.hxx",
            "main.hh",
            "main.h++",
            "DIR/MAIN.CPP",
        ] {
            assert!(is_code_path(p), "{p} обязан быть кодом");
            assert_eq!(input_kind(p), InputKind::Cpp, "{p}");
        }
        for p in ["scheme.gvn", "scheme.txt", "main", "main.rs", ".cpp"] {
            assert!(!is_code_path(p), "{p} не код");
            assert_eq!(input_kind(p), InputKind::Gvn, "{p}");
        }
        assert_eq!(InputKind::C.lang(), Some(Lang::C));
        assert_eq!(InputKind::Cpp.lang(), Some(Lang::Cpp));
        assert_eq!(InputKind::Gvn.lang(), None);
    }
}
