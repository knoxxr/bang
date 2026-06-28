// Bang — 정적 타입 검사 (gradual)
//
// 타입 힌트(let/파라미터/반환)를 실행 전에 정적으로 검사한다.
// 동적 언어이므로 **확실한 충돌만** 보고하고(둘 다 구체 타입이고 서로 다름),
// 알 수 없는(Unknown) 타입은 통과시켜 거짓 양성을 피한다.
//
// 검사 항목:
//   1) 타입된 let:    let x: int = "s"   (값 타입이 선언과 충돌)
//   2) 함수 호출 인자:  fn f(a: int) 에 f("s")
//   3) 타입된 반환값:   fn f() -> int { return "s" }

use std::collections::HashMap;

use crate::ast::*;
use crate::lexer::token::Span;

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Float,
    Bool,
    Str,
    Nil,
    List,
    Map,
    Fn,
    Unknown, // any 또는 정적으로 알 수 없음 → 충돌 판정 제외
}

impl Ty {
    fn from_ann(a: TypeAnn) -> Ty {
        match a {
            TypeAnn::Int => Ty::Int,
            TypeAnn::Float => Ty::Float,
            TypeAnn::Bool => Ty::Bool,
            TypeAnn::Str => Ty::Str,
            TypeAnn::Nil => Ty::Nil,
            TypeAnn::List => Ty::List,
            TypeAnn::Map => Ty::Map,
            TypeAnn::Fn => Ty::Fn,
            TypeAnn::Any => Ty::Unknown,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Ty::Int => "int",
            Ty::Float => "float",
            Ty::Bool => "bool",
            Ty::Str => "str",
            Ty::Nil => "nil",
            Ty::List => "list",
            Ty::Map => "map",
            Ty::Fn => "fn",
            Ty::Unknown => "any",
        }
    }
    /// 둘 다 구체 타입이고 서로 다르면 충돌.
    fn conflicts(self, other: Ty) -> bool {
        self != Ty::Unknown && other != Ty::Unknown && self != other
    }
}

#[derive(Clone)]
struct FnSig {
    params: Vec<Ty>,
    ret: Ty,
}

pub struct Checker {
    errors: Vec<TypeError>,
    sigs: HashMap<String, FnSig>,    // 함수 이름 → 시그니처
    scopes: Vec<HashMap<String, Ty>>, // 변수 → 타입
    ret_stack: Vec<Ty>,              // 현재 함수의 선언된 반환 타입
}

/// 프로그램을 정적 타입 검사한다.
pub fn check(prog: &Program) -> Vec<TypeError> {
    let mut c = Checker {
        errors: Vec::new(),
        sigs: HashMap::new(),
        scopes: vec![HashMap::new()],
        ret_stack: Vec::new(),
    };
    c.collect_signatures(&prog.stmts);
    c.check_stmts(&prog.stmts);
    c.errors
}

impl Checker {
    fn err(&mut self, msg: impl Into<String>, span: Span) {
        self.errors.push(TypeError { message: msg.into(), span });
    }

    /// 최상위 함수 시그니처를 먼저 수집(전방 참조 허용).
    fn collect_signatures(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            if let StmtKind::Let { name, value, .. } = &s.kind {
                if let ExprKind::Function { params, param_types, ret_type, .. } = &value.kind {
                    self.sigs.insert(name.clone(), Self::sig_of(params.len(), param_types, *ret_type));
                }
            }
        }
    }

    fn sig_of(nparams: usize, param_types: &[Option<TypeAnn>], ret: Option<TypeAnn>) -> FnSig {
        let mut params = vec![Ty::Unknown; nparams];
        for (i, pt) in param_types.iter().enumerate() {
            if let Some(t) = pt { params[i] = Ty::from_ann(*t); }
        }
        FnSig { params, ret: ret.map(Ty::from_ann).unwrap_or(Ty::Unknown) }
    }

    fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop_scope(&mut self) { self.scopes.pop(); }
    fn define(&mut self, name: &str, ty: Ty) {
        if let Some(s) = self.scopes.last_mut() { s.insert(name.to_string(), ty); }
    }
    fn lookup(&self, name: &str) -> Ty {
        for s in self.scopes.iter().rev() {
            if let Some(t) = s.get(name) { return *t; }
        }
        Ty::Unknown
    }

    fn check_stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts { self.check_stmt(s); }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                let vty = self.infer(value);
                if let Some(decl) = ty {
                    let dt = Ty::from_ann(*decl);
                    if dt.conflicts(vty) {
                        self.err(format!("타입 불일치: '{name}'은 {} 인데 {} 값이 대입됨",
                            dt.name(), vty.name()), stmt.span);
                    }
                    self.define(name, dt);
                } else {
                    self.define(name, vty);
                }
            }
            StmtKind::Expr(e) => { self.infer(e); }
            StmtKind::Return(opt) => {
                let rt = self.ret_stack.last().copied().unwrap_or(Ty::Unknown);
                let vty = opt.as_ref().map(|e| self.infer(e)).unwrap_or(Ty::Nil);
                if rt.conflicts(vty) {
                    self.err(format!("반환 타입 불일치: {} 기대, {} 반환",
                        rt.name(), vty.name()), stmt.span);
                }
            }
            StmtKind::If { cond, then, else_ } => {
                self.infer(cond);
                self.push_scope(); self.check_stmts(&then.stmts); self.pop_scope();
                if let Some(b) = else_ {
                    self.push_scope(); self.check_stmts(&b.stmts); self.pop_scope();
                }
            }
            StmtKind::While { cond, body } => {
                self.infer(cond);
                self.push_scope(); self.check_stmts(&body.stmts); self.pop_scope();
            }
            StmtKind::For { var, iter, body } => {
                self.infer(iter);
                self.push_scope();
                self.define(var, Ty::Unknown); // 원소 타입은 알 수 없음
                self.check_stmts(&body.stmts);
                self.pop_scope();
            }
            StmtKind::Block(b) => {
                self.push_scope(); self.check_stmts(&b.stmts); self.pop_scope();
            }
            StmtKind::Parallel(b) => {
                self.push_scope(); self.check_stmts(&b.stmts); self.pop_scope();
            }
            StmtKind::Try { body, catch_var, handler } => {
                self.push_scope(); self.check_stmts(&body.stmts); self.pop_scope();
                self.push_scope();
                self.define(catch_var, Ty::Unknown);
                self.check_stmts(&handler.stmts);
                self.pop_scope();
            }
            StmtKind::Throw(e) => { self.infer(e); }
            StmtKind::Break | StmtKind::Continue => {}
        }
    }

    /// 함수 본문을 검사하고(파라미터 타입 스코프 + 반환 타입), Fn 타입 반환.
    fn check_function(
        &mut self,
        params: &[String],
        param_types: &[Option<TypeAnn>],
        ret: Option<TypeAnn>,
        body: &Block,
    ) {
        self.push_scope();
        for (i, p) in params.iter().enumerate() {
            let pt = param_types.get(i).and_then(|x| *x).map(Ty::from_ann).unwrap_or(Ty::Unknown);
            self.define(p, pt);
        }
        self.ret_stack.push(ret.map(Ty::from_ann).unwrap_or(Ty::Unknown));
        self.check_stmts(&body.stmts);
        self.ret_stack.pop();
        self.pop_scope();
    }

    /// 식의 정적 타입을 추론한다 (알 수 없으면 Unknown). 부작용으로 내부 검사 수행.
    fn infer(&mut self, expr: &Expr) -> Ty {
        match &expr.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Nil => Ty::Nil,
            ExprKind::List(items) => { for it in items { self.infer(it); } Ty::List }
            ExprKind::Map(entries) => {
                for (_k, v) in entries { self.infer(v); } // 키는 Str/Ident 리터럴
                Ty::Map
            }
            ExprKind::Ident(name) => self.lookup(name),
            ExprKind::Unary { op, expr: inner } => {
                let t = self.infer(inner);
                match op {
                    UnaryOp::Not => Ty::Bool,
                    UnaryOp::Neg => if t == Ty::Float { Ty::Float } else if t == Ty::Int { Ty::Int } else { Ty::Unknown },
                }
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.infer(left);
                let r = self.infer(right);
                use BinaryOp::*;
                match op {
                    Eq | Ne | Lt | Le | Gt | Ge | And | Or => Ty::Bool,
                    Add => {
                        if l == Ty::Str && r == Ty::Str { Ty::Str }
                        else if l == Ty::Int && r == Ty::Int { Ty::Int }
                        else if (l == Ty::Float || l == Ty::Int) && (r == Ty::Float || r == Ty::Int)
                            && (l == Ty::Float || r == Ty::Float) { Ty::Float }
                        else if l == Ty::List && r == Ty::List { Ty::List }
                        else { Ty::Unknown }
                    }
                    Sub | Mul | Div | Mod => {
                        if l == Ty::Int && r == Ty::Int && *op != Div { Ty::Int }
                        else if (l == Ty::Int || l == Ty::Float) && (r == Ty::Int || r == Ty::Float) { Ty::Float }
                        else { Ty::Unknown }
                    }
                }
            }
            ExprKind::Assign { target, value } => {
                self.infer(target);
                self.infer(value)
            }
            ExprKind::Call { callee, args } => {
                // 인자 타입 추론(내부 검사)
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
                // 알려진 함수면 인자 타입 검사 + 반환 타입
                if let ExprKind::Ident(name) = &callee.kind {
                    if let Some(sig) = self.sigs.get(name).cloned() {
                        if sig.params.len() == arg_tys.len() {
                            for (i, (pt, at)) in sig.params.iter().zip(&arg_tys).enumerate() {
                                if pt.conflicts(*at) {
                                    self.err(format!("'{name}' 인자 {}: {} 기대, {} 전달",
                                        i + 1, pt.name(), at.name()), expr.span);
                                }
                            }
                        }
                        return sig.ret;
                    }
                } else {
                    self.infer(callee);
                }
                Ty::Unknown
            }
            ExprKind::Index { target, index } => {
                self.infer(target); self.infer(index); Ty::Unknown
            }
            ExprKind::Field { target, .. } => { self.infer(target); Ty::Unknown }
            ExprKind::Function { name, params, param_types, ret_type, body } => {
                // 지역 함수 시그니처도 등록(이름 있으면)
                if let Some(n) = name {
                    self.sigs.insert(n.clone(), Self::sig_of(params.len(), param_types, *ret_type));
                }
                self.check_function(params, param_types, *ret_type, body);
                Ty::Fn
            }
            ExprKind::Spawn(inner) => { self.infer(inner); Ty::Unknown }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn errs(src: &str) -> Vec<TypeError> {
        let toks = Lexer::new(src).tokenize().expect("lex");
        let prog = Parser::new(toks).parse().expect("parse");
        check(&prog)
    }

    #[test]
    fn typed_let_mismatch() {
        assert_eq!(errs("let x: int = \"s\"").len(), 1);
        assert!(errs("let x: int = 42").is_empty());
        assert!(errs("let x: str = \"ok\"").is_empty());
    }

    #[test]
    fn any_and_untyped_pass() {
        assert!(errs("let x: any = \"s\"").is_empty());
        assert!(errs("let x = \"s\"").is_empty());
        assert!(errs("let x = 1\nlet y: int = x").is_empty());
    }

    #[test]
    fn call_arg_mismatch() {
        assert_eq!(errs("fn f(a: int) { return a }\nf(\"s\")").len(), 1);
        assert!(errs("fn f(a: int) { return a }\nf(3)").is_empty());
        // 미지 타입 인자는 통과 (gradual)
        assert!(errs("fn f(a: int) { return a }\nlet xs = [1]\nf(xs[0])").is_empty());
    }

    #[test]
    fn return_mismatch() {
        assert_eq!(errs("fn g() -> int { return \"s\" }").len(), 1);
        assert!(errs("fn g() -> int { return 5 }").is_empty());
        assert!(errs("fn g() -> str { return \"ok\" }").is_empty());
    }

    #[test]
    fn no_false_positive_dynamic() {
        // 동적 흐름은 충돌 보고 안 함
        let src = "let xs = [1, \"two\"]\nfn f(n: int) { return n }\nlet y: int = xs[0]";
        assert!(errs(src).is_empty());
    }
}
