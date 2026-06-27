bang — 실제로 쓸 수 있는 동적 타입 범용 프로그래밍 언어의 컴파일러/런타임을
Rust로 처음부터 구현한다. 소스 확장자는 .bang, CLI/바이너리 이름은 bang
(bang run <file>, bang repl, bang build).

변경 금지 결정 (합의된 전제)


구현 언어: Rust (edition 2021).
렉서·파서는 직접 손으로 구현한다. 외부 파서 제너레이터(lalrpop, pest 등) 미사용.
백엔드 전략(단계적): (1) 트리 워킹 인터프리터 → (2) 바이트코드 VM(메인 실행 경로)
→ (3) 선택적 네이티브(Cranelift 우선). 현재는 (1)→(2)에 집중.
에러는 항상 (line, col) span을 담아 사람이 읽기 좋게 출력한다.
한 번에 한 Phase만 진행한다. 사용자가 다음 Phase를 지시하기 전에는 범위를 넘지 않는다.


불변 규칙 (언어 의미 — 위반 금지)

값 의미론


컨테이너(list, map, string)는 값 의미론: 바인딩·인자 전달·클로저 캡처·
채널 송신·spawn 캡처에서 모두 복제된다.
함수·채널·Future는 참조 의미론(Arc 공유). 채널이 유일하게 허용된 명시적 공유 통로다.
이 규칙이 "공유 가변 상태 없음"을 보장한다. 성능을 이유로 컨테이너에 공유 가변 참조를
도입하지 말 것 — 동시성 안전성의 근거가 무너진다.


투명 동시성 (Transparent Concurrency)


async/await 함수 색칠(coloring)을 도입하지 않는다. 모든 함수는 평범하게 작성한다.
spawn <식>은 새 작업으로 식을 실행하고 Future를 즉시 반환한다.
자동 대기(얕게): Future가 값으로 필요한 시점(단항/이항 피연산자, 호출 인자, return,
인덱스/필드 대상, 조건, for-in 순회 대상)에 자동 조인한다. await 키워드는 없다.
print만 표시용으로 깊게 해소한다.
구조적 동시성: 모든 spawn은 가장 가까운 함수 본문 또는 parallel { } 블록에
뿌리내리고, 스코프 종료 시 자동 조인된다. 작업 누수는 불가능해야 한다.
키워드는 spawn, parallel. channel/parallel_map/wait/send/recv/close는
키워드가 아니라 내장 함수·메서드다.
구현 백킹: Phase 3은 작업당 OS 스레드(std::thread), Phase 5는 M:N 스케줄러 +
논블로킹 I/O로 교체하되 의미는 동일하게 유지한다. Value는 Clone + Send여야 한다.


구문 규칙


문장 종결은 줄바꿈(세미콜론 없음). 렉서는 Newline 토큰을 내보내되,
여는 괄호 ( [ { 안과 이항 연산자/콤마 직후의 줄바꿈은 억제한다.
spawn은 단항 수준 전위 연산자(spawn fib(n) = spawn(fib(n))). 산술식 전체를
spawn하려면 spawn (a + b)처럼 괄호.
{ }는 위치로 구분: 표현식 자리=맵 리터럴, 문장/제어구문 본문=블록.
이름 있는 fn은 let <name> = fn(...){...}로 디슈가하며, 이름이 본문 안에서 보여
재귀를 허용한다.


모듈 경계

src/
  lexer/        토큰화 (Newline 토큰, span)
  ast/          AST 노드 (kind + span 분리: struct X { kind, span })
  parser/       재귀 하강 + Pratt
  resolver/     스코프/바인딩 정적 해석 (Phase 4)
  interpreter/  트리 워킹 평가 + 동시성 의미 (Phase 3)
  compiler/     AST → 바이트코드 (Phase 5)
  vm/           스택 VM + GC (Phase 5)
  runtime/      스케줄러·채널·Future (Phase 3 OS스레드 → Phase 5 M:N)
  stdlib/       내장 함수·모듈 (Phase 6)
  scheduler/    M:N 스케줄러 (Phase 9)
  codegen/      Cranelift JIT(jit) + AOT C 트랜스파일러(transpile) (Phase 9~10)
docs/SPEC.md    언어 명세 (동시성 모델 섹션 포함). 의미 변경 시 함께 갱신.
docs/GUIDE.md   사용자 가이드.
examples/       기본 샘플 + 동시성 샘플(channels/parallel_block/spawn_basic 등)이 한 디렉토리에 평면 배치.
                상단 주석의 기대 출력이 통합 테스트 정답지.

빌드 · 테스트 · 실행

bashcargo build                              # 디버그 빌드
cargo build --release                    # 릴리스 빌드
cargo test                               # 전체 테스트 (단위 + 통합)
cargo clippy --all-targets -- -D warnings  # 린트 (경고 0 유지)
cargo fmt                                # 포맷
cargo run -- run examples/fibonacci.bang            # 개발 중 실행
cargo run -- run --dump-ast examples/fibonacci.bang  # AST 덤프 확인

코딩 규약


cargo fmt 적용, cargo clippy 경고 0 유지.
라이브러리/런타임 경로에서 unwrap()/expect()/panic! 금지 — 에러는 Result로
전파한다(테스트 코드는 예외).
AST 노드는 kind와 span을 분리한다.
Value는 Clone + Send를 만족해야 한다(스레드 이동 가능). 데이터는 깊은 복사,
함수/채널/Future는 Arc 클론.
모든 주요 모듈에 단위 테스트를 둔다. examples/의 기대 출력 주석은 통합 테스트로 검증하며
항상 최신으로 유지한다.


개발 규율


한 Phase는 컴파일·테스트가 모두 green인 상태로 끝낸다. 다음 Phase로 넘어가기 전에
점검 항목을 제시한다.
언어 의미·문법·키워드가 바뀌면 docs/SPEC.md를 같은 커밋에서 갱신한다.
동시성 안전성과 충돌하는 "최적화"는 도입하지 않는다(위 불변 규칙 우선).
새 의존성 추가는 최소화하고, 추가 시 이유를 PR/요약에 남긴다.
작업 진행 방식: (a) 계획 요약 → (b) 코드 → (c) 테스트 작성·실행 → (d) 점검 항목.


현재 상태 (Phase 진행 체크리스트 — 완료 시 갱신할 것)


✅ Phase 0 — 언어 설계 + 스캐폴딩
✅ Phase 1 — 렉서
✅ Phase 2 — 파서 + AST
✅ Phase 3 — 인터프리터 + 동시성 (첫 핵심 마일스톤)
✅ Phase 4 — 의미 분석 / (선택) 타입 검사
✅ Phase 5 Part A — 바이트코드 VM (단일 스레드, 12예제 통과, cargo test green)
✅ Phase 5 Part B — OS 스레드 spawn/parallel 병렬화 (64 tests green, clippy 0, 두 번째 핵심 마일스톤)
✅ Phase 6 — 표준 라이브러리(38개 신규 내장 함수) + import 모듈 시스템
✅ Phase 7 — REPL(지속 상태) + bang check/build 명령어 + 소스 컨텍스트 에러 출력
✅ Phase 8 — 상수 폴딩(컴파일 타임 산술/논리 평가) (64 tests green, clippy 0)
✅ Phase 9 — M:N 스케줄러(src/scheduler.rs) + Cranelift JIT 백엔드(src/codegen/, --features jit)
            Part A: 고정 크기 스레드 풀로 OP_SPAWN 교체 (성능 개선, 의미 불변)
            Part B: Int-only 함수 Cranelift JIT (--jit 플래그, VM 자동 폴백 내장)
            (tests: 83 unit + 36 interp + 28 vm = 147 green, clippy 0)
✅ Phase 10 — AOT C 트랜스파일러 (src/codegen/transpile.rs)
             bang compile -o <출력> <파일.bang> → AST → C11 → cc -O2 → 네이티브 바이너리
             지원: Int/Float/Bool/Nil/Str, 산술·비교·논리, let/assign, if/while/return, 최상위 fn(재귀 포함)
             미지원: List/Map/Index/Field/Spawn/Parallel/for-in/클로저 (에러 반환)
             통합 테스트: tests/transpile_test.rs가 생성 C를 실제 cc로 컴파일·실행해 stdout 검증
             (tests: 87 unit + 26 interp + 3 lexer + 9 parser + 36 resolver + 28 vm + 8 transpile = 197 green, clippy 0)
✅ Phase 11 — 설치/배포 UX (Python 같은 CLI)
             `cargo install --path .` → ~/.cargo/bin/bang. README.md/LICENSE(MIT)/Cargo 메타데이터 추가.
             베어 파일 실행(bang script.bang), shebang(#!) 무시, 인자 없이 REPL 진입,
             stdin 실행(bang -), --version/-V. 버전 단일화(env!(CARGO_PKG_VERSION), Cargo 0.10.0).
             통합 테스트: tests/cli_test.rs (빌드 바이너리 직접 실행 7개)
             (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver + 28 vm + 8 transpile + 7 cli = 207 green, clippy 0)
             (미진: Phase D — Homebrew/프리빌트 바이너리 배포는 원격 저장소 운영 결정 후 진행)