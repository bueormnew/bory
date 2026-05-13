use crate::span::Span;

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpr {
    pub name: String,
    pub args: Vec<TypeExpr>,
}

impl TypeExpr {
    pub fn simple(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: Vec::new(),
        }
    }

    pub fn render(&self) -> String {
        if self.args.is_empty() {
            self.name.clone()
        } else {
            let args = self
                .args
                .iter()
                .map(TypeExpr::render)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}[{args}]", self.name)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub type_hint: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: String,
    pub type_hint: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Literal(Literal),
    Variable(String),
    List(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Unary {
        op: UnaryOp,
        right: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f64),
    String(String),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    And,
    Or,
    In,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Var {
        name: String,
        type_hint: Option<TypeExpr>,
        initializer: Option<Expr>,
    },
    Assign {
        target: AssignTarget,
        op: AssignOp,
        value: Expr,
    },
    Expr(Expr),
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    ForIn {
        name: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    ForRange {
        name: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    Task {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
    },
    TypeDecl {
        flavor: TypeFlavor,
        name: String,
        fields: Vec<FieldDecl>,
        body: Vec<Stmt>,
    },
    Use {
        spec: String,
        alias: String,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    Load(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFlavor {
    Struct,
    Class,
}

#[derive(Debug, Clone)]
pub enum AssignTarget {
    Variable(String),
    Index { object: Expr, index: Expr },
    Member { object: Expr, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}
