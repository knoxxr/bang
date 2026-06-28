# Bang 언어 명세 (Language Specification)

> **버전**: 0.1-draft  
> **상태**: 점진적 확장 — Phase별로 섹션이 추가된다.

---

## 1. 개요

Bang은 동적 타입 범용 프로그래밍 언어이다.
**투명 동시성(Transparent Concurrency)** 모델을 핵심 차별점으로 갖는다.

- 소스 확장자: `.bang`
- CLI: `bang run <file>`, `bang repl`
- 구현 언어: Rust (edition 2021)

---

## 2. 어휘 구조 (Lexical Structure)

### 2.1 문자 집합

소스 코드는 UTF-8 인코딩이다. 식별자는 ASCII 영문자·숫자·`_`로 구성된다.

### 2.2 주석

```
// 한 줄 주석 (줄 끝까지)
```

블록 주석은 지원하지 않는다.

### 2.3 키워드

```
let   fn    if    else   while  for   in    return
break continue
and   or    not
spawn parallel
try   catch  throw
true  false  nil
```

`true`, `false`, `nil`은 키워드이자 리터럴 값이다.

### 2.4 내장 식별자 (키워드 아님)

다음은 표준 라이브러리가 제공하는 내장 함수/값이다. 키워드가 아니므로
사용자가 같은 이름으로 변수를 선언할 수 있다(권장하지 않음).

```
print  len  str  type  assert
channel  send  recv  close  wait  parallel_map
```

### 2.5 문장 종결 규칙 (세미콜론 없음)

문장은 줄바꿈(newline)으로 종결된다. 세미콜론은 사용하지 않는다.

줄바꿈이 문장 종결자로 작동하는 조건 (Go-style 자동 삽입):

| 직전 토큰 | 줄바꿈 → 문장 종결? |
|-----------|:------------------:|
| 리터럴 (int, float, string, true, false, nil) | ✅ |
| 식별자 | ✅ |
| `return`, `break`, `continue` | ✅ |
| `)`, `]`, `}` | ✅ |
| 연산자 (`+`, `-`, `*`, `/`, `%`, `=`, 비교, 논리) | ❌ (다음 줄로 계속) |
| `(`, `[`, `{`, `,`, `:`, `.` | ❌ |
| `let`, `fn`, `if`, `else`, `while`, `for`, `in`, `spawn`, `parallel` | ❌ |

---

## 3. 타입 시스템

Bang은 동적 타입 언어이다. 변수에 타입 선언이 없고,
실행 시점에 값의 타입이 결정된다.

### 3.1 기본 타입

| 타입 | 설명 | 리터럴 예시 |
|------|------|-------------|
| `int` | 64비트 정수 | `42`, `0`, `-7` |
| `float` | 64비트 부동소수점 | `3.14`, `0.5` |
| `bool` | 불리언 | `true`, `false` |
| `string` | 불변 문자열 | `"hello"`, `"a\nb"` |
| `nil` | 값 없음 | `nil` |

### 3.2 컬렉션 타입

| 타입 | 설명 | 리터럴 예시 |
|------|------|-------------|
| `list` | 순서 있는 가변 배열 | `[1, 2, 3]`, `[]` |
| `map` | 키-값 매핑 | `{"name": "Alice", "age": 30}` |

### 3.3 함수 타입

함수는 일급 값(first-class value)이다. 변수에 대입, 인자로 전달,
반환값으로 사용할 수 있다. 클로저를 지원한다.

### 3.4 Future 타입

`spawn` 표현식의 결과. 값이 사용되는 시점에 자동으로 대기(await)한다.
사용자가 직접 Future를 생성하거나 조작할 수 없다.

---

## 4. 문법 (Grammar)

### 4.1 EBNF 표기

```ebnf
program        = { statement } EOF ;

(* === 문(Statement) === *)
statement      = let_stmt
               | fn_decl
               | if_stmt
               | while_stmt
               | for_stmt
               | return_stmt
               | break_stmt
               | continue_stmt
               | expr_stmt ;

let_stmt       = "let" IDENT "=" expression NL ;
fn_decl        = "fn" IDENT "(" [ params ] ")" block ;
if_stmt        = "if" expression block
                 { "else" "if" expression block }
                 [ "else" block ] ;
while_stmt     = "while" expression block ;
for_stmt       = "for" IDENT "in" expression block ;
return_stmt    = "return" [ expression ] NL ;
break_stmt     = "break" NL ;
continue_stmt  = "continue" NL ;
expr_stmt      = expression NL ;

block          = "{" { statement } "}" ;
params         = IDENT { "," IDENT } ;

(* === 식(Expression) — 우선순위 낮은 순 === *)
expression     = assignment ;
assignment     = IDENT "=" assignment
               | logic_or ;
logic_or       = logic_and { "or" logic_and } ;
logic_and      = equality { "and" equality } ;
equality       = comparison { ( "==" | "!=" ) comparison } ;
comparison     = addition { ( "<" | "<=" | ">" | ">=" ) addition } ;
addition       = multiplication { ( "+" | "-" ) multiplication } ;
multiplication = unary { ( "*" | "/" | "%" ) unary } ;
unary          = ( "not" | "-" ) unary
               | postfix ;
postfix        = primary { call_or_index } ;
call_or_index  = "(" [ arguments ] ")"    (* 함수 호출 *)
               | "[" expression "]"        (* 인덱싱 *)
               | "." IDENT ;              (* 멤버 접근 *)

primary        = INT_LIT | FLOAT_LIT | STRING_LIT
               | "true" | "false" | "nil"
               | IDENT
               | "(" expression ")"
               | list_literal
               | map_literal
               | spawn_expr
               | lambda
               | parallel_block ;

list_literal   = "[" [ expression { "," expression } ] "]" ;
map_literal    = "{" [ map_entry { "," map_entry } ] "}" ;
map_entry      = expression ":" expression ;
spawn_expr     = "spawn" expression ;
lambda         = "fn" "(" [ params ] ")" block ;
parallel_block = "parallel" block ;

arguments      = expression { "," expression } ;

(* === 토큰 === *)
NL             = <자동 삽입 줄바꿈> ;
IDENT          = ( letter | "_" ) { letter | digit | "_" } ;
INT_LIT        = digit { digit } ;
FLOAT_LIT      = digit { digit } "." digit { digit } ;
STRING_LIT     = '"' { char | escape } '"' ;
escape         = "\\" ( "n" | "t" | "\\" | '"' ) ;
```

### 4.2 대입과 인덱스 대입

```bang
let x = 1        // 변수 선언 (let 필수)
x = 2            // 재대입 (let 불필요)
list[0] = 10     // 인덱스 대입
map["key"] = v   // 맵 키 대입
```

> **주의**: `let`은 선언, `=`은 재대입. 선언되지 않은 변수에 `=`은 에러.

---

## 5. 연산자 우선순위

| 우선순위 | 연산자 | 결합 방향 | 설명 |
|:--------:|--------|:---------:|------|
| 1 (최저) | `=` | 우→좌 | 대입 |
| 2 | `or` | 좌→우 | 논리합 (단락 평가) |
| 3 | `and` | 좌→우 | 논리곱 (단락 평가) |
| 4 | `==`  `!=` | 좌→우 | 동등 비교 |
| 5 | `<`  `<=`  `>`  `>=` | 좌→우 | 순서 비교 |
| 6 | `+`  `-` | 좌→우 | 덧셈, 뺄셈, 문자열 연결 |
| 7 | `*`  `/`  `%` | 좌→우 | 곱셈, 나눗셈, 나머지 |
| 8 | `-` (단항)  `not` | 우→좌 | 산술 부정, 논리 부정 |
| 9 (최고) | `()`  `[]`  `.` | 좌→우 | 호출, 인덱싱, 멤버 |

---

## 6. 동시성 모델 (Transparent Concurrency)

### 6.1 설계 원칙

1. **함수 색칠 없음**: async/await 키워드가 없다. 모든 함수는 동기·비동기
   구분 없이 하나의 형태로 작성된다.
2. **값 이동(move)**: `spawn`에 전달되는 값은 이동된다. 공유 가변 상태가
   발생하지 않으므로 락·뮤텍스·데이터 레이스가 사용자에게 노출되지 않는다.
3. **구조적 동시성**: 모든 `spawn`은 가장 가까운 함수 본문 또는
   `parallel` 블록에 뿌리내린다. 스코프 종료 시 자동 조인되어
   작업 누수가 불가능하다.

### 6.2 spawn

```bang
let future = spawn expensive_work(data)
// ... 다른 작업 ...
let result = future + 1   // future 사용 시점에 자동 대기
```

- `spawn <expr>` → 표현식을 새 가상 스레드에서 실행, `Future` 반환.
- Future 값을 연산에 사용하면 자동으로 완료를 기다린다 (암묵적 await).
- 명시적 대기가 필요하면 `wait(future)` 내장 함수 사용.

### 6.3 parallel 블록

```bang
parallel {
    let a = spawn fetch_user(id)
    let b = spawn fetch_posts(id)
}
// 여기서 a, b 모두 완료 보장
```

- `parallel` 블록은 내부의 모든 `spawn`이 완료될 때까지 블록을 벗어나지 않는다.
- 블록 안에서 선언된 변수는 블록 밖에서 접근 가능 (스코프 승격).

### 6.4 채널 (Channel)

```bang
let ch = channel()          // 무한 버퍼 채널 생성
send(ch, value)             // 값 전송
let v = recv(ch)            // 값 수신 (블로킹)
close(ch)                   // 채널 닫기
for msg in ch { ... }       // 채널이 닫힐 때까지 반복 수신
```

- `channel`, `send`, `recv`, `close`는 내장 함수(키워드 아님).
- 값은 이동되어 전달된다.

### 6.5 parallel_map

```bang
let results = parallel_map(items, fn(item) {
    return process(item)
})
```

- 리스트의 각 요소를 병렬로 처리하고 결과를 순서 유지하여 반환.

### 6.6 단계적 구현 메모

| 단계 | 구현 범위 |
|------|-----------|
| Phase 1-2 | 렉서 + 파서 + AST (동시성 문법만 파싱, 실행 안 함) |
| Phase 3 | 트리 워킹 인터프리터 (순차 실행, spawn은 즉시 평가) |
| Phase 4 | 바이트코드 VM + 가상 스레드 기반 진짜 동시성 |
| Phase 5 | 채널, parallel_map, 구조적 동시성 완전 구현 |

---

## 7. 문자열 이스케이프 시퀀스

| 시퀀스 | 의미 |
|--------|------|
| `\n` | 줄바꿈 |
| `\t` | 탭 |
| `\\` | 역슬래시 |
| `\"` | 큰따옴표 |

---

## 8. 에러 처리

모든 에러 메시지는 `[줄:열]` 형식의 소스 위치를 포함한다.

에러 종류:
- **LexError**: 예상하지 못한 문자, 종료되지 않은 문자열, 잘못된 이스케이프
- **ParseError**: 문법 오류 (기대하지 않은 토큰 등)
- **RuntimeError**: 타입 불일치, 정의되지 않은 변수, 0 나눗셈 등

### 8.1 예외 (try / catch / throw)

런타임 에러는 `try`/`catch`로 잡아 복구할 수 있고, `throw`로 임의의 값을 던질 수 있다.

```
try {
    <문장들>
} catch <이름> {
    <문장들>   // <이름>에 던져진 값이 바인딩됨
}

throw <식>
```

규칙:
- `throw <식>`은 임의의 값(문자열·맵·숫자 등)을 예외로 던진다. 현재 함수에서
  즉시 빠져나와 가장 가까운 바깥 `try`의 `catch`로 전파된다. 호출 스택을 가로질러
  전파되므로, 중첩 함수에서 던진 예외도 호출자의 `try`에서 잡힌다.
- 내장 런타임 에러(0 나눗셈, 타입 불일치 등)도 `catch`로 잡힌다. 이 경우 catch
  변수에는 에러 **메시지 문자열**이 바인딩된다.
- `catch` 블록은 그 try 자신의 핸들러로 보호되지 않는다. catch 안에서 다시
  `throw`하면 더 바깥의 `try`로 전파된다(re-throw).
- 잡히지 않은 예외는 프로그램을 비정상 종료시킨다("잡히지 않은 예외: …").
- `try` 본문 안에서의 `return`/`break`/`continue`는 핸들러를 올바르게 정리한다.
- 구현: 기본 실행 엔진인 VM에서 동작한다. 트리워킹 인터프리터(`--interp`)와
  AOT 트랜스파일(`bang compile`)은 미지원이다.

```
fn parse_int(s) {
    let n = int(s)        // 변환 실패 시 런타임 에러
    return n
}

try {
    print(parse_int("42"))     // 42
    print(parse_int("abc"))    // 여기서 에러 발생 → catch로
} catch e {
    print("파싱 실패: " + e)
}
```

## 9. 선택적 타입 힌트 (Gradual Typing)

변수·파라미터·반환값에 선택적으로 타입을 표기할 수 있다. 표기는 **점진적(gradual)**
이어서 생략 가능하며, 표기된 경곗값은 **런타임에 검증**된다.

```
let <이름>: <타입> = <식>
fn <이름>(<p1>: <타입>, <p2>: <타입>) -> <타입> { ... }
```

타입 이름: `int` `float` `bool` `str` `nil` `list` `map` `fn` `any`
(타입 이름은 키워드가 아니라 타입 위치에서만 의미를 갖는 식별자다.)

규칙:
- 힌트가 있으면 값이 바인딩/전달/반환되는 시점에 타입을 검사하고, 불일치 시
  런타임 에러를 낸다. 이 에러는 `try`/`catch`로 잡을 수 있다.
- `any`는 모든 타입을 허용한다(검사 통과).
- 힌트가 없는 변수·파라미터·반환은 검사하지 않는다(완전 동적).
- 검사 대상: 타입이 표기된 `let`의 값, 표기된 파라미터(함수 진입 시),
  표기된 반환 타입(명시적 `return`의 값). 값이 Future면 해소 후 검사한다.
- 구현: 기본 실행 엔진인 VM에서 동작한다(`--interp` / AOT는 힌트를 무시).

```
fn area(w: int, h: int) -> int {
    return w * h
}
print(area(3, 4))        // 12

try {
    let n: int = "oops"  // 타입 불일치 → 런타임 에러
} catch e {
    print(e)             // "타입 불일치: int 기대, Str 받음"
}
```
