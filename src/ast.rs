// Bang 프로그래밍 언어 — 추상 구문 트리 (AST)

use crate::lexer::token::Span;
use std::fmt;

// =============================================================================
// 최상위
// =============================================================================

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

// =============================================================================
// 타입 힌트 (선택적, 런타임 검증)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeAnn {
    Int,
    Float,
    Bool,
    Str,
    Nil,
    List,
    Map,
    Fn,
    Any,
}

impl TypeAnn {
    /// 타입 이름 문자열 → TypeAnn (예약된 타입 이름).
    pub fn from_name(s: &str) -> Option<TypeAnn> {
        match s {
            "int" => Some(TypeAnn::Int),
            "float" => Some(TypeAnn::Float),
            "bool" => Some(TypeAnn::Bool),
            "str" => Some(TypeAnn::Str),
            "nil" => Some(TypeAnn::Nil),
            "list" => Some(TypeAnn::List),
            "map" => Some(TypeAnn::Map),
            "fn" => Some(TypeAnn::Fn),
            "any" => Some(TypeAnn::Any),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TypeAnn::Int => "int",
            TypeAnn::Float => "float",
            TypeAnn::Bool => "bool",
            TypeAnn::Str => "str",
            TypeAnn::Nil => "nil",
            TypeAnn::List => "list",
            TypeAnn::Map => "map",
            TypeAnn::Fn => "fn",
            TypeAnn::Any => "any",
        }
    }

    /// 바이트코드 피연산자용 태그.
    pub fn as_u8(self) -> u8 {
        match self {
            TypeAnn::Int => 0,
            TypeAnn::Float => 1,
            TypeAnn::Bool => 2,
            TypeAnn::Str => 3,
            TypeAnn::Nil => 4,
            TypeAnn::List => 5,
            TypeAnn::Map => 6,
            TypeAnn::Fn => 7,
            TypeAnn::Any => 8,
        }
    }

    pub fn from_u8(b: u8) -> Option<TypeAnn> {
        Some(match b {
            0 => TypeAnn::Int,
            1 => TypeAnn::Float,
            2 => TypeAnn::Bool,
            3 => TypeAnn::Str,
            4 => TypeAnn::Nil,
            5 => TypeAnn::List,
            6 => TypeAnn::Map,
            7 => TypeAnn::Fn,
            8 => TypeAnn::Any,
            _ => return None,
        })
    }
}

// =============================================================================
// 블록
// =============================================================================

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

// =============================================================================
// 문(Statement)
// =============================================================================

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `let name [: type] = value`
    Let { name: String, ty: Option<TypeAnn>, value: Expr },
    /// 표현식 문장
    Expr(Expr),
    /// `return [expr]`
    Return(Option<Expr>),
    /// `if cond { then } [else { else_ }]`
    If { cond: Expr, then: Block, else_: Option<Block> },
    /// `while cond { body }`
    While { cond: Expr, body: Block },
    /// `for var in iter { body }`
    For { var: String, iter: Expr, body: Block },
    /// 독립 블록 `{ ... }`
    Block(Block),
    /// `parallel { ... }`
    Parallel(Block),
    Break,
    Continue,
    /// `try { body } catch name { handler }`
    Try { body: Block, catch_var: String, handler: Block },
    /// `throw expr`
    Throw(Expr),
}

// =============================================================================
// 식(Expression)
// =============================================================================

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// 맵 리터럴 키: 문자열 리터럴 또는 식별자(→문자열 키)
#[derive(Debug, Clone)]
pub enum MapKey {
    Str(String),
    Ident(String),
}

impl MapKey {
    pub fn as_str(&self) -> &str {
        match self {
            MapKey::Str(s) | MapKey::Ident(s) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, // -
    Not, // not
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Not => write!(f, "not"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Eq  => "==",
            BinaryOp::Ne  => "!=",
            BinaryOp::Lt  => "<",
            BinaryOp::Le  => "<=",
            BinaryOp::Gt  => ">",
            BinaryOp::Ge  => ">=",
            BinaryOp::And => "and",
            BinaryOp::Or  => "or",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // 리터럴
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,
    // 변수
    Ident(String),
    // 컬렉션
    List(Vec<Expr>),
    Map(Vec<(MapKey, Expr)>),
    // 연산
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    // 후위
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Index { target: Box<Expr>, index: Box<Expr> },
    Field { target: Box<Expr>, name: String },
    // 함수 / 클로저
    // name: Some("foo") 이면 본문 안에서 재귀 가능 (named fn 디슈가)
    // param_types/ret_type: 선택적 타입 힌트 (런타임 검증, None=미지정).
    // param_types는 params와 길이가 같다.
    Function {
        name: Option<String>,
        params: Vec<String>,
        param_types: Vec<Option<TypeAnn>>,
        ret_type: Option<TypeAnn>,
        body: Block,
    },
    // 동시성
    Spawn(Box<Expr>),
    // 대입 (target: Ident / Index / Field 만 허용)
    Assign { target: Box<Expr>, value: Box<Expr> },
}

// =============================================================================
// Pretty Printer
// =============================================================================

pub fn dump_program(prog: &Program) -> String {
    let mut out = String::new();
    for stmt in &prog.stmts {
        dump_stmt(&mut out, stmt, 0);
    }
    out
}

fn indent(n: usize) -> String {
    "  ".repeat(n)
}

fn dump_stmt(out: &mut String, stmt: &Stmt, depth: usize) {
    let pad = indent(depth);
    match &stmt.kind {
        StmtKind::Let { name, ty, value } => {
            let tystr = ty.map(|t| format!(": {}", t.name())).unwrap_or_default();
            out.push_str(&format!("{pad}Let {name}{tystr} [{}:{}]\n", stmt.span.line, stmt.span.col));
            dump_expr(out, value, depth + 1);
        }
        StmtKind::Expr(expr) => {
            out.push_str(&format!("{pad}ExprStmt [{}:{}]\n", stmt.span.line, stmt.span.col));
            dump_expr(out, expr, depth + 1);
        }
        StmtKind::Return(val) => {
            out.push_str(&format!("{pad}Return [{}:{}]\n", stmt.span.line, stmt.span.col));
            if let Some(v) = val {
                dump_expr(out, v, depth + 1);
            }
        }
        StmtKind::If { cond, then, else_ } => {
            out.push_str(&format!("{pad}If [{}:{}]\n", stmt.span.line, stmt.span.col));
            out.push_str(&format!("{pad}  cond:\n"));
            dump_expr(out, cond, depth + 2);
            out.push_str(&format!("{pad}  then:\n"));
            dump_block(out, then, depth + 2);
            if let Some(el) = else_ {
                out.push_str(&format!("{pad}  else:\n"));
                dump_block(out, el, depth + 2);
            }
        }
        StmtKind::While { cond, body } => {
            out.push_str(&format!("{pad}While [{}:{}]\n", stmt.span.line, stmt.span.col));
            out.push_str(&format!("{pad}  cond:\n"));
            dump_expr(out, cond, depth + 2);
            out.push_str(&format!("{pad}  body:\n"));
            dump_block(out, body, depth + 2);
        }
        StmtKind::For { var, iter, body } => {
            out.push_str(&format!("{pad}For {var} in [{}:{}]\n", stmt.span.line, stmt.span.col));
            dump_expr(out, iter, depth + 1);
            out.push_str(&format!("{pad}  body:\n"));
            dump_block(out, body, depth + 2);
        }
        StmtKind::Block(block) => {
            out.push_str(&format!("{pad}Block [{}:{}]\n", stmt.span.line, stmt.span.col));
            dump_block(out, block, depth + 1);
        }
        StmtKind::Parallel(block) => {
            out.push_str(&format!("{pad}Parallel [{}:{}]\n", stmt.span.line, stmt.span.col));
            dump_block(out, block, depth + 1);
        }
        StmtKind::Break => {
            out.push_str(&format!("{pad}Break [{}:{}]\n", stmt.span.line, stmt.span.col));
        }
        StmtKind::Continue => {
            out.push_str(&format!("{pad}Continue [{}:{}]\n", stmt.span.line, stmt.span.col));
        }
        StmtKind::Try { body, catch_var, handler } => {
            out.push_str(&format!("{pad}Try [{}:{}]\n", stmt.span.line, stmt.span.col));
            out.push_str(&format!("{pad}  body:\n"));
            dump_block(out, body, depth + 2);
            out.push_str(&format!("{pad}  catch {catch_var}:\n"));
            dump_block(out, handler, depth + 2);
        }
        StmtKind::Throw(expr) => {
            out.push_str(&format!("{pad}Throw [{}:{}]\n", stmt.span.line, stmt.span.col));
            dump_expr(out, expr, depth + 1);
        }
    }
}

fn dump_block(out: &mut String, block: &Block, depth: usize) {
    for stmt in &block.stmts {
        dump_stmt(out, stmt, depth);
    }
}

fn dump_expr(out: &mut String, expr: &Expr, depth: usize) {
    let pad = indent(depth);
    match &expr.kind {
        ExprKind::Int(n) => out.push_str(&format!("{pad}Int({n})\n")),
        ExprKind::Float(n) => out.push_str(&format!("{pad}Float({n})\n")),
        ExprKind::Str(s) => out.push_str(&format!("{pad}Str({s:?})\n")),
        ExprKind::Bool(b) => out.push_str(&format!("{pad}Bool({b})\n")),
        ExprKind::Nil => out.push_str(&format!("{pad}Nil\n")),
        ExprKind::Ident(name) => out.push_str(&format!("{pad}Ident({name})\n")),
        ExprKind::List(items) => {
            out.push_str(&format!("{pad}List[{}]\n", items.len()));
            for item in items {
                dump_expr(out, item, depth + 1);
            }
        }
        ExprKind::Map(entries) => {
            out.push_str(&format!("{pad}Map{{{}}}\n", entries.len()));
            for (key, val) in entries {
                out.push_str(&format!("{pad}  key: {:?}\n", key.as_str()));
                dump_expr(out, val, depth + 2);
            }
        }
        ExprKind::Unary { op, expr } => {
            out.push_str(&format!("{pad}Unary({op})\n"));
            dump_expr(out, expr, depth + 1);
        }
        ExprKind::Binary { op, left, right } => {
            out.push_str(&format!("{pad}Binary({op})\n"));
            dump_expr(out, left, depth + 1);
            dump_expr(out, right, depth + 1);
        }
        ExprKind::Call { callee, args } => {
            out.push_str(&format!("{pad}Call({}args)\n", args.len()));
            dump_expr(out, callee, depth + 1);
            for arg in args {
                dump_expr(out, arg, depth + 1);
            }
        }
        ExprKind::Index { target, index } => {
            out.push_str(&format!("{pad}Index\n"));
            dump_expr(out, target, depth + 1);
            dump_expr(out, index, depth + 1);
        }
        ExprKind::Field { target, name } => {
            out.push_str(&format!("{pad}Field(.{name})\n"));
            dump_expr(out, target, depth + 1);
        }
        ExprKind::Function { name, params, ret_type, body, .. } => {
            let n = name.as_deref().unwrap_or("<anon>");
            let ret = ret_type.map(|t| format!(" -> {}", t.name())).unwrap_or_default();
            out.push_str(&format!("{pad}Function({n})[{}]{ret}\n", params.join(", ")));
            dump_block(out, body, depth + 1);
        }
        ExprKind::Spawn(expr) => {
            out.push_str(&format!("{pad}Spawn\n"));
            dump_expr(out, expr, depth + 1);
        }
        ExprKind::Assign { target, value } => {
            out.push_str(&format!("{pad}Assign\n"));
            dump_expr(out, target, depth + 1);
            dump_expr(out, value, depth + 1);
        }
    }
}
