#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Term,
    Io,
    Act,
    Decision,
    Loop,
    Return,
    Conn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopKind {
    While,
    For,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub label: String,
    pub stmts: Vec<Stmt>,
    pub to_end: bool,
    pub link: Option<char>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Text(String),
    Node(Box<Node>),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub text: String,
    pub branches: Vec<Branch>,
    pub body: Option<Vec<Stmt>>,
    pub loop_kind: Option<LoopKind>,
    pub lang: String,
    pub switch_var: Option<String>,
}

impl Node {
    pub fn new(kind: NodeKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            branches: Vec::new(),
            body: None,
            loop_kind: None,
            lang: "en".to_string(),
            switch_var: None,
        }
    }
}
