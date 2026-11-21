// Bang — Phase 10: C 트랜스파일러 (AST → C11)
//
// 지원 범위:
//   - 값 타입: Int, Float, Bool, Nil, String 리터럴
//   - 문: let, assign, if/else, while, return, break, continue, expr
//   - 식: 리터럴, 산술·비교·논리 연산자, 함수 호출, 식별자
//   - 함수: 최상위 named fn (재귀 포함), 단 클로저·일급함수 미지원
//   - 내장: print, str, int, float, len, bool, assert, abs, sqrt,
//            floor, ceil, round, pow, min, max, exit
//
// 미지원 → TranspileError:
//   List, Map, Index, Field, Spawn, Parallel, For-in, closure/lambda

use std::collections::HashSet;
use std::fmt::Write as FmtWrite;

use crate::ast::{BinaryOp, Block, Expr, ExprKind, Program, Stmt, StmtKind, UnaryOp};
use crate::lexer::token::Span;

// ============================================================================
// 오류
// ============================================================================

#[derive(Debug, Clone)]
pub struct TranspileError {
    pub message: String,
    pub span:    Span,
}

impl TranspileError {
    fn new(msg: impl Into<String>, span: Span) -> Self {
        Self { message: msg.into(), span }
    }
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.span.line == 0 {
            write!(f, "AOT 오류: {}", self.message)
        } else {
            write!(f, "[{}:{}] AOT 오류: {}", self.span.line, self.span.col, self.message)
        }
    }
}

// ============================================================================
// C 런타임 헤더 (embeds)
// ============================================================================

const C_RUNTIME: &str = r#"/* Bang Language C Runtime — 자동 생성 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <errno.h>

typedef enum { BT_INT=0, BT_FLOAT=1, BT_BOOL=2, BT_NIL=3, BT_STR=4 } BT;
typedef struct { BT tag; union { int64_t i; double f; int b; char* s; }; } BV;

static BV BV_int(int64_t n)  { BV v; v.tag=BT_INT;   v.i=n;   return v; }
static BV BV_flt(double n)   { BV v; v.tag=BT_FLOAT; v.f=n;   return v; }
static BV BV_bool(int b)     { BV v; v.tag=BT_BOOL;  v.b=b!=0;return v; }
static BV BV_nil(void)       { BV v; v.tag=BT_NIL;   v.i=0;   return v; }
static BV BV_str(const char* s){ BV v; v.tag=BT_STR; v.s=(char*)s; return v; }

#define BINT(n)  BV_int((int64_t)(n))
#define BFLT(n)  BV_flt((double)(n))
#define BBOOL(b) BV_bool((b)!=0)
#define BNIL     BV_nil()
#define BSTR(s)  BV_str(s)

/* VM과 같은 의미를 지키는 런타임 (Phase 58). 에러는 VM의 "잡히지 않은 에러"와 같이 stderr + exit 1. */
static void bv_fail(const char* msg) { fprintf(stderr, "런타임 오류: %s\n", msg); exit(1); }

/* 참거짓 — Python 규칙 (VM is_truthy와 같음) */
static int bv_truthy(BV v) {
    switch (v.tag) {
        case BT_BOOL:  return v.b;
        case BT_NIL:   return 0;
        case BT_INT:   return v.i != 0;
        case BT_FLOAT: return v.f != 0.0;
        case BT_STR:   return v.s[0] != '\0';
    }
    return 1;
}

/* float 표시 — VM fmt_float와 같음: 가장 짧게 왕복되는 자릿수를 지수 없이, 정수 값이면 ".0" (|x| < 1e16) */
static void bv_fmt_float(double x, char* out, size_t cap) {
    if (isnan(x)) { snprintf(out, cap, "NaN"); return; }
    if (isinf(x)) { snprintf(out, cap, x < 0 ? "-inf" : "inf"); return; }
    char tmp[64];
    int prec;
    for (prec = 1; prec <= 17; prec++) {
        snprintf(tmp, sizeof tmp, "%.*e", prec - 1, x);
        if (strtod(tmp, NULL) == x) break;
    }
    /* tmp = [-]d[.ddd]e[+-]XX */
    const char* p = tmp;
    int neg = 0;
    if (*p == '-') { neg = 1; p++; }
    char digits[32]; int n = 0;
    while (*p && *p != 'e') { if (*p != '.') digits[n++] = *p; p++; }
    digits[n] = '\0';
    int e = (*p == 'e') ? atoi(p + 1) : 0;
    while (n > 1 && digits[n - 1] == '0') digits[--n] = '\0';   /* 꼬리 0 제거 */
    int point = e + 1;               /* 소수점 앞 자릿수 */
    char body[400]; int k = 0;
    if (point <= 0) {
        body[k++] = '0'; body[k++] = '.';
        for (int i = 0; i < -point; i++) body[k++] = '0';
        for (int i = 0; i < n; i++) body[k++] = digits[i];
    } else if (point >= n) {
        for (int i = 0; i < n; i++) body[k++] = digits[i];
        for (int i = 0; i < point - n; i++) body[k++] = '0';
        if (fabs(x) < 1e16) { body[k++] = '.'; body[k++] = '0'; }
    } else {
        for (int i = 0; i < point; i++) body[k++] = digits[i];
        body[k++] = '.';
        for (int i = point; i < n; i++) body[k++] = digits[i];
    }
    body[k] = '\0';
    snprintf(out, cap, "%s%s", neg ? "-" : "", body);
}

static void bv_display(BV v) {
    char buf[420];
    switch (v.tag) {
        case BT_INT:   printf("%lld", (long long)v.i); break;
        case BT_FLOAT: bv_fmt_float(v.f, buf, sizeof buf); printf("%s", buf); break;
        case BT_BOOL:  printf("%s",   v.b ? "true" : "false"); break;
        case BT_NIL:   printf("nil"); break;
        case BT_STR:   printf("%s",   v.s); break;
    }
}

static void bv_print(BV v)                  { bv_display(v); printf("\n"); }
static void bv_print_n(BV* a, int n) {
    for (int i = 0; i < n; i++) { if (i) printf(" "); bv_display(a[i]); }
    printf("\n");
}

static int bv_is_num(BV v) { return v.tag == BT_INT || v.tag == BT_FLOAT; }
static double bv_to_f(BV v, const char* op) {
    if (v.tag==BT_FLOAT) return v.f;
    if (v.tag==BT_INT)   return (double)v.i;
    char m[96]; snprintf(m, sizeof m, "%s: 숫자 필요", op); bv_fail(m); return 0;
}
static void bv_overflow(const char* op) {
    char m[96]; snprintf(m, sizeof m, "정수 오버플로: %s 결과가 64비트 범위를 벗어남", op); bv_fail(m);
}
/* float → int: Rust `as i64`와 같이 범위 밖은 포화, NaN은 0 */
static int64_t bv_f2i(double f) {
    if (isnan(f)) return 0;
    if (f >= 9223372036854775807.0) return INT64_MAX;
    if (f <= -9223372036854775808.0) return INT64_MIN;
    return (int64_t)f;
}

static BV bv_add(BV a, BV b) {
    if (a.tag==BT_INT && b.tag==BT_INT) { int64_t r; if (__builtin_add_overflow(a.i, b.i, &r)) bv_overflow("+"); return BINT(r); }
    if (bv_is_num(a) && bv_is_num(b)) return BFLT(bv_to_f(a,"+")+bv_to_f(b,"+"));
    if (a.tag==BT_STR && b.tag==BT_STR) {
        size_t la=strlen(a.s), lb=strlen(b.s);
        char* r=(char*)malloc(la+lb+1);
        if (!r) bv_fail("메모리 부족");
        memcpy(r,a.s,la); memcpy(r+la,b.s,lb+1);
        return BSTR(r);
    }
    bv_fail("+: 연산 불가 (타입이 맞지 않음)"); return BNIL;
}
static BV bv_sub(BV a,BV b){
    if(a.tag==BT_INT&&b.tag==BT_INT){int64_t r;if(__builtin_sub_overflow(a.i,b.i,&r))bv_overflow("-");return BINT(r);}
    return BFLT(bv_to_f(a,"-")-bv_to_f(b,"-"));
}
static BV bv_mul(BV a,BV b){
    if(a.tag==BT_INT&&b.tag==BT_INT){int64_t r;if(__builtin_mul_overflow(a.i,b.i,&r))bv_overflow("*");return BINT(r);}
    return BFLT(bv_to_f(a,"*")*bv_to_f(b,"*"));
}
/* `/`는 항상 float. int/int에서 나누는 수가 0이면 에러, float이 섞이면 IEEE(inf/NaN) — VM과 같음 */
static BV bv_div(BV a,BV b){
    if(a.tag==BT_INT&&b.tag==BT_INT){ if(!b.i) bv_fail("0으로 나눌 수 없음"); return BFLT((double)a.i/(double)b.i); }
    return BFLT(bv_to_f(a,"/")/bv_to_f(b,"/"));
}
static BV bv_mod(BV a,BV b){
    if(a.tag==BT_INT&&b.tag==BT_INT){
        if(!b.i) bv_fail("나머지: 0으로 나눌 수 없음");
        if(a.i==INT64_MIN && b.i==-1) bv_overflow("%");
        return BINT(a.i%b.i);
    }
    return BFLT(fmod(bv_to_f(a,"%"),bv_to_f(b,"%")));
}
static BV bv_neg(BV a){
    if(a.tag==BT_INT){ if(a.i==INT64_MIN) bv_overflow("단항 -"); return BINT(-a.i); }
    if(a.tag==BT_FLOAT)return BFLT(-a.f);
    bv_fail("단항 -: 숫자 필요"); return BNIL;
}
static BV bv_not_v(BV a){return BBOOL(!bv_truthy(a));}
static BV bv_and_v(BV a,BV b){return bv_truthy(a)?b:a;}
static BV bv_or_v(BV a,BV b){ return bv_truthy(a)?a:b;}

static int bv_eq(BV a,BV b){
    if(a.tag==BT_INT&&b.tag==BT_INT)return a.i==b.i;
    if(bv_is_num(a)&&bv_is_num(b))return bv_to_f(a,"==")==bv_to_f(b,"==");
    if(a.tag==BT_BOOL&&b.tag==BT_BOOL)return a.b==b.b;
    if(a.tag==BT_NIL&&b.tag==BT_NIL)return 1;
    if(a.tag==BT_STR&&b.tag==BT_STR)return strcmp(a.s,b.s)==0;
    return 0;
}
/* 비교: 숫자끼리, 문자열끼리(바이트 순 — VM의 Rust String 비교와 같음). 그 밖은 에러 */
static int bv_cmp(BV a,BV b,const char* op){
    if(a.tag==BT_INT&&b.tag==BT_INT)return (a.i>b.i)-(a.i<b.i);
    if(bv_is_num(a)&&bv_is_num(b)){double x=bv_to_f(a,op),y=bv_to_f(b,op);return (x>y)-(x<y);}
    if(a.tag==BT_STR&&b.tag==BT_STR){int c=strcmp(a.s,b.s);return (c>0)-(c<0);}
    bv_fail("비교: 비교할 수 없는 타입"); return 0;
}
static int bv_lt(BV a,BV b){return bv_cmp(a,b,"<")<0;}
static int bv_le(BV a,BV b){return bv_cmp(a,b,"<=")<=0;}
static int bv_gt(BV a,BV b){return bv_cmp(a,b,">")>0;}
static int bv_ge(BV a,BV b){return bv_cmp(a,b,">=")>=0;}

static BV bv_str_cast(BV v){
    char buf[420];
    switch(v.tag){
        case BT_STR: return v;
        case BT_INT: snprintf(buf,sizeof buf,"%lld",(long long)v.i); break;
        case BT_FLOAT: bv_fmt_float(v.f,buf,sizeof buf); break;
        case BT_BOOL: return BSTR(v.b?"true":"false");
        case BT_NIL: return BSTR("nil");
        default: return BSTR("");
    }
    char*r=strdup(buf); if(!r) bv_fail("메모리 부족"); return BSTR(r);
}
/* 앞뒤 공백을 뺀 문자열 전체가 숫자여야 변환 (VM의 trim + parse와 같음) */
static const char* bv_trim_copy(const char* s, char* buf, size_t cap){
    while(*s==' '||*s=='\t'||*s=='\n'||*s=='\r') s++;
    size_t n=strlen(s); while(n>0&&(s[n-1]==' '||s[n-1]=='\t'||s[n-1]=='\n'||s[n-1]=='\r')) n--;
    if(n>=cap) n=cap-1; memcpy(buf,s,n); buf[n]='\0'; return buf;
}
static BV bv_int_cast(BV v){
    if(v.tag==BT_INT)return v;
    if(v.tag==BT_FLOAT)return BINT(bv_f2i(v.f));
    if(v.tag==BT_BOOL)return BINT(v.b?1:0);
    if(v.tag==BT_STR){
        char t[128]; const char* s=bv_trim_copy(v.s,t,sizeof t); char* end; errno=0;
        long long n=strtoll(s,&end,10);
        if(!*s||*end||errno){char m[200];snprintf(m,sizeof m,"int 변환 실패: '%s'",v.s);bv_fail(m);}
        return BINT(n);
    }
    bv_fail("int(): 변환할 수 없는 타입"); return BNIL;
}
static BV bv_float_cast(BV v){
    if(v.tag==BT_FLOAT)return v;
    if(v.tag==BT_INT)return BFLT((double)v.i);
    if(v.tag==BT_STR){
        char t[128]; const char* s=bv_trim_copy(v.s,t,sizeof t); char* end;
        double f=strtod(s,&end);
        if(!*s||*end||(s[0]=='0'&&(s[1]=='x'||s[1]=='X'))){char m[200];snprintf(m,sizeof m,"float 변환 실패: '%s'",v.s);bv_fail(m);}
        return BFLT(f);
    }
    bv_fail("float(): 변환할 수 없는 타입"); return BNIL;
}
static BV bv_bool_cast(BV v){ return BBOOL(bv_truthy(v)); }
/* len: 문자 수 (UTF-8 바이트가 아니라 — VM과 같음) */
static BV bv_len_f(BV v){
    if(v.tag!=BT_STR) bv_fail("len(): str 필요");
    int64_t n=0; for(const unsigned char* p=(const unsigned char*)v.s;*p;p++) if((*p&0xC0)!=0x80) n++;
    return BINT(n);
}
static BV bv_abs_f(BV v){
    if(v.tag==BT_INT){ if(v.i==INT64_MIN) bv_overflow("abs"); return BINT(v.i<0?-v.i:v.i); }
    return BFLT(fabs(bv_to_f(v,"abs")));
}
static BV bv_sqrt_f(BV v){return BFLT(sqrt(bv_to_f(v,"sqrt")));}
/* floor/ceil/round는 int를 돌려준다 (VM과 같음). round는 0에서 먼 쪽으로 반올림 */
static BV bv_floor_f(BV v){return BINT(bv_f2i(floor(bv_to_f(v,"floor"))));}
static BV bv_ceil_f(BV v){return BINT(bv_f2i(ceil(bv_to_f(v,"ceil"))));}
static BV bv_round_f(BV v){return BINT(bv_f2i(round(bv_to_f(v,"round"))));}
/* pow: int^int(지수 ≥ 0)이고 결과가 정수 범위면 int, 그 외 float (VM과 같음) */
static BV bv_pow_f(BV a,BV b){
    double r=pow(bv_to_f(a,"pow"),bv_to_f(b,"pow"));
    if(a.tag==BT_INT&&b.tag==BT_INT&&b.i>=0&&r==floor(r)&&fabs(r)<9223372036854775807.0) return BINT((int64_t)r);
    return BFLT(r);
}
static BV bv_min_f(BV a,BV b){return bv_lt(a,b)?a:b;}
static BV bv_max_f(BV a,BV b){return bv_gt(a,b)?a:b;}
"#;

// ============================================================================
// 트랜스파일러
// ============================================================================

pub struct Transpiler {
    out:      String,
    indent:   usize,
    errors:   Vec<TranspileError>,
    /// 최상위 사용자 함수 이름 집합 (forward-decl 및 call-site 구분용)
    user_fns: HashSet<String>,
    /// 현재 스코프에서 이미 선언된 변수 이름 (C 재선언 방지)
    scope_stack: Vec<HashSet<String>>,
}

impl Transpiler {
    fn new() -> Self {
        Self {
            out:         String::with_capacity(4096),
            indent:      0,
            errors:      Vec::new(),
            user_fns:    HashSet::new(),
            scope_stack: Vec::new(),
        }
    }

    // ---- 출력 헬퍼 ----

    fn pad(&self) -> String { "    ".repeat(self.indent) }

    fn push_line(&mut self, s: &str) {
        let pad = self.pad();
        self.out.push_str(&pad);
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn push_raw(&mut self, s: &str) { self.out.push_str(s); }

    fn err(&mut self, msg: impl Into<String>, span: Span) {
        self.errors.push(TranspileError::new(msg, span));
    }

    // ---- 스코프 관리 ----

    fn enter_scope(&mut self) { self.scope_stack.push(HashSet::new()); }
    fn leave_scope(&mut self) { self.scope_stack.pop(); }

    /// 현재 스코프에서 이름이 이미 선언됐으면 false, 처음이면 true + 등록
    fn try_declare(&mut self, name: &str) -> bool {
        if let Some(top) = self.scope_stack.last_mut() {
            if top.contains(name) { return false; }
            top.insert(name.to_string());
        }
        true
    }

    // ---- 1차 패스: 최상위 함수 이름 수집 ----

    fn collect_fns(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            if let StmtKind::Let { name, value, .. } = &stmt.kind {
                if matches!(value.kind, ExprKind::Function { .. }) {
                    self.user_fns.insert(name.clone());
                }
            }
        }
    }

    // ---- 최상위 함수 전방 선언 ----

    fn emit_forward_decls(&mut self, stmts: &[Stmt]) {
        let fns: Vec<(String, Vec<String>)> = stmts
            .iter()
            .filter_map(|s| {
                if let StmtKind::Let { name, value, .. } = &s.kind {
                    if let ExprKind::Function { params, .. } = &value.kind {
                        return Some((name.clone(), params.clone()));
                    }
                }
                None
            })
            .collect();

        if fns.is_empty() { return; }
        self.push_raw("/* 함수 전방 선언 */\n");
        for (name, params) in &fns {
            let param_str = params
                .iter()
                .map(|p| format!("BV b_{p}"))
                .collect::<Vec<_>>()
                .join(", ");
            let param_str = if param_str.is_empty() { "void".to_string() } else { param_str };
            self.push_line(&format!("static BV b_{name}({param_str});"));
        }
        self.push_raw("\n");
    }

    // ---- 최상위 함수 정의 ----

    fn emit_fn_defs(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            if let StmtKind::Let { name, value, .. } = &stmt.kind {
                if let ExprKind::Function { params, body, param_types, param_defaults, ret_type, .. } = &value.kind {
                    if param_defaults.iter().any(Option::is_some) {
                        self.err("AOT: 매개변수 기본값 미지원", stmt.span);
                    }
                    // VM은 타입 힌트를 런타임 검증한다. AOT가 무시하면 같은 프로그램의 결과가
                    // 달라지므로(조용한 불일치) 명확한 에러로 거절한다.
                    if param_types.iter().any(Option::is_some) || ret_type.is_some() {
                        self.err("AOT: 타입 힌트 미지원 (VM은 런타임 검증 — 힌트를 지우거나 bang run 사용)", stmt.span);
                    }
                    self.emit_fn_def(name, params, body);
                }
            }
        }
    }

    fn emit_fn_def(&mut self, name: &str, params: &[String], body: &Block) {
        let param_str = params
            .iter()
            .map(|p| format!("BV b_{p}"))
            .collect::<Vec<_>>()
            .join(", ");
        let param_str = if param_str.is_empty() { "void".to_string() } else { param_str };
        self.push_line(&format!("static BV b_{name}({param_str}) {{"));
        self.indent += 1;
        self.enter_scope();
        // 파라미터를 현재 스코프에 등록 (let 으로 재선언되지 않도록)
        if let Some(top) = self.scope_stack.last_mut() {
            for p in params { top.insert(p.clone()); }
        }
        self.emit_stmts(&body.stmts);
        // 암묵적 nil 반환
        self.push_line("return BNIL;");
        self.leave_scope();
        self.indent -= 1;
        self.push_line("}\n");
    }

    // ---- 문 목록 ----

    fn emit_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.emit_stmt(stmt);
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        let sp = stmt.span;
        match &stmt.kind {
            StmtKind::LetUnpack { .. } => {
                self.err("AOT: 분해 선언(let [a, b] = ...) 미지원", sp);
            }
            StmtKind::Let { name, value, ty } => {
                if ty.is_some() {
                    self.err("AOT: 타입 힌트 미지원 (VM은 런타임 검증 — 힌트를 지우거나 bang run 사용)", sp);
                }
                // 최상위 함수 정의는 이미 처리 → 건너뜀
                if matches!(value.kind, ExprKind::Function { .. }) && self.scope_stack.len() == 1 {
                    return;
                }
                // 내부 함수(중첩 fn)는 비지원
                if matches!(value.kind, ExprKind::Function { .. }) {
                    self.err("내부 함수(클로저) AOT 미지원", sp);
                    return;
                }
                let expr = self.emit_expr(value);
                if self.try_declare(name) {
                    self.push_line(&format!("BV b_{name} = {expr};"));
                } else {
                    self.push_line(&format!("b_{name} = {expr};"));
                }
            }

            StmtKind::Expr(e) => {
                match &e.kind {
                    // 대입식 → 문장 레벨에서 처리 (= 연산자)
                    ExprKind::Assign { target, value } => {
                        let val_e = self.emit_expr(value);
                        match &target.kind {
                            ExprKind::Ident(name) => {
                                self.push_line(&format!("b_{name} = {val_e};"));
                            }
                            _ => { self.err("AOT: 지원하지 않는 대입 대상", sp); }
                        }
                    }
                    // print() 등 내장 함수 호출 → 문 레벨에서 직접 emit
                    ExprKind::Call { callee, args } => {
                        if let Some(builtin_stmt) = self.maybe_builtin_stmt(callee, args, sp) {
                            self.push_line(&format!("{builtin_stmt};"));
                        } else {
                            let expr = self.emit_expr(e);
                            self.push_line(&format!("{expr};"));
                        }
                    }
                    _ => {
                        let expr = self.emit_expr(e);
                        self.push_line(&format!("{expr};"));
                    }
                }
            }

            StmtKind::Return(val) => {
                let v = val
                    .as_ref()
                    .map(|e| self.emit_expr(e))
                    .unwrap_or_else(|| "BNIL".to_string());
                self.push_line(&format!("return {v};"));
            }

            StmtKind::If { cond, then, else_ } => {
                let cond_e = self.emit_expr(cond);
                self.push_line(&format!("if (bv_truthy({cond_e})) {{"));
                self.indent += 1;
                self.enter_scope();
                self.emit_stmts(&then.stmts);
                self.leave_scope();
                self.indent -= 1;
                if let Some(el) = else_ {
                    self.push_line("} else {");
                    self.indent += 1;
                    self.enter_scope();
                    self.emit_stmts(&el.stmts);
                    self.leave_scope();
                    self.indent -= 1;
                }
                self.push_line("}");
            }

            StmtKind::While { cond, body } => {
                let cond_e = self.emit_expr(cond);
                self.push_line(&format!("while (bv_truthy({cond_e})) {{"));
                self.indent += 1;
                self.enter_scope();
                self.emit_stmts(&body.stmts);
                self.leave_scope();
                self.indent -= 1;
                self.push_line("}");
            }

            StmtKind::Block(b) => {
                self.push_line("{");
                self.indent += 1;
                self.enter_scope();
                self.emit_stmts(&b.stmts);
                self.leave_scope();
                self.indent -= 1;
                self.push_line("}");
            }

            StmtKind::Break    => { self.push_line("break;"); }
            StmtKind::Continue => { self.push_line("continue;"); }

            StmtKind::For { .. } => {
                self.err("for-in AOT 미지원 (while + 인덱스 변수 사용 권장)", sp);
            }
            StmtKind::Parallel(_) => {
                self.err("parallel AOT 미지원", sp);
            }
            StmtKind::Try { .. } => {
                self.err("try/catch AOT 미지원", sp);
            }
            StmtKind::Throw(_) => {
                self.err("throw AOT 미지원", sp);
            }
        }
    }

    // ---- 내장 함수 문장 레벨 emit (print 등 반환값 버리는 경우) ----

    fn maybe_builtin_stmt(&mut self, callee: &Expr, args: &[Expr], _sp: Span) -> Option<String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.as_str(),
            _ => return None,
        };
        match name {
            "print" => {
                let exprs: Vec<String> = args.iter().map(|a| self.emit_expr(a)).collect();
                if exprs.is_empty() {
                    Some("printf(\"\\n\")".to_string())
                } else if exprs.len() == 1 {
                    Some(format!("bv_print({})", exprs[0]))
                } else {
                    let arr = exprs.join(", ");
                    Some(format!("{{ BV __pa[] = {{{arr}}}; bv_print_n(__pa, {}); }}", exprs.len()))
                }
            }
            _ => None,
        }
    }

    // ---- 식 emit → C 표현식 문자열 반환 ----

    fn emit_expr(&mut self, expr: &Expr) -> String {
        let sp = expr.span;
        match &expr.kind {
            ExprKind::Int(n)    => format!("BINT({n})"),
            // 지수 표기({:e})로 써야 C가 항상 double 리터럴로 읽는다 ("1e21"; 이전엔 정수 상수로 넘침)
            ExprKind::Float(n) if n.is_finite() => format!("BFLT({n:e})"),
            ExprKind::Float(n) => format!("BFLT({})", if n.is_nan() { "NAN" } else if *n > 0.0 { "INFINITY" } else { "-INFINITY" }),
            ExprKind::Bool(b)   => if *b { "BBOOL(1)".into() } else { "BBOOL(0)".into() },
            ExprKind::Nil       => "BNIL".into(),
            ExprKind::Str(s)    => format!("BSTR({})", c_str_literal(s)),

            ExprKind::Ident(name) => format!("b_{name}"),

            ExprKind::Unary { op, expr } => {
                let inner = self.emit_expr(expr);
                match op {
                    UnaryOp::Neg => format!("bv_neg({inner})"),
                    UnaryOp::Not => format!("bv_not_v({inner})"),
                }
            }

            ExprKind::Binary { op, left, right } => {
                let l = self.emit_expr(left);
                let r = self.emit_expr(right);
                match op {
                    BinaryOp::Add => format!("bv_add({l},{r})"),
                    BinaryOp::Sub => format!("bv_sub({l},{r})"),
                    BinaryOp::Mul => format!("bv_mul({l},{r})"),
                    BinaryOp::Div => format!("bv_div({l},{r})"),
                    BinaryOp::Mod => format!("bv_mod({l},{r})"),
                    BinaryOp::Eq  => format!("BBOOL(bv_eq({l},{r}))"),
                    BinaryOp::Ne  => format!("BBOOL(!bv_eq({l},{r}))"),
                    BinaryOp::Lt  => format!("BBOOL(bv_lt({l},{r}))"),
                    BinaryOp::Le  => format!("BBOOL(bv_le({l},{r}))"),
                    BinaryOp::Gt  => format!("BBOOL(bv_gt({l},{r}))"),
                    BinaryOp::Ge  => format!("BBOOL(bv_ge({l},{r}))"),
                    BinaryOp::And => format!("bv_and_v({l},{r})"),
                    BinaryOp::Or  => format!("bv_or_v({l},{r})"),
                    BinaryOp::In | BinaryOp::NotIn => {
                        self.err("AOT: in / not in 연산자 미지원", sp);
                        "BNIL".into()
                    }
                }
            }

            ExprKind::CompoundAssign { op, target, value } => {
                let val_e = self.emit_expr(value);
                let f = match op {
                    BinaryOp::Add => "bv_add", BinaryOp::Sub => "bv_sub",
                    BinaryOp::Mul => "bv_mul", BinaryOp::Div => "bv_div",
                    BinaryOp::Mod => "bv_mod",
                    _ => {
                        self.err("AOT: 지원하지 않는 복합 대입", sp);
                        return "BNIL".into();
                    }
                };
                match &target.kind {
                    ExprKind::Ident(name) => format!("(b_{name} = {f}(b_{name},{val_e}), b_{name})"),
                    _ => {
                        self.err("AOT: 지원하지 않는 대입 대상", sp);
                        "BNIL".into()
                    }
                }
            }

            ExprKind::Slice { .. } => {
                self.err("AOT: 슬라이스 미지원", sp);
                "BNIL".into()
            }

            ExprKind::FString(_) => {
                self.err("AOT: f-문자열 미지원 (+ 와 str() 사용)", sp);
                "BNIL".into()
            }

            ExprKind::IfExpr { .. } => {
                self.err("AOT: if 식 미지원 (if 문 사용)", sp);
                "BNIL".into()
            }

            ExprKind::Call { callee, args } => {
                self.emit_call(callee, args, sp)
            }

            ExprKind::Assign { target, value } => {
                // 식으로서의 대입 (a = b) — 값을 반환
                let val_e = self.emit_expr(value);
                match &target.kind {
                    ExprKind::Ident(name) => {
                        // C comma expression: (b_name = val, b_name)
                        format!("(b_{name} = {val_e}, b_{name})")
                    }
                    _ => {
                        self.err("AOT: 지원하지 않는 대입 대상", sp);
                        "BNIL".into()
                    }
                }
            }

            ExprKind::Function { .. } => {
                self.err("AOT: 익명 함수/클로저 미지원", sp);
                "BNIL".into()
            }
            ExprKind::Spawn(_) => {
                self.err("AOT: spawn 미지원", sp);
                "BNIL".into()
            }
            ExprKind::List(_) => {
                self.err("AOT: List 리터럴 미지원 (Phase 10에서 제외)", sp);
                "BNIL".into()
            }
            ExprKind::Map(_) => {
                self.err("AOT: Map 리터럴 미지원 (Phase 10에서 제외)", sp);
                "BNIL".into()
            }
            ExprKind::Index { .. } => {
                self.err("AOT: 인덱스 접근 미지원", sp);
                "BNIL".into()
            }
            ExprKind::Field { .. } => {
                self.err("AOT: 필드 접근 미지원", sp);
                "BNIL".into()
            }
        }
    }

    // ---- 함수 호출 emit ----

    fn emit_call(&mut self, callee: &Expr, args: &[Expr], sp: Span) -> String {
        // 내장 함수 처리
        if let ExprKind::Ident(name) = &callee.kind {
            let arg_exprs: Vec<String> = args.iter().map(|a| self.emit_expr(a)).collect();
            let a0 = || arg_exprs.first().cloned().unwrap_or_else(|| "BNIL".into());
            let a1 = || arg_exprs.get(1).cloned().unwrap_or_else(|| "BNIL".into());

            match name.as_str() {
                "print" => {
                    if arg_exprs.is_empty() {
                        return "((void)printf(\"\\n\"), BNIL)".into();
                    } else if arg_exprs.len() == 1 {
                        return format!("((void)bv_print({}), BNIL)", a0());
                    } else {
                        let arr = arg_exprs.join(", ");
                        return format!("({{ BV __pa[] = {{{arr}}}; bv_print_n(__pa, {}); BNIL; }})", arg_exprs.len());
                    }
                }
                "str"   => return format!("bv_str_cast({})", a0()),
                "int"   => return format!("bv_int_cast({})", a0()),
                "float" => return format!("bv_float_cast({})", a0()),
                "bool"  => return format!("bv_bool_cast({})", a0()),
                "len"   => return format!("bv_len_f({})", a0()),
                "abs"   => return format!("bv_abs_f({})", a0()),
                "sqrt"  => return format!("bv_sqrt_f({})", a0()),
                "floor" => return format!("bv_floor_f({})", a0()),
                "ceil"  => return format!("bv_ceil_f({})", a0()),
                "round" => return format!("bv_round_f({})", a0()),
                "pow"   => return format!("bv_pow_f({},{})", a0(), a1()),
                "min" | "max" if arg_exprs.len() != 2 => {
                    self.err("AOT: min/max는 인자 2개만 지원 (리스트 인자 미지원)", sp);
                    return "BNIL".into();
                }
                "min"   => return format!("bv_min_f({},{})", a0(), a1()),
                "max"   => return format!("bv_max_f({},{})", a0(), a1()),
                "exit" if arg_exprs.is_empty() => return "((void)exit(0), BNIL)".into(),
                "exit"  => return format!("((void)exit((int)bv_int_cast({}).i), BNIL)", a0()),
                "assert" => {
                    // VM과 같은 메시지: "assertion 실패" 또는 "assertion 실패: <메시지>"
                    let cond = a0();
                    let fail = if arg_exprs.len() >= 2 {
                        format!("(fprintf(stderr,\"런타임 오류: assertion 실패: %s\\n\",bv_str_cast({}).s),exit(1),0)", a1())
                    } else {
                        "(fprintf(stderr,\"런타임 오류: assertion 실패\\n\"),exit(1),0)".to_string()
                    };
                    return format!("((void)(bv_truthy({cond})||{fail}), BNIL)");
                }
                _ if self.user_fns.contains(name.as_str()) => {
                    let args_s = arg_exprs.join(", ");
                    return format!("b_{name}({args_s})");
                }
                _ => {
                    self.err(format!("AOT: 알 수 없는 함수 '{name}'"), sp);
                    return "BNIL".into();
                }
            }
        }

        // 클로저 호출 등 비지원
        self.err("AOT: 동적 함수 호출(클로저) 미지원", sp);
        "BNIL".into()
    }

    // ---- 전체 프로그램 변환 ----

    fn run(&mut self, prog: &Program) {
        // 1. 최상위 함수 이름 수집
        self.collect_fns(&prog.stmts);

        // 2. 런타임 헤더 emit
        self.push_raw(C_RUNTIME);
        self.push_raw("\n");

        // 3. 전방 선언
        self.emit_forward_decls(&prog.stmts);

        // 4. 함수 정의
        self.emit_fn_defs(&prog.stmts);

        // 5. main()
        self.push_raw("int main(void) {\n");
        self.indent = 1;
        self.enter_scope();

        for stmt in &prog.stmts {
            // 최상위 함수 let 바인딩은 이미 처리됐으므로 건너뜀
            if let StmtKind::Let { value, .. } = &stmt.kind {
                if matches!(value.kind, ExprKind::Function { .. }) {
                    continue;
                }
            }
            self.emit_stmt(stmt);
        }

        self.leave_scope();
        self.indent = 0;
        self.push_raw("    return 0;\n}\n");
    }
}

// ============================================================================
// 공개 API
// ============================================================================

/// Bang AST를 C11 소스 코드로 변환한다.
pub fn transpile(prog: &Program) -> Result<String, Vec<TranspileError>> {
    let mut t = Transpiler::new();
    t.run(prog);
    if t.errors.is_empty() {
        Ok(t.out)
    } else {
        Err(t.errors)
    }
}

// ============================================================================
// 헬퍼
// ============================================================================

/// Bang 문자열 리터럴을 C 문자열 리터럴로 변환한다.
fn c_str_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 32 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

// ============================================================================
// 단위 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse(src: &str) -> Program {
        let tokens = Lexer::new(src).tokenize().expect("tokenize 실패");
        Parser::new(tokens).parse().expect("parse 실패")
    }

    /// 타입 힌트는 VM에서 런타임 검증되므로 AOT가 무시하면 결과가 달라진다 → 명확한 에러.
    #[test]
    fn test_transpile_rejects_type_hints() {
        for src in [
            "let x: int = \"oops\"\nprint(x)",
            "fn f(a: int) { return a }\nprint(f(1))",
            "fn g(a) -> int { return a }\nprint(g(1))",
        ] {
            let errs = transpile(&parse(src)).expect_err(src);
            assert!(errs.iter().any(|e| e.to_string().contains("타입 힌트")), "{src}: {errs:?}");
        }
    }

    #[test]
    fn test_transpile_hello() {
        let prog = parse(r#"print("hello")"#);
        let c = transpile(&prog).expect("transpile 실패");
        assert!(c.contains("bv_print(BSTR(\"hello\"))"), "got: {c}");
    }

    #[test]
    fn test_transpile_arithmetic() {
        let prog = parse("print(1 + 2 * 3)");
        let c = transpile(&prog).expect("transpile 실패");
        assert!(c.contains("bv_add") && c.contains("bv_mul"), "got: {c}");
    }

    #[test]
    fn test_transpile_fn() {
        let prog = parse("fn add(a, b) { return a + b }\nprint(add(1, 2))");
        let c = transpile(&prog).expect("transpile 실패");
        assert!(c.contains("static BV b_add") && c.contains("b_add("), "got: {c}");
    }

    #[test]
    fn test_transpile_while() {
        let prog = parse("let i = 0\nwhile i < 3 { print(i)\ni = i + 1 }");
        let c = transpile(&prog).expect("transpile 실패");
        assert!(c.contains("while") && c.contains("bv_lt"), "got: {c}");
    }
}
