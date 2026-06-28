// Bang — 트리 워킹 인터프리터 (Phase 3)
#![allow(clippy::ptr_arg)] // scope: &mut Vec<Arc<BangFuture>> — push가 필요하므로 Vec 사용

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::ast::*;
use crate::lexer::token::Span;
use crate::runtime::{
    deep_resolve, resolve_shallow, BangChannel, BangFunction, BangFuture, Env, RuntimeError, Value,
};

// ============================================================================
// 제어 흐름 결과
// ============================================================================

#[derive(Debug)]
enum StmtResult {
    None,
    Return(Value),
    Break,
    Continue,
}

// ============================================================================
// Interpreter
// ============================================================================

#[derive(Clone)]
pub struct Interpreter {
    pub output: Arc<Mutex<Vec<String>>>,
    /// REPL용 지속 env — run()은 이것을 무시, run_incremental()은 이것을 사용
    pub global_env: Arc<Mutex<Env>>,
}

impl Interpreter {
    pub fn new() -> Self {
        let global_env = Arc::new(Mutex::new(Env::new()));
        let interp = Self {
            output: Arc::new(Mutex::new(Vec::new())),
            global_env: global_env.clone(),
        };
        interp.define_builtins(&global_env);
        interp
    }

    pub fn run(&self, prog: &Program) -> Result<(), RuntimeError> {
        let global = Arc::new(Mutex::new(Env::new()));
        self.define_builtins(&global);
        let mut scope: Vec<Arc<BangFuture>> = Vec::new();
        let result = self.eval_program(prog, &global, &mut scope);
        let mut join_err: Option<RuntimeError> = Option::None;
        for f in &scope {
            if join_err.is_none() {
                if let Err(e) = f.resolve() { join_err = Some(e); }
            }
        }
        result?;
        if let Some(e) = join_err { return Err(e); }
        Ok(())
    }

    /// REPL 전용: self.global_env 에 상태를 누적하며 실행.
    pub fn run_incremental(&self, prog: &Program) -> Result<(), RuntimeError> {
        let mut scope: Vec<Arc<BangFuture>> = Vec::new();
        let result = self.eval_program(prog, &self.global_env, &mut scope);
        let mut join_err: Option<RuntimeError> = Option::None;
        for f in &scope {
            if join_err.is_none() {
                if let Err(e) = f.resolve() { join_err = Some(e); }
            }
        }
        result?;
        if let Some(e) = join_err { return Err(e); }
        Ok(())
    }

    // =========================================================================
    // 내장 함수 등록
    // =========================================================================

    fn define_builtins(&self, env: &Arc<Mutex<Env>>) {
        let mut g = env.lock().unwrap();
        for name in &[
            "print", "str", "int", "float", "len",
            "channel", "send", "recv", "close", "wait", "parallel_map",
        ] {
            g.define(name.to_string(), Value::Builtin(name));
        }
    }

    // =========================================================================
    // 프로그램 / 블록
    // =========================================================================

    fn eval_program(
        &self,
        prog: &Program,
        env: &Arc<Mutex<Env>>,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<(), RuntimeError> {
        for stmt in &prog.stmts {
            match self.eval_stmt(stmt, env, scope)? {
                StmtResult::None => {}
                _ => break,
            }
        }
        Ok(())
    }

    fn eval_block(
        &self,
        block: &Block,
        parent_env: &Arc<Mutex<Env>>,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<StmtResult, RuntimeError> {
        let env = Arc::new(Mutex::new(Env::with_parent(parent_env.clone())));
        for stmt in &block.stmts {
            match self.eval_stmt(stmt, &env, scope)? {
                StmtResult::None => {}
                ctrl => return Ok(ctrl),
            }
        }
        Ok(StmtResult::None)
    }

    // =========================================================================
    // 문(Statement)
    // =========================================================================

    fn eval_stmt(
        &self,
        stmt: &Stmt,
        env: &Arc<Mutex<Env>>,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<StmtResult, RuntimeError> {
        let span = stmt.span;
        match &stmt.kind {
            StmtKind::Let { name, value } => {
                let val = self.eval_expr(value, env, scope)?;
                // 이름 있는 함수: 클로저에 자기 이름 등록 → 재귀 허용
                if let Value::Function(ref f) = val {
                    if let Some(fname) = &f.name {
                        f.closure.lock().unwrap().define(fname.clone(), val.clone());
                    }
                }
                env.lock().unwrap().define(name.clone(), val);
                Ok(StmtResult::None)
            }
            StmtKind::Expr(expr) => {
                self.eval_expr(expr, env, scope)?;
                Ok(StmtResult::None)
            }
            StmtKind::Return(opt_expr) => {
                let val = match opt_expr {
                    Some(e) => resolve_shallow(self.eval_expr(e, env, scope)?)?,
                    Option::None => Value::Nil,
                };
                Ok(StmtResult::Return(val))
            }
            StmtKind::If { cond, then, else_ } => {
                let cv = resolve_shallow(self.eval_expr(cond, env, scope)?)?;
                if cv.is_truthy() {
                    self.eval_block(then, env, scope)
                } else if let Some(el) = else_ {
                    self.eval_block(el, env, scope)
                } else {
                    Ok(StmtResult::None)
                }
            }
            StmtKind::While { cond, body } => {
                loop {
                    let cv = resolve_shallow(self.eval_expr(cond, env, scope)?)?;
                    if !cv.is_truthy() { break; }
                    match self.eval_block(body, env, scope)? {
                        StmtResult::None => {}
                        StmtResult::Break => break,
                        StmtResult::Continue => continue,
                        ctrl @ StmtResult::Return(_) => return Ok(ctrl),
                    }
                }
                Ok(StmtResult::None)
            }
            StmtKind::For { var, iter, body } => {
                let iter_val = resolve_shallow(self.eval_expr(iter, env, scope)?)?;
                match iter_val {
                    Value::List(items) => {
                        'for_list: for item in items {
                            env.lock().unwrap().define(var.clone(), item);
                            match self.eval_block(body, env, scope)? {
                                StmtResult::None => {}
                                StmtResult::Break => break 'for_list,
                                StmtResult::Continue => continue 'for_list,
                                ctrl @ StmtResult::Return(_) => return Ok(ctrl),
                            }
                        }
                    }
                    Value::Channel(ch) => {
                        'for_ch: while let Some(v) = ch.recv() {
                            env.lock().unwrap().define(var.clone(), v);
                            match self.eval_block(body, env, scope)? {
                                StmtResult::None => {}
                                StmtResult::Break => break 'for_ch,
                                StmtResult::Continue => continue 'for_ch,
                                ctrl @ StmtResult::Return(_) => return Ok(ctrl),
                            }
                        }
                    }
                    other => return Err(RuntimeError::new(
                        format!("for-in: List 또는 Channel 필요, {} 발견", other.type_name()), span)),
                }
                Ok(StmtResult::None)
            }
            StmtKind::Block(block) => self.eval_block(block, env, scope),
            StmtKind::Parallel(block) => {
                let mut par_scope: Vec<Arc<BangFuture>> = Vec::new();
                let result = self.eval_block(block, env, &mut par_scope);
                let mut join_err: Option<RuntimeError> = Option::None;
                for f in &par_scope {
                    if join_err.is_none() {
                        if let Err(e) = f.resolve() { join_err = Some(e); }
                    }
                }
                result?;
                if let Some(e) = join_err { return Err(e); }
                Ok(StmtResult::None)
            }
            StmtKind::Break => Ok(StmtResult::Break),
            StmtKind::Continue => Ok(StmtResult::Continue),
            // try/catch/throw는 VM(기본 실행 엔진)에서만 지원한다.
            StmtKind::Try { .. } | StmtKind::Throw(_) => Err(RuntimeError::new(
                "try/catch/throw는 VM에서 실행하세요 (트리워킹 인터프리터 --interp 미지원)",
                span,
            )),
        }
    }

    // =========================================================================
    // 식(Expression)
    // =========================================================================

    fn eval_expr(
        &self,
        expr: &Expr,
        env: &Arc<Mutex<Env>>,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<Value, RuntimeError> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Int(n)   => Ok(Value::Int(*n)),
            ExprKind::Float(n) => Ok(Value::Float(*n)),
            ExprKind::Bool(b)  => Ok(Value::Bool(*b)),
            ExprKind::Str(s)   => Ok(Value::Str(s.clone())),
            ExprKind::Nil      => Ok(Value::Nil),

            ExprKind::Ident(name) => env.lock().unwrap().get(name).ok_or_else(|| {
                RuntimeError::new(format!("정의되지 않은 변수: {name}"), span)
            }),

            ExprKind::List(items) => {
                let vals = items.iter()
                    .map(|e| self.eval_expr(e, env, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(vals))
            }

            ExprKind::Map(entries) => {
                let mut map = HashMap::new();
                for (key, val_expr) in entries {
                    let v = self.eval_expr(val_expr, env, scope)?;
                    map.insert(key.as_str().to_string(), v);
                }
                Ok(Value::Map(map))
            }

            ExprKind::Unary { op, expr: inner } => {
                let val = resolve_shallow(self.eval_expr(inner, env, scope)?)?;
                self.eval_unary(*op, val, span)
            }

            ExprKind::Binary { op, left, right } => {
                // and/or: 단락 평가 (short-circuit)
                if *op == BinaryOp::And {
                    let lv = resolve_shallow(self.eval_expr(left, env, scope)?)?;
                    return if !lv.is_truthy() { Ok(lv) }
                        else { resolve_shallow(self.eval_expr(right, env, scope)?) };
                }
                if *op == BinaryOp::Or {
                    let lv = resolve_shallow(self.eval_expr(left, env, scope)?)?;
                    return if lv.is_truthy() { Ok(lv) }
                        else { resolve_shallow(self.eval_expr(right, env, scope)?) };
                }
                let lv = resolve_shallow(self.eval_expr(left, env, scope)?)?;
                let rv = resolve_shallow(self.eval_expr(right, env, scope)?)?;
                eval_binary(*op, lv, rv, span)
            }

            ExprKind::Assign { target, value } => {
                let val = self.eval_expr(value, env, scope)?;
                self.do_assign(target, val.clone(), env, scope, span)?;
                Ok(val)
            }

            ExprKind::Call { callee, args } => {
                let callee_val = self.eval_expr(callee, env, scope)?;
                let arg_vals = args.iter()
                    .map(|a| self.eval_expr(a, env, scope).and_then(resolve_shallow))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call_value(callee_val, arg_vals, span, scope)
            }

            ExprKind::Index { target, index } => {
                let tgt = resolve_shallow(self.eval_expr(target, env, scope)?)?;
                let idx = resolve_shallow(self.eval_expr(index, env, scope)?)?;
                eval_index(tgt, idx, span)
            }

            ExprKind::Field { target, name } => {
                let tgt = resolve_shallow(self.eval_expr(target, env, scope)?)?;
                eval_field(tgt, name, span)
            }

            ExprKind::Function { name, params, body } => {
                Ok(Value::Function(Arc::new(BangFunction {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    closure: env.clone(),
                })))
            }

            ExprKind::Spawn(inner_expr) => self.do_spawn(inner_expr, env, scope),
        }
    }

    // =========================================================================
    // spawn
    // =========================================================================

    fn do_spawn(
        &self,
        expr: &Expr,
        env: &Arc<Mutex<Env>>,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<Value, RuntimeError> {
        let snapshot = env.lock().unwrap().snapshot();
        let interp = self.clone();
        let expr_clone = expr.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<Value, RuntimeError>>();
        let future = Arc::new(BangFuture::new(rx));

        std::thread::spawn(move || {
            let mut thread_scope: Vec<Arc<BangFuture>> = Vec::new();
            let result = interp.eval_expr(&expr_clone, &snapshot, &mut thread_scope);
            let mut join_err: Option<RuntimeError> = Option::None;
            for f in &thread_scope {
                if join_err.is_none() {
                    if let Err(e) = f.resolve() { join_err = Some(e); }
                }
            }
            let final_result = join_err.map(Err).unwrap_or(result);
            tx.send(final_result).ok();
        });

        scope.push(future.clone());
        Ok(Value::Future(future))
    }

    // =========================================================================
    // 함수 호출
    // =========================================================================

    fn call_value(
        &self,
        callee: Value,
        args: Vec<Value>,
        span: Span,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::Function(f) => self.call_function(&f, args, span),
            Value::Builtin(name) => self.call_builtin(name, args, span, scope),
            other => Err(RuntimeError::new(
                format!("호출할 수 없는 값: {}", other.type_name()), span)),
        }
    }

    fn call_function(
        &self,
        func: &BangFunction,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if args.len() != func.params.len() {
            return Err(RuntimeError::new(
                format!("인자 개수 불일치: {}개 기대, {}개 전달", func.params.len(), args.len()),
                span,
            ));
        }
        let call_env = Arc::new(Mutex::new(Env::with_parent(func.closure.clone())));
        {
            let mut guard = call_env.lock().unwrap();
            for (p, v) in func.params.iter().zip(args) {
                guard.define(p.clone(), v);
            }
            if let Some(name) = &func.name {
                guard.define(name.clone(), Value::Function(Arc::new(BangFunction {
                    name: func.name.clone(),
                    params: func.params.clone(),
                    body: func.body.clone(),
                    closure: func.closure.clone(),
                })));
            }
        }
        let mut call_scope: Vec<Arc<BangFuture>> = Vec::new();
        let block_result = self.eval_block(&func.body, &call_env, &mut call_scope);
        let mut join_err: Option<RuntimeError> = Option::None;
        for f in &call_scope {
            if join_err.is_none() {
                if let Err(e) = f.resolve() { join_err = Some(e); }
            }
        }
        let ret_val = match block_result? {
            StmtResult::Return(v) => v,
            _ => Value::Nil,
        };
        if let Some(e) = join_err { return Err(e); }
        Ok(ret_val)
    }

    fn call_builtin(
        &self,
        name: &str,
        args: Vec<Value>,
        span: Span,
        scope: &mut Vec<Arc<BangFuture>>,
    ) -> Result<Value, RuntimeError> {
        match name {
            "print"        => self.builtin_print(args, span),
            "str"          => builtin_str(args, span),
            "int"          => builtin_int(args, span),
            "float"        => builtin_float(args, span),
            "len"          => builtin_len(args, span),
            "channel"      => builtin_channel(args),
            "send"         => builtin_send(args, span),
            "recv"         => builtin_recv(args, span),
            "close"        => builtin_close(args, span),
            "wait"         => builtin_wait(args, span),
            "parallel_map" => self.builtin_parallel_map(args, span, scope),
            _ => Err(RuntimeError::new(format!("알 수 없는 내장 함수: {name}"), span)),
        }
    }

    // =========================================================================
    // 대입 헬퍼
    // =========================================================================

    fn do_assign(
        &self,
        target: &Expr,
        val: Value,
        env: &Arc<Mutex<Env>>,
        scope: &mut Vec<Arc<BangFuture>>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        match &target.kind {
            ExprKind::Ident(name) => {
                if !env.lock().unwrap().assign(name, val) {
                    return Err(RuntimeError::new(
                        format!("정의되지 않은 변수에 대입: {name}"), span));
                }
            }
            ExprKind::Index { target: tgt, index } => {
                let container = resolve_shallow(self.eval_expr(tgt, env, scope)?)?;
                let idx_val = resolve_shallow(self.eval_expr(index, env, scope)?)?;
                match container {
                    Value::List(mut items) => {
                        let i = list_index(idx_val, items.len(), span)?;
                        items[i] = val;
                        self.do_assign(tgt, Value::List(items), env, scope, span)?;
                    }
                    Value::Map(mut map) => {
                        let key = str_key(idx_val, span)?;
                        map.insert(key, val);
                        self.do_assign(tgt, Value::Map(map), env, scope, span)?;
                    }
                    other => return Err(RuntimeError::new(
                        format!("인덱스 대입: List/Map 필요, {} 발견", other.type_name()), span)),
                }
            }
            ExprKind::Field { target: tgt, name: field } => {
                let container = resolve_shallow(self.eval_expr(tgt, env, scope)?)?;
                match container {
                    Value::Map(mut map) => {
                        map.insert(field.clone(), val);
                        self.do_assign(tgt, Value::Map(map), env, scope, span)?;
                    }
                    other => return Err(RuntimeError::new(
                        format!("필드 대입: Map 필요, {} 발견", other.type_name()), span)),
                }
            }
            _ => return Err(RuntimeError::new("유효하지 않은 대입 대상", span)),
        }
        Ok(())
    }

    // =========================================================================
    // 단항 연산
    // =========================================================================

    fn eval_unary(&self, op: UnaryOp, val: Value, span: Span) -> Result<Value, RuntimeError> {
        match op {
            UnaryOp::Neg => match val {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(n) => Ok(Value::Float(-n)),
                _ => Err(RuntimeError::new(
                    format!("단항 - : 숫자 필요, {} 발견", val.type_name()), span)),
            },
            UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
        }
    }

    // =========================================================================
    // 내장 함수 — self 필요
    // =========================================================================

    fn builtin_print(&self, args: Vec<Value>, _span: Span) -> Result<Value, RuntimeError> {
        let parts: Result<Vec<String>, _> = args.into_iter()
            .map(|v| deep_resolve(v).map(|v| format!("{v}")))
            .collect();
        let line = parts?.join(" ");
        println!("{line}");
        self.output.lock().unwrap().push(line);
        Ok(Value::Nil)
    }

    fn builtin_parallel_map(
        &self,
        args: Vec<Value>,
        span: Span,
        _scope: &mut [Arc<BangFuture>],
    ) -> Result<Value, RuntimeError> {
        if args.len() < 2 {
            return Err(RuntimeError::new("parallel_map(list, fn) 인자 2개 필요", span));
        }
        let list = match args[0].clone() {
            Value::List(items) => items,
            _ => return Err(RuntimeError::new(
                format!("parallel_map: 첫 인자 List 필요, {} 발견", args[0].type_name()), span)),
        };
        let func = args[1].clone();
        let mut futures: Vec<Arc<BangFuture>> = Vec::new();
        for item in list {
            let interp = self.clone();
            let func_clone = func.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<Value, RuntimeError>>();
            let future = Arc::new(BangFuture::new(rx));
            std::thread::spawn(move || {
                let mut dummy: Vec<Arc<BangFuture>> = Vec::new();
                let r = interp.call_value(func_clone, vec![item], span, &mut dummy);
                for f in &dummy { f.resolve().ok(); }
                tx.send(r).ok();
            });
            futures.push(future);
        }
        let results: Result<Vec<Value>, _> = futures.iter().map(|f| f.resolve()).collect();
        Ok(Value::List(results?))
    }
}

impl Default for Interpreter {
    fn default() -> Self { Self::new() }
}

// ============================================================================
// 자유 함수
// ============================================================================

fn eval_index(target: Value, index: Value, span: Span) -> Result<Value, RuntimeError> {
    match target {
        Value::List(items) => {
            let i = list_index(index, items.len(), span)?;
            Ok(items[i].clone())
        }
        Value::Map(map) => {
            let key = str_key(index, span)?;
            Ok(map.get(&key).cloned().unwrap_or(Value::Nil))
        }
        Value::Str(s) => match index {
            Value::Int(n) => {
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len() as i64;
                let idx = if n < 0 { len + n } else { n };
                if idx < 0 || idx as usize >= chars.len() {
                    return Err(RuntimeError::new(
                        format!("문자열 인덱스 범위 초과: {n}"), span));
                }
                Ok(Value::Str(chars[idx as usize].to_string()))
            }
            _ => Err(RuntimeError::new("문자열 인덱스는 정수여야 합니다", span)),
        },
        other => Err(RuntimeError::new(
            format!("인덱스 접근: List/Map/Str 필요, {} 발견", other.type_name()), span)),
    }
}

fn eval_field(target: Value, name: &str, span: Span) -> Result<Value, RuntimeError> {
    match target {
        Value::Map(map) => Ok(map.get(name).cloned().unwrap_or(Value::Nil)),
        other => Err(RuntimeError::new(
            format!("필드 접근: Map 필요, {} 발견", other.type_name()), span)),
    }
}

fn eval_binary(op: BinaryOp, l: Value, r: Value, span: Span) -> Result<Value, RuntimeError> {
    match op {
        BinaryOp::Add => eval_add(l, r, span),
        BinaryOp::Sub => eval_num(l, r, span, "−", |a, b| Value::Int(a.wrapping_sub(b)), |a, b| Value::Float(a - b)),
        BinaryOp::Mul => eval_num(l, r, span, "×", |a, b| Value::Int(a.wrapping_mul(b)), |a, b| Value::Float(a * b)),
        BinaryOp::Div => eval_div(l, r, span),
        BinaryOp::Mod => eval_num(l, r, span, "%", |a, b| Value::Int(a.wrapping_rem(b)), |a, b| Value::Float(a % b)),
        BinaryOp::Eq  => Ok(Value::Bool(values_eq(&l, &r))),
        BinaryOp::Ne  => Ok(Value::Bool(!values_eq(&l, &r))),
        BinaryOp::Lt  => eval_cmp(l, r, span, std::cmp::Ordering::Less, false),
        BinaryOp::Le  => eval_cmp(l, r, span, std::cmp::Ordering::Less, true),
        BinaryOp::Gt  => eval_cmp(l, r, span, std::cmp::Ordering::Greater, false),
        BinaryOp::Ge  => eval_cmp(l, r, span, std::cmp::Ordering::Greater, true),
        BinaryOp::And | BinaryOp::Or => unreachable!("short-circuit handled in eval_expr"),
    }
}

fn eval_add(l: Value, r: Value, span: Span) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b as f64)),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
        (Value::List(mut a), Value::List(b)) => { a.extend(b); Ok(Value::List(a)) }
        (l, r) => Err(RuntimeError::new(
            format!("+ 연산: 호환 타입 필요, {} + {}", l.type_name(), r.type_name()), span)),
    }
}

fn eval_num(
    l: Value,
    r: Value,
    span: Span,
    _op_name: &str,
    int_op: fn(i64, i64) -> Value,
    float_op: fn(f64, f64) -> Value,
) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(int_op(a, b)),
        (Value::Float(a), Value::Float(b)) => Ok(float_op(a, b)),
        (Value::Int(a), Value::Float(b)) => Ok(float_op(a as f64, b)),
        (Value::Float(a), Value::Int(b)) => Ok(float_op(a, b as f64)),
        (l, r) => Err(RuntimeError::new(
            format!("산술 연산: 숫자 필요, {} vs {}", l.type_name(), r.type_name()), span)),
    }
}

fn eval_div(l: Value, r: Value, span: Span) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(RuntimeError::new("0으로 나누기", span)); }
            if a % b == 0 { Ok(Value::Int(a / b)) } else { Ok(Value::Float(a as f64 / b as f64)) }
        }
        (Value::Float(a), Value::Float(b)) => {
            if b == 0.0 { return Err(RuntimeError::new("0으로 나누기", span)); }
            Ok(Value::Float(a / b))
        }
        (Value::Int(a), Value::Float(b)) => {
            if b == 0.0 { return Err(RuntimeError::new("0으로 나누기", span)); }
            Ok(Value::Float(a as f64 / b))
        }
        (Value::Float(a), Value::Int(b)) => {
            if b == 0 { return Err(RuntimeError::new("0으로 나누기", span)); }
            Ok(Value::Float(a / b as f64))
        }
        (l, r) => Err(RuntimeError::new(
            format!("/ 연산: 숫자 필요, {} / {}", l.type_name(), r.type_name()), span)),
    }
}

fn eval_cmp(
    l: Value, r: Value, span: Span, expected: std::cmp::Ordering, allow_eq: bool,
) -> Result<Value, RuntimeError> {
    let ord = match (&l, &r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) =>
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Int(a), Value::Float(b)) =>
            (*a as f64).partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Float(a), Value::Int(b)) =>
            a.partial_cmp(&(*b as f64)).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        _ => return Err(RuntimeError::new(
            format!("비교: 호환 타입 필요, {} vs {}", l.type_name(), r.type_name()), span)),
    };
    Ok(Value::Bool(ord == expected || (allow_eq && ord == std::cmp::Ordering::Equal)))
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        (Value::List(x), Value::List(y)) =>
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| values_eq(a, b)),
        _ => false,
    }
}

fn list_index(idx_val: Value, len: usize, span: Span) -> Result<usize, RuntimeError> {
    match idx_val {
        Value::Int(n) => {
            let i = if n < 0 { len as i64 + n } else { n };
            if i < 0 || i as usize >= len {
                Err(RuntimeError::new(format!("리스트 인덱스 범위 초과: {n}"), span))
            } else {
                Ok(i as usize)
            }
        }
        _ => Err(RuntimeError::new("리스트 인덱스는 정수여야 합니다", span)),
    }
}

fn str_key(idx_val: Value, span: Span) -> Result<String, RuntimeError> {
    match idx_val {
        Value::Str(s) => Ok(s),
        _ => Err(RuntimeError::new("맵 키는 문자열이어야 합니다", span)),
    }
}

// ============================================================================
// 내장 함수 자유 함수
// ============================================================================

fn builtin_str(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    args.into_iter().next()
        .ok_or_else(|| RuntimeError::new("str(): 인자 1개 필요", span))
        .map(|v| Value::Str(format!("{v}")))
}

fn builtin_int(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Int(n))   => Ok(Value::Int(n)),
        Some(Value::Float(n)) => Ok(Value::Int(n as i64)),
        Some(Value::Bool(b))  => Ok(Value::Int(if b { 1 } else { 0 })),
        Some(Value::Str(s))   => s.parse::<i64>().map(Value::Int)
            .map_err(|_| RuntimeError::new(format!("int() 변환 실패: {s:?}"), span)),
        Some(other) => Err(RuntimeError::new(format!("int(): {} 변환 불가", other.type_name()), span)),
        Option::None => Err(RuntimeError::new("int(): 인자 1개 필요", span)),
    }
}

fn builtin_float(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Float(n)) => Ok(Value::Float(n)),
        Some(Value::Int(n))   => Ok(Value::Float(n as f64)),
        Some(Value::Str(s))   => s.parse::<f64>().map(Value::Float)
            .map_err(|_| RuntimeError::new(format!("float() 변환 실패: {s:?}"), span)),
        Some(other) => Err(RuntimeError::new(format!("float(): {} 변환 불가", other.type_name()), span)),
        Option::None => Err(RuntimeError::new("float(): 인자 1개 필요", span)),
    }
}

fn builtin_len(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::List(items)) => Ok(Value::Int(items.len() as i64)),
        Some(Value::Str(s))      => Ok(Value::Int(s.chars().count() as i64)),
        Some(Value::Map(map))    => Ok(Value::Int(map.len() as i64)),
        Some(other) => Err(RuntimeError::new(
            format!("len(): List/Str/Map 필요, {} 발견", other.type_name()), span)),
        Option::None => Err(RuntimeError::new("len(): 인자 1개 필요", span)),
    }
}

fn builtin_channel(args: Vec<Value>) -> Result<Value, RuntimeError> {
    let cap = match args.first() { Some(Value::Int(n)) => *n as usize, _ => 0 };
    Ok(Value::Channel(Arc::new(BangChannel::new(cap))))
}

fn builtin_send(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::new("send(ch, val) 인자 2개 필요", span));
    }
    match &args[0] {
        Value::Channel(ch) => { ch.send(args[1].clone())?; Ok(Value::Nil) }
        _ => Err(RuntimeError::new(
            format!("send(): 첫 인자 Channel 필요, {} 발견", args[0].type_name()), span)),
    }
}

fn builtin_recv(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Channel(ch)) => Ok(ch.recv().unwrap_or(Value::Nil)),
        Some(other) => Err(RuntimeError::new(
            format!("recv(): Channel 필요, {} 발견", other.type_name()), span)),
        Option::None => Err(RuntimeError::new("recv(): 인자 1개 필요", span)),
    }
}

fn builtin_close(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Channel(ch)) => { ch.close(); Ok(Value::Nil) }
        Some(other) => Err(RuntimeError::new(
            format!("close(): Channel 필요, {} 발견", other.type_name()), span)),
        Option::None => Err(RuntimeError::new("close(): 인자 1개 필요", span)),
    }
}

fn builtin_wait(args: Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    match args.into_iter().next() {
        Some(v) => resolve_shallow(v),
        Option::None => Err(RuntimeError::new("wait(): 인자 1개 필요", span)),
    }
}
