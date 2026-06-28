# bang

동적 타입 범용 프로그래밍 언어. 렉서·파서부터 트리워킹 인터프리터, 바이트코드 VM,
M:N 스케줄러, JIT/AOT 백엔드까지 Rust로 처음부터 구현했다.

핵심 특징:

- **값 의미론** — list/map/string은 복제되어 전달된다. 공유 가변 상태가 없다.
- **투명 동시성** — 함수 색칠(async/await) 없이 `spawn`/`parallel`만으로 병렬 실행.
  Future는 값이 필요한 시점에 자동 조인된다(`await` 키워드 없음).
- **사람이 읽기 좋은 에러** — 모든 오류에 `(line, col)`과 소스 컨텍스트가 붙는다.

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

> Windows에서 인터프리터/VM 실행은 그대로 동작한다. 단, AOT 컴파일(`bang compile`)은
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
bang                      # 인자 없이 실행하면 REPL 진입
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
bang run     [--interp] [--jit] [--dump-ast] <파일|->   실행 (기본: VM)
                          --jit 은 소스에서 --features jit 로 빌드한 경우에만 동작
                          (Homebrew/curl 배포본은 미포함)
bang compile -o <출력> <파일>   AOT 컴파일 (C 트랜스파일 + cc/clang/gcc -O2)
bang check   <파일>       오류 검사 (실행 없음)
bang build   <파일>       컴파일 검증 + 통계
bang parse   <파일>       AST 출력
bang tokenize <파일>      토큰 출력 (디버그)
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
| 수학 | `abs(x)` `sqrt(x)` `floor(x)` `ceil(x)` `round(x)` `pow(b,e)` `min(...)` `max(...)` |
| 동시성 | `channel(...)` `send(c,v)` `recv(c)` `close(c)` `wait(f)` `parallel_map(l,f)` |
| JSON | `json_parse(s)` `json_stringify(v)` |
| 시간/난수 | `now_ms()` `format_time(ms)` `random()` `random_int(lo,hi)` |
| 문자열/문자 | `ord(s)` `chr(n)` (그 외 위 문자열 행 참고) |
| 파일시스템 | `read_file(p)` `write_file(p,s)` `list_dir(p)` `file_exists(p)` `is_dir(p)` |
| I/O | `print(...)` `print_err(...)` `input(...)` `args()` |

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
print(sqrt(16.0))                     // 4
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
- import는 기본 실행 엔진인 VM에서 동작한다(`--interp` 모드는 미지원).

## 에러 처리 (try / catch / throw)

런타임 에러는 `try`/`catch`로 잡아 복구하고, `throw`로 임의의 값을 던진다.

```
// 사용자 throw
try {
    throw "문제 발생"
} catch e {
    print("잡음: " + e)
}

// 내장 런타임 에러(0 나눗셈 등)도 잡힌다 — 메시지가 문자열로 바인딩
try {
    let x = 1 / 0
} catch e {
    print(e)              // "0으로 나눌 수 없음"
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
- 내장 런타임 에러도 `catch`로 잡히며, catch 변수엔 에러 **메시지 문자열**이 바인딩된다.
- `catch` 안에서 다시 `throw`하면 더 바깥 `try`로 전파된다(re-throw).
- 잡히지 않은 예외는 프로그램을 종료시킨다.
- 기본 실행 엔진인 VM에서 동작한다(`--interp` / AOT `compile`은 미지원).

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
    print(e)             // "타입 불일치: int 기대, Str 받음"
}

// any 는 모두 허용, 힌트 없는 코드는 완전 동적
fn id(v: any) -> any { return v }
```

타입: `int` `float` `bool` `str` `nil` `list` `map` `fn` `any`.
(VM 전용 — `--interp`/AOT는 힌트를 무시한다.)

## 개발

```bash
cargo build                                  # 디버그 빌드
cargo build --release                        # 릴리스 빌드
cargo build --features jit                   # Cranelift JIT 포함
cargo test                                   # 전체 테스트
cargo clippy --all-targets -- -D warnings    # 린트
cargo run -- run examples/fibonacci.bang     # 개발 중 실행
```

언어 명세는 [`docs/SPEC.md`](docs/SPEC.md), 사용자 가이드는 [`docs/GUIDE.md`](docs/GUIDE.md).

## 라이선스

[MIT](LICENSE)
