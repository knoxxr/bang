# bang

동적 타입 범용 프로그래밍 언어. 렉서·파서부터 트리워킹 인터프리터, 바이트코드 VM,
M:N 스케줄러, JIT/AOT 백엔드까지 Rust로 처음부터 구현했다.

핵심 특징:

- **값 의미론** — list/map/string은 복제되어 전달된다. 공유 가변 상태가 없다.
- **투명 동시성** — 함수 색칠(async/await) 없이 `spawn`/`parallel`만으로 병렬 실행.
  Future는 값이 필요한 시점에 자동 조인된다(`await` 키워드 없음).
- **사람이 읽기 좋은 에러** — 모든 오류에 `(line, col)`과 소스 컨텍스트가 붙는다.

## 설치

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

> AOT 컴파일(`bang compile`)은 시스템에 C 컴파일러(`cc`: clang/gcc)가 필요하다.
> macOS에서는 Xcode Command Line Tools(`xcode-select --install`)로 설치된다.

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
bang compile -o <출력> <파일>   AOT 컴파일 (C 트랜스파일 + cc -O2)
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
