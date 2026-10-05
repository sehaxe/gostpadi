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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    While,
    For,
    /// тело выполняется до проверки условия. Рисуется тем же
    /// шестиугольником «подготовка», что и остальные циклы, но с
    /// подписью `do while`: в C условие проверяется ПОСЛЕ тела, и
    /// без подписи шестиугольник врал бы, что проверка идёт первой.
    DoWhile,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub label: String,
    pub stmts: Vec<Stmt>,
    pub to_end: bool,
    pub link: Option<char>,
}

/// Тип обычной плитки: процесс или ввод-вывод (ГОСТ 19.701).
/// Решается один раз — при строительстве IR во frontend'е.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    Act,
    Io,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// обычная плитка-оператор
    Tile {
        kind: TileKind,
        text: String,
    },
    /// выход из внутреннего цикла или switch: рельса или растворение
    Break,
    /// следующая итерация внутреннего цикла
    Continue,
    /// тупик-плитка return с исходным текстом
    Return(String),
    Node(Box<Node>),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub text: String,
    pub branches: Vec<Branch>,
    pub body: Option<Vec<Stmt>>,
    pub loop_kind: Option<LoopKind>,
    pub switch_var: Option<String>,
    /// Продолжение цикла с прошлого листа: верхняя трапеция уже
    /// нарисована там, рисуем только тело и нижнюю.
    pub cont: bool,
    /// Тело уходит на следующий лист: нижнюю трапецию не рисуем,
    /// вместо неё кружок-соединитель.
    pub cont_out: bool,
}

impl From<TileKind> for NodeKind {
    fn from(k: TileKind) -> Self {
        match k {
            TileKind::Io => NodeKind::Io,
            TileKind::Act => NodeKind::Act,
        }
    }
}

impl Node {
    /// Текст блока «подготовка»: ключевое слово цикла + условие,
    /// чтобы на схеме было видно, какой это цикл.
    pub fn loop_label(&self) -> String {
        match self.loop_kind {
            Some(LoopKind::While) => format!("while {}", self.text),
            Some(LoopKind::For) => format!("for {}", self.text),
            Some(LoopKind::DoWhile) => format!("do while {}", self.text),
            None => self.text.clone(),
        }
    }

    pub fn new(kind: NodeKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            branches: Vec::new(),
            body: None,
            loop_kind: None,
            switch_var: None,
            cont: false,
            cont_out: false,
        }
    }

    /// Кусок цикла, разорванного по листам.
    pub fn continued(mut self, from_prev: bool, to_next: bool) -> Self {
        self.cont = from_prev;
        self.cont_out = to_next;
        self
    }
}
