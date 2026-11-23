# bang

**순차적으로 생각하고, 병렬로 실행한다.** async/await 색칠도, GIL도, 락도 없다.
`spawn` 한 단어면 평범한 함수가 진짜 멀티코어 병렬이 되고, 값 의미론이 데이터
레이스를 문법 이전에 제거한다.

```bang
// URL 3개를 "동시에" 가져온다 — async도, 콜백도, 락도 없이.
let pages = parallel_map(urls, fn(url) { return http_get(url) })
print("가져온 페이지: " + str(len(pages)))
```

```bash
brew install knoxxr/tap/bang      # 또는 아래 설치 섹션
bang fanout.bang
```

같은 일을 Python으로 하려면 `aiohttp` + `async def` + `await` + `gather` +
`asyncio.run` — 그리고 그 함수를 부르는 모든 함수가 `async`로 물든다(함수 색칠).
bang에는 그게 없다. `spawn`/`parallel_map`만 있으면 된다.

> **정직한 한계**: bang의 순차 실행 속도는 CPython보다 3~4배 느리다. bang이 이기는
> 지점은 한 코어가 아니라 **머신 전체** — 색칠·락 없이 8코어를 다 쓰는 코드의
> 단순함이다. 순차 성능이 전부라면 bang은 당신의 언어가 아니다. → [벤치마크](bench/README.md)

## 왜 bang인가

- **투명 동시성** — 함수 색칠(async/await) 없이 `spawn`/`parallel`만으로 병렬 실행.
  Future는 값이 필요한 시점에 자동 조인된다(`await` 키워드 없음).
- **값 의미론** — list/map/string은 복제되어 전달된다. 작업 사이에 공유되는 가변 값이 없으니
  언어 안의 값에서는 데이터 레이스가 생길 수 없다. (파일·소켓 같은 외부 자원은 본래 공유된다.)
- **구조적 동시성** — 모든 작업은 스코프에 뿌리내리고 자동 조인된다. 작업 누수가 없다.
- **사람이 읽기 좋은 에러** — 모든 오류에 `(line, col)`과 소스 컨텍스트가 붙는다.

철학과 설계 원칙(헌법·영구 거부 목록)은 [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md),
활성화 계획은 [docs/VISION.md](docs/VISION.md) 참고.

## 설치

지원 플랫폼: **macOS**(Apple Silicon·Intel), **Linux**(x86_64·ARM64), **Windows**(x64).

### Homebrew (macOS, 권장)

프리빌트 바이너리로 즉시 설치된다.

```bash
brew install knoxxr/tap/bang
bang --version
```

### 설치 스크립트 (Linux / macOS, curl)

프리빌트 바이너리를 내려받아 설치한다. Linux(x86_64·ARM64)와 macOS를 지원한다.

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/knoxxr/bang/releases/latest/download/bang-installer.sh | sh
```

### Windows (PowerShell)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/knoxxr/bang/releases/latest/download/bang-installer.ps1 | iex"
```

> Windows에서 실행(`bang run`)은 그대로 동작한다. 단, 실험적 AOT 컴파일(`bang compile`)은
> C 컴파일러가 필요한데 Windows 기본 MSVC(`cl`)는 비호환이므로,
> MSYS2(MinGW `gcc`) 또는 LLVM(`clang`)을 설치해 PATH에 두어야 한다.

### 소스에서 빌드 (cargo)

Rust 툴체인(`cargo`)이 필요하다.

```bash
# 저장소를 클론하지 않고 바로 설치
cargo install --git https://github.com/knoxxr/bang

# 또는 클론한 저장소에서
cargo install --path .
```

`bang` 은 `~/.cargo/bin` 에 깔린다. 이 경로가 PATH에 있어야 한다
(rustup 설치 시 보통 자동 설정됨).

> AOT 컴파일(`bang compile`)은 시스템에 C 컴파일러가 필요하다. `cc → clang → gcc`
> 순으로 자동 탐색한다. macOS는 Xcode Command Line Tools(`xcode-select --install`),
> Linux는 `gcc`/`clang`, Windows는 MSYS2(MinGW) 또는 LLVM(clang)을 설치하면 된다.

## 사용법

Python처럼 쓸 수 있다.

```bash
bang script.bang          # 파일 실행 (run 생략 가능)
bang run script.bang      # 명시적 실행
bang                      # 인자 없이 실행하면 REPL 진입 (VM 기반 — try/catch·import·JSON 등 전 기능 동작)
echo 'print(1 + 2)' | bang -   # 표준 입력에서 실행
```

### 스크립트 직접 실행 (shebang)

첫 줄에 shebang을 넣고 실행 권한을 주면 `./script.bang` 으로 직접 실행된다.

```bash
#!/usr/bin/env bang
print("hello from bang")
```

```bash
chmod +x script.bang
./script.bang
```

### 전체 명령

```
bang <파일.bang>          .bang 파일 실행 (run 생략 가능)
bang                      REPL 진입
bang run     [--dump-ast] <파일|->   실행 (바이트코드 VM — 유일한 실행 엔진)
bang compile -o <출력> <파일>   [실험적] AOT 컴파일 (C 트랜스파일 + cc/clang/gcc -O2)
                          정수·문자열·함수만 지원. 미지원 기능은 컴파일 에러
bang check   <파일>       오류 검사 + 정적 타입 검사 (실행 없음)
bang build   <파일>       컴파일 검증 + 통계
bang parse   <파일>       AST 출력
bang tokenize <파일>      토큰 출력 (디버그)
bang test   [경로...]     테스트 실행 (*_test.bang / test_*.bang 의 test_* 함수, 실패 시 exit 1)
bang fmt     [--check] [경로...]   들여쓰기 정규화 (줄 안 내용은 바꾸지 않음)
bang repl                 대화형 셸
bang version              버전 출력
bang help                 도움말
```

## 예제

```
// fibonacci.bang
fn fib(n) {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

let i = 0
while i < 10 {
    print(fib(i))
    i = i + 1
}
```

위 코드를 `fibonacci.bang` 으로 저장한 뒤 실행한다(저장소 예제는 `examples/fibonacci.bang`):

```bash
bang fibonacci.bang
```

### 동시성 (헤드라인 기능)

`spawn` 으로 작업을 띄우면 Future가 즉시 반환되고, 값이 필요한 시점에 자동 조인된다.
`await` 키워드도, 함수 색칠도 없다.

```
// 두 작업을 병렬 실행 — a + b 계산 시점에 자동으로 조인된다
let a = spawn fib(30)
let b = spawn fib(31)
print(a + b)
```

더 많은 샘플은 [`examples/`](examples/) — 각 파일 상단 주석의 기대 출력이 통합 테스트 정답지다.
동시성 예제: `channels.bang`, `parallel_block.bang`, `spawn_basic.bang`.

## 표준 라이브러리

별도 import 없이 바로 쓰는 내장 함수다. **컨테이너는 값 의미론**이라
`push`/`sort` 같은 함수는 원본을 바꾸지 않고 **새 값을 반환**한다 — 결과를 다시 받아야 한다.

```
let xs = [3, 1, 2]
let ys = push(xs, 9)   // xs 는 그대로, ys = [3, 1, 2, 9]
let sorted = sort(xs)  // sorted = [1, 2, 3], xs 는 그대로
```

| 분류 | 함수 |
|---|---|
| 타입/변환 | `str(x)` `int(x)` `float(x)` `bool(x)` `type(x)` `len(x)` |
| 리스트 | `push(l,x)` `pop(l)` `sort(l)` `sort_by(l,keyfn)` `reverse(l)` `map(l,f)` `filter(l,f)` `reduce(l,f,init)` `any(l,f)` `all(l,f)` `sum(l)` `flat(l)` `enumerate(l)` `zip(a,b)` `range(...)` `slice(l,s,e)` `index_of(l,x)` `unique(l)` |
| 맵 | `keys(m)` `values(m)` `has(m,k)` `get(m,k,default)` `merge(m1,m2)` |
| 문자열 | `split(s,sep)` `join(l,sep)` `trim(s)` `trim_start(s)` `trim_end(s)` `replace(s,a,b)` `contains(s,sub)` `starts_with(s,p)` `ends_with(s,p)` `upper(s)` `lower(s)` `find(s,sub)` `chars(s)` `slice(s,start,end)` `repeat(s,n)` |
| 수학 | `idiv(a,b)`(정수 몫 — `/`는 항상 float) `abs(x)` `sqrt(x)` `floor(x)` `ceil(x)` `round(x)` `pow(b,e)` `min(...)` `max(...)` `gcd(a,b)` `clamp(x,lo,hi)` `sign(x)` `sin(x)` `cos(x)` `tan(x)` `log(x)` `log10(x)` `exp(x)` `pi()` `e()` |
| 집합(리스트) | `union(a,b)` `intersect(a,b)` `difference(a,b)` (중복 제거) |
| 동시성 | `channel(...)` `send(c,v)` `recv(c)` `select(chans)` `close(c)` `wait(f)` `parallel_map(l,f)` |
| 네트워킹(서버) | `tcp_listen(addr)` `tcp_accept(s)` `tcp_read(c)` `tcp_read_until(c,marker)` `tcp_write(c,s)` `tcp_send_file(c,path)` `tcp_set_timeout(c,ms)` `tcp_close(c)` |
| 네트워킹(클라이언트) | `tcp_connect(addr)` `http_get(url)` `http_post(url, body[, headers])` `http_request(method, url[, body[, headers]])` → `{status, headers, body}` (http/https, 리다이렉트는 따라가지 않음, map/list body는 JSON) |
| 경로 | `path_join(a, b, ...)` `path_basename(p)` `path_dirname(p)` `path_ext(p)` `path_abs(p)` |
| CSV | `csv_parse(text)` → `[[str]]` `csv_stringify(rows)` |
| 이벤트 서버 | `serve_event(addr, handler)` — 논블로킹 이벤트 루프(단일 스레드 멀티플렉싱). `handler: fn(req)->resp_str` ⚠️ 단일 스레드라 CPU 무거운/블로킹 핸들러는 루프를 막음(상세·한계는 docs/SPEC.md 6.4-3) |
| JSON | `json_parse(s)` `json_stringify(v)` |
| 정규식 | `regex_match(s,pat)` `regex_find(s,pat)` `regex_find_all(s,pat)` `regex_replace(s,pat,repl)` `regex_groups(s,pat)` |
| 시간/난수 | `now_ms()` `format_time(ms)` `random()` `random_int(lo,hi)` |
| 문자열/문자 | `ord(s)` `chr(n)` `format(fmt, ...)` (그 외 위 문자열 행 참고) — 보간은 `f"{x:.2f}"` |
| 파일시스템 | `read_file(p)` `write_file(p,s)` `file_append(p,s)` `file_size(p)` `list_dir(p)` `file_exists(p)` `is_dir(p)` `file_remove(p)` `file_move(src,dst)` `dir_create(p)` |
| 바이트(불변) | `bytes_from_str(s)` `bytes_to_str(b)` `bytes_from_hex(s)` `bytes_to_hex(b)` `file_read_bytes(p)` `tcp_read_bytes(c)` + `len` `b[i]` `slice` `+` `==` (쓰기 빌트인은 str/bytes 모두 받음) |
| 프로세스/환경 | `proc_run([prog, args...])` → `{code, stdout, stderr}` (셸 미경유) `env_get(name)` `env_cwd()` `sleep(ms)` |
| I/O | `print(...)` `print_err(...)` `input(...)` `args()` |
| 테스트 | `assert(cond[,msg])` `assert_eq(actual, expected)` — `bang test`와 함께 |

```
// 고차 함수 — 함수를 값으로 넘긴다
let nums = [1, 2, 3, 4, 5]
let evens = filter(nums, fn(x) { return x % 2 == 0 })
let doubled = map(evens, fn(x) { return x * 2 })
print(reduce(doubled, fn(a, b) { return a + b }, 0))   // 12

// min/max 는 리스트 또는 2개 인자
print(max([3, 7, 1]))   // 7
print(max(3, 7))        // 7

// 문자열·수학
print(upper(trim("  hi  ")))          // HI
print(join(["a", "b", "c"], "-"))     // a-b-c
print(sqrt(16.0))                     // 4.0
```

전체 함수의 정확한 시그니처·의미는 [`docs/SPEC.md`](docs/SPEC.md) 참고.

## 모듈 (import)

`import("경로.bang")` 는 다른 `.bang` 파일을 실행하고, 그 파일의 **최상위 바인딩
(let·fn)을 맵으로 반환**한다. `.이름` 또는 `["이름"]` 으로 꺼내 쓴다.

```
// math.bang
let pi = 3.14159
fn square(x) { return x * x }
fn area(r) { return pi * square(r) }   // 형제 함수·모듈 상수 참조 OK
```

```
// main.bang
let math = import("math.bang")
print(math.pi)           // 3.14159
print(math.square(5))    // 25
print(math.area(2))      // 12.56636
```

- 모듈 함수는 **자기 모듈의 전역**(형제 함수·상수)을 그대로 참조한다.
- import 경로는 실행 디렉토리 기준 상대경로다.
- 모듈은 **한 번만 실행**되고 캐시된다(싱글톤). 같은 파일을 여러 번 import해도
  최상위 코드는 한 번만 실행되며, 모든 import가 같은 모듈 인스턴스를 공유한다.
- 실험적 AOT 컴파일(`bang compile`)에서는 import를 지원하지 않는다(컴파일 에러).

### 패키지 (git 의존성)

중앙 레지스트리 없이 **git 기반**으로 패키지를 관리한다.

```bash
# 의존성 추가 (git clone → bang_modules/, bang.toml 기록)
bang add mathutils https://github.com/user/mathutils@v1.0.0

# bang.toml의 모든 의존성 설치 (협업자가 클론 후)
bang install
```

`bang.toml`:
```toml
[dependencies]
mathutils = "https://github.com/user/mathutils@v1.0.0"
```

설치된 패키지는 **바레 이름**으로 import한다 — 검색 순서:
`./<name>.bang` → `bang_modules/<name>/<name>.bang` (또는 `main.bang`/`lib.bang`) →
`BANG_PATH` 의 디렉토리.

```
let math = import("mathutils")   // bang_modules/mathutils/ 에서 해석
print(math.square(5))
```
(경로/`.bang` 확장자가 있으면 기존처럼 파일 경로로 직접 import.)

## 에러 처리 (try / catch / throw)

런타임 에러는 `try`/`catch`로 잡아 복구하고, `throw`로 임의의 값을 던진다.

```
// 사용자 throw
try {
    throw "문제 발생"
} catch e {
    print("잡음: " + e)
}

// 내장 런타임 에러(0 나눗셈 등)도 잡힌다 — {kind, message, line, col} 맵
try {
    let x = 1 / 0
} catch e {
    print(e.message)      // "0으로 나눌 수 없음" (e.kind == "zero_division")
}

// 중첩 함수에서 던진 예외가 호출자의 try로 전파
fn checked(n) {
    if n < 0 {
        throw "음수 불가"
    }
    return n
}
try {
    print(checked(-1))
} catch e {
    print(e)             // "음수 불가"
}

// 임의의 값(맵)을 던져 구조화된 에러 전달
try {
    throw {"code": 404, "msg": "not found"}
} catch e {
    print(e.code)
}
```

- `throw <식>`은 문자열·맵·숫자 등 **임의의 값**을 던지며, 가장 가까운 바깥
  `try`의 `catch`로 (호출 스택을 가로질러) 전파된다.
- 내장 런타임 에러도 `catch`로 잡히며, catch 변수엔 맵 **`{kind, message, line, col}`**이
  바인딩된다 (`e.kind`로 종류 구별 — `zero_division`/`index`/`key`/`type`/`io` 등, SPEC 8.2).
- 맵에 없는 키를 읽으면 `key` 에러다 — 없을 수 있는 키는 `get(m, k, 기본값)`/`has(m, k)`.
- `catch` 안에서 다시 `throw`하면 더 바깥 `try`로 전파된다(re-throw).
- 잡히지 않은 예외는 프로그램을 종료시킨다.
- 실험적 AOT 컴파일(`bang compile`)에서는 지원하지 않는다(컴파일 에러).

## 선택적 타입 힌트

변수·파라미터·반환값에 타입을 선택적으로 표기할 수 있다. 표기는 **점진적**이라
생략 가능하며, 표기한 경곗값은 **런타임에 검증**된다(불일치 시 `try/catch`로 잡히는 에러).

```
let count: int = 42

fn area(w: int, h: int) -> int {
    return w * h
}
print(area(3, 4))        // 12

// 불일치는 런타임 에러
try {
    let n: int = "oops"
} catch e {
    print(e.message)     // "타입 불일치: int 기대, Str 받음"
}

// any 는 모두 허용, 힌트 없는 코드는 완전 동적
fn id(v: any) -> any { return v }
```

타입: `int` `float` `bool` `str` `nil` `list` `map` `fn` `any`.
(실험적 AOT 컴파일은 타입 힌트가 있으면 컴파일 에러를 낸다 — 조용히 무시하지 않는다.)

**정적 검사**: `bang check <파일>` 은 실행 전에 타입 힌트를 정적 분석해
**확실한 충돌만** 보고한다(거짓 양성을 피하기 위해 동적/미지 타입은 통과).

```bash
$ bang check app.bang
# 타입 오류: '...'은 int 인데 str 값이 대입됨   (let / 함수 인자 / 반환값 검사)
```

## 개발

```bash
cargo build                                  # 디버그 빌드
cargo build --release                        # 릴리스 빌드
cargo build --no-default-features           # https(rustls) 없이 자급 코어만
cargo test                                   # 전체 테스트
cargo clippy --all-targets -- -D warnings    # 린트
cargo run -- run examples/fibonacci.bang     # 개발 중 실행
```

## 문서

- **[왜 bang인가](docs/WHY.md)** — Python 동시성(GIL·asyncio·multiprocessing)에 지친 사람에게
- **[튜토리얼](docs/TUTORIAL.md)** — 따라 하며 배우는 입문 가이드 (먼저 읽으세요)
- [언어 명세](docs/SPEC.md) — 문법·의미 레퍼런스
- [헌법](docs/PHILOSOPHY.md) — 정체성·9조·영구 거부 목록·격언
- [비전](docs/VISION.md) — 활성화 실행 계획(철학+GTM 종합)
- [사용 가이드](docs/GUIDE.md) — 상세 가이드

## 구현

렉서·파서(직접 손으로 구현)부터 바이트코드 VM, M:N 스케줄러, 실험적 AOT C 트랜스파일러까지
Rust로 처음부터 구현했다. 실행 엔진은 VM 하나다(초기의 트리워킹 인터프리터와 Cranelift JIT는
v0.30.0에서 제거). 외부 파서 제너레이터를 쓰지 않고, JSON·정규식(자체 엔진+캡처)·시간·난수·HTTP까지
stdlib를 자체 구현한다 — 런타임 의존성은 논블로킹 I/O 리액터(`polling`)와, `https` 기능용
TLS(`rustls`+`webpki-roots`, `--no-default-features`로 제외 가능)뿐이다.

## 라이선스

[MIT](LICENSE)

---

## English summary

**bang** is a small, dynamically typed scripting language where you think sequentially and run in parallel.
There is no async/await (no function coloring), no GIL, and no locks: `spawn` makes an ordinary function call
run on another core, and values (lists, maps, strings) are copied across task boundaries, so tasks cannot
race on shared language values.

```bang
// Fetch URLs concurrently — no async, no callbacks, no locks.
let pages = parallel_map(urls, fn(url) { return http_get(url) })
print(f"fetched {len(pages)} pages")
```

- **Install**: `brew install knoxxr/tap/bang`, or the shell / PowerShell installers above.
- **Structured concurrency**: every task is joined before the function that spawned it returns; unhandled task
  errors propagate to the caller.
- **Honest limits**: sequential execution is about 3–4× slower than CPython ([benchmarks](bench/README.md));
  memory is reference-counted with a generational cycle collector (see SPEC §15); the experimental native compiler
  (`bang compile`) supports only a small subset.
- **Docs** are written in Korean first: [TUTORIAL](docs/TUTORIAL.md), [SPEC](docs/SPEC.md),
  [PHILOSOPHY](docs/PHILOSOPHY.md). [`llms.txt`](llms.txt) gives an LLM enough context to write bang.

