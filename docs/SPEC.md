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

**정본 실행 엔진은 바이트코드 VM이다.** 이 명세가 기술하는 의미는 VM 기준이며,
`bang run`·`bang repl`·`import`가 모두 VM으로 실행된다. 다른 백엔드가 VM과 다르게
동작하면 그 백엔드의 결함이다(지원하지 않는 기능은 조용히 무시하지 않고 명확한 에러를 내야 한다).

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
| `int` | 64비트 정수 (오버플로 시 런타임 에러 — 아래 참고) | `42`, `0`, `-7` |
| `float` | 64비트 부동소수점 | `3.14`, `0.5` |
| `bool` | 불리언 | `true`, `false` |
| `string` | 불변 문자열 | `"hello"`, `"a\nb"` |
| `nil` | 값 없음 | `nil` |

**정수 오버플로**: `int` 연산(`+ - * / %`, 단항 `-`, `abs`, `sum`, `gcd` 등)의 결과가
64비트 범위를 벗어나면 **try/catch로 잡을 수 있는 런타임 에러**를 낸다
("정수 오버플로: ..."). 조용히 wrap하지 않으며, debug/release 빌드의 의미가 동일하다.
(bigint 자동 승격은 없다 — 큰 수가 필요하면 float 사용을 고려.)

### 3.2 컬렉션 타입

| 타입 | 설명 | 리터럴 예시 |
|------|------|-------------|
| `list` | 순서 있는 가변 배열 | `[1, 2, 3]`, `[]` |
| `map` | 키-값 매핑 | `{"name": "Alice", "age": 30}` |
| `bytes` | 불변 바이트 열 (11장) | 리터럴 없음 — `bytes_from_str("hi")`, `bytes_from_hex("00ff")` |

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
- 채널로는 **데이터 값만** 보낼 수 있다: `int` `float` `bool` `str` `nil` `list` `map`
  (컨테이너는 안쪽까지 재귀 검사). 함수·채널·Future·TCP 핸들을 보내면 — 리스트/맵
  안에 들어 있어도 — `try`/`catch`로 잡히는 런타임 에러가 난다.
  (v0.27까지는 조용히 `nil`로 바뀌었다. 헌법 7조 "조용한 마법 금지"에 따라 에러로 변경.)

### 6.4-1 select (멀티 채널 대기)

```bang
let r = select([ch1, ch2, ch3])   // 먼저 준비된 채널에서 수신
// r == [index, value]  (index = 입력 리스트에서의 채널 위치)
// 모든 채널이 닫히고 비면 nil
```

여러 채널 중 가장 먼저 값이 도착한 채널에서 수신한다. 폴링 기반 구현이다.

### 6.4-2 spawn 에러 가시성

spawn된 작업이 처리되지 않은 에러로 끝나면 스코프 종료 시(자동 조인)
stderr에 경고를 출력한다. 부모 작업은 중단되지 않는다(구조적 동시성).
에러를 직접 다루려면 spawn 대상 함수 안에서 `try`/`catch`를 쓴다.

### 6.4-3 serve_event — 논블로킹 이벤트 루프 서버

```bang
serve_event(addr, handler)   // handler: fn(req_str) -> resp_str
```

단일 스레드가 리액터(epoll/kqueue/IOCP)로 리스너와 모든 연결의 readiness를
멀티플렉싱한다. 데이터가 도착한 연결만 읽어 요청이 완성되면(헤더 끝 `\r\n\r\n`)
`handler(요청문자열)`을 호출하고, 반환된 문자열(전체 HTTP 응답)을 그대로 쓴다.
keep-alive로 같은 연결을 재사용한다. **유휴 연결은 스레드를 점유하지 않으므로**
적은 스레드로 수많은 동시 연결(C10K)을 유지할 수 있다. 핸들러가 던진 예외는
stderr 경고 후 해당 연결만 닫는다.

**한계 (반드시 인지):**
- **단일 스레드 이벤트 루프**다. 핸들러는 루프 스레드에서 동기 실행되므로,
  **CPU가 무거운 핸들러나 핸들러 내부의 블로킹 호출은 이벤트 루프 전체를 멈춘다**
  (그동안 다른 연결이 처리되지 않음). CPU 바운드 작업은 핸들러 안에서 직접 돌리지 말고,
  현재로서는 `spawn`/스레드풀 기반의 블로킹 서버(`tcp_accept` + `spawn`)를 쓰거나
  작업을 채널로 넘겨 분리하라.
- 요청 파싱은 헤더 끝(`\r\n\r\n`)까지만 자동 수집한다. **요청 바디(POST 등)는
  핸들러 문자열에 일부만/늦게 담길 수 있다** — 큰 바디 처리는 아직 한계.
- 핸들러는 요청마다 독립 실행(`run_spawned`)되어 **요청 간 가변 상태를 공유하지 않는다**
  (공유가 필요하면 채널/외부 저장소 사용). 응답은 반드시 **완전한 HTTP 텍스트 문자열**이어야 한다.
- 활성(busy) 처리량은 단일 루프 스레드에 묶인다. "활성 처리량을 코어 수만큼 확장"하는
  것은 향후 과제(핸들러 풀 오프로드 또는 VM 중단/재개)다.

블로킹 모델(연결마다 `spawn`)과 비교: 블로킹은 CPU 병렬엔 강하나 연결당 스레드라
유휴 연결이 많으면 스레드가 고갈된다. `serve_event`는 그 반대 트레이드오프다.

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
| Phase 3 | 트리 워킹 인터프리터 (순차 실행, spawn은 즉시 평가) — Phase 44(v0.30.0)에서 제거, VM이 유일한 엔진 |
| Phase 4 | 바이트코드 VM + 가상 스레드 기반 진짜 동시성 |
| Phase 5 | 채널, parallel_map, 구조적 동시성 완전 구현 |

---

## 7. 문자열 이스케이프 시퀀스

| 시퀀스 | 의미 |
|--------|------|
| `\n` | 줄바꿈 |
| `\t` | 탭 |
| `\r` | 캐리지 리턴 |
| `\0` | 널 문자 |
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
- 실험적 AOT 컴파일(`bang compile`)은 지원하지 않는다(컴파일 에러).

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

타입 이름: `int` `float` `bool` `str` `bytes` `nil` `list` `map` `fn` `any`
(타입 이름은 키워드가 아니라 타입 위치에서만 의미를 갖는 식별자다.)

규칙:
- 힌트가 있으면 값이 바인딩/전달/반환되는 시점에 타입을 검사하고, 불일치 시
  런타임 에러를 낸다. 이 에러는 `try`/`catch`로 잡을 수 있다.
- `any`는 모든 타입을 허용한다(검사 통과).
- 힌트가 없는 변수·파라미터·반환은 검사하지 않는다(완전 동적).
- 검사 대상: 타입이 표기된 `let`의 값, 표기된 파라미터(함수 진입 시),
  표기된 반환 타입(명시적 `return`의 값). 값이 Future면 해소 후 검사한다.
- 실험적 AOT 컴파일(`bang compile`)은 타입 힌트를 지원하지 않으며, 힌트가 있으면 컴파일 에러를 낸다
  (무시하면 VM과 결과가 달라지므로).
- 정적 검사: `bang check`는 실행 전에 타입 힌트를 정적 분석한다. 동적 언어이므로
  **둘 다 구체 타입이고 서로 다른, 확실한 충돌만** 보고하고(let·함수 인자·반환값),
  미지(any/추론 불가) 타입은 통과시켜 거짓 양성을 피한다.

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

---

## 10. 시스템 빌트인 (프로세스·환경·대기·파일)

스크립트 실무용 빌트인. 실패는 모두 `try`/`catch`로 잡히는 런타임 에러다.

| 함수 | 반환 | 실패 조건 |
|---|---|---|
| `proc_run(argv)` | `{"code": int\|nil, "stdout": str, "stderr": str}` | 빈 리스트, 문자열 아닌 원소, 명령을 찾을 수 없음 |
| `env_get(name)` | `str`, 없으면 `nil` | 값이 유니코드가 아님 |
| `env_cwd()` | `str` (현재 작업 디렉터리) | 읽을 수 없음 |
| `sleep(ms)` | `nil` | 음수 |
| `file_remove(path)` | `nil` | 없는 파일, 디렉터리 지정 |
| `file_move(src, dst)` | `nil` | 원본 없음, 다른 볼륨으로 이동 |
| `dir_create(path)` | `nil` (중간 경로 포함, 이미 있으면 통과) | 같은 이름의 파일이 있음 |
| `file_append(path, s)` | `nil` (없으면 생성) | 쓰기 실패 |

```bang
let r = proc_run(["git", "status", "--short"])
if r.code != 0 {
    print_err(r.stderr)
}
```

`proc_run` 규칙:
- 셸을 거치지 않는다. `argv[0]`을 직접 실행하고 나머지를 인자로 넘긴다. 따라서
  셸 문법(파이프, `*` 글롭, 환경 변수 치환)은 해석되지 않으며, 셸 주입이 불가능하다.
  Windows의 `echo`/`dir`처럼 셸 내장 명령은 실행 파일이 아니므로 직접 부를 수 없다.
- stdin은 연결하지 않는다(입력을 기다리는 명령이 멈추지 않음).
- 0이 아닌 종료 코드는 에러가 아니라 `code`로 돌려준다. Unix에서 시그널로
  종료되면 `code`는 `nil`이다.

`sleep`은 현재 작업만 멈춘다. `spawn`된 작업에서 불러도 스케줄러의 탄력 풀이
다른 작업을 계속 처리한다.

**한계 (반드시 인지)**:
- `proc_run`에는 타임아웃이 없다. 끝나지 않는 명령은 호출한 작업을 영구히 멈춘다.
- `proc_run`의 stdout/stderr에 UTF-8이 아닌 바이트가 있으면 `�`(U+FFFD)로 대체된다
  (바이트 값 타입 도입 전까지의 한계).
- `serve_event` 핸들러 안의 `sleep`/`proc_run`은 단일 스레드 이벤트 루프 전체를 멈춘다.
- `file_move`는 다른 드라이브·볼륨으로 이동하지 못한다(복사 후 삭제로 대체).
- 디렉터리 삭제 빌트인은 없다.

---

## 11. 바이트 값 타입 (bytes)

`bytes`는 **불변** 바이트 열이다. 문자열(`str`)은 항상 유효한 UTF-8이어야 하므로,
이미지·압축 파일·바이너리 프로토콜처럼 UTF-8이 아닌 데이터는 `bytes`로 다룬다.

- **값 의미론**: list/str과 같다. 바인딩·인자·채널·spawn에서 복제된 것처럼 동작한다
  (불변이므로 내부적으로는 공유해도 관찰상 차이가 없다). 채널로 보낼 수 있다.
- **리터럴 문법 없음** (헌법 6조 "작은 표면"). 변환 빌트인으로 만든다.
- 타입 힌트 이름은 `bytes`, `type(b)`는 `"Bytes"`.

| 연산 | 결과 |
|---|---|
| `len(b)` | 바이트 수 |
| `b[i]` | `0..255` 정수. 음수 인덱스는 뒤에서부터. 범위 밖이면 에러 |
| `slice(b, s, e)` | 새 `bytes` (범위는 list/str의 slice와 같이 잘라 맞춤) |
| `a + b` | 연결한 새 `bytes` |
| `a == b` | 내용 비교 |
| `b[i] = v` | **에러** — 불변. `slice`와 `+`로 새 값을 만든다 |
| `for x in b` | 지원하지 않음 — `for i in range(len(b))` 사용 |

| 빌트인 | 설명 |
|---|---|
| `bytes_from_str(s)` | 문자열을 UTF-8로 인코딩 |
| `bytes_to_str(b)` | UTF-8로 디코딩. 유효하지 않으면 **에러** (조용히 `�`로 바꾸지 않음) |
| `bytes_from_hex(s)` | `"48656c6c6f"` → bytes. 대소문자 무관. 홀수 자릿수·비16진 문자는 에러 |
| `bytes_to_hex(b)` | 소문자 16진수 문자열 |
| `file_read_bytes(path)` | 파일 전체를 무손실로 읽기 |
| `tcp_read_bytes(conn)` | 최대 4096바이트 1회 읽기. EOF면 빈 `bytes` |

쓰기 빌트인은 `str`과 `bytes`를 모두 받는다: `write_file(p, data)`,
`file_append(p, data)`, `tcp_write(conn, data)`.

표시(`print`)는 `<bytes 길이: 16진수>` 형식이며 앞 32바이트까지만 보인다.
`json_stringify`는 `bytes`를 직렬화하지 않는다(에러) — 필요하면 `bytes_to_hex`로 변환한다.

```bang
let png = file_read_bytes("logo.png")
print(bytes_to_hex(slice(png, 0, 8)))   // 89504e470d0a1a0a (PNG 시그니처)
write_file("copy.png", png)             // 무손실 복사
```

**한계**: `proc_run`의 stdout/stderr와 `http_get`의 body는 아직 `str`이다(비-UTF-8은 `�`로 대체).

---

## 12. HTTP 클라이언트 (http_get)

```bang
let r = http_get("https://example.com/")
print(r.status)                 // 200
print(r.headers["content-type"]) // 헤더 키는 소문자
print(len(r.body))
```

- `http_get(url)` → `{"status": int, "headers": map, "body": str}`. `http://`와 `https://`를 지원한다.
- 리다이렉트는 따라가지 않는다 — `status`(301/302 등)와 `headers.location`을 그대로 돌려준다.
- 4xx/5xx는 에러가 아니라 `status`로 돌려준다. 연결 실패·TLS 실패·타임아웃(10초)은 catch 가능한 에러다.
- `Transfer-Encoding: chunked`와 `Content-Length`를 처리한다.

**https 구현**: TLS는 rustls(+ring)로 처리한다. 헌법 5조 "자급 코어"의 유일한 의존성 예외이며,
`https` cargo feature(기본 켜짐)로 격리되어 있다. `cargo build --no-default-features`로 빌드하면
TLS 의존성이 빠지고 `https://` 요청은 "https를 지원하지 않음" 에러가 된다. 모든 OS에서 TLS 동작이
같다(OS 네이티브 TLS를 쓰지 않음).

인증서는 항상 검증한다(만료·자체 서명·호스트 불일치는 "TLS 핸드셰이크 실패" 에러). 검증을 끄는
옵션은 없다.

**한계 (반드시 인지)**:
- 루트 인증서는 바이너리에 내장된 Mozilla 목록(webpki-roots)만 쓴다. OS 인증서 저장소를 읽지 않으므로,
  **사내 프록시처럼 자체 인증 기관(CA)을 쓰는 환경에서는 https 접속이 실패한다.**
  내장 목록은 bang을 새로 빌드해야 갱신된다.
- GET만 지원한다(POST 등 없음). 요청 헤더를 지정할 수 없다.
- `body`는 `str`이다 — UTF-8이 아닌 응답은 `�`로 대체된다(11장 한계 참고).
- 클라이언트 인증서(mTLS), 프록시, HTTP/2는 지원하지 않는다.
