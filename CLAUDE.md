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
             Phase D 배포 자동화: cargo-dist(dist-workspace.toml + .github/workflows/release.yml)
               - 타깃: macOS aarch64/x86_64 + Linux x86_64/aarch64 + Windows x86_64-msvc
                 인스톨러: shell(curl, Linux/macOS) + homebrew(macOS) + powershell(Windows)
                 (Windows AOT compile은 cc/clang/gcc 후보 탐색; MSVC cl 비호환 → MinGW/clang 필요)
               - tap: knoxxr/homebrew-tap (HOMEBREW_TAP_TOKEN 시크릿 필요)
               - 릴리스 트리거: git tag vX.Y.Z && git push --tags → CI가 빌드·릴리스·formula 발행
               - 남은 수동 단계: GitHub에 knoxxr/homebrew-tap 생성 + 시크릿 등록 + 태그 푸시
✅ Phase 12 — import 크로스모듈 전역 버그 수정 (VM)
             이전: import한 모듈 함수가 형제 함수/모듈 상수를 참조하면 VM 패닉
             (모듈 함수가 전역을 절대 슬롯으로 참조하는데 메인 VM 전역 배열을 가리킴).
             수정: VmClosure가 자기 모듈 전역(Arc<Mutex<Vec>>)을 보유 →
             OP_LOAD/STORE_GLOBAL은 현재 프레임 클로저 전역 사용. spawn은 deep_clone_closure가
             전역까지 깊은 복사해 격리 유지. import는 sub_vm 전역 Arc가 함수에 실려 유지됨.
             통합 테스트: tests/import_test.rs (4개). 참고: import는 VM 전용(--interp 미지원).
             (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver + 28 vm + 8 transpile + 7 cli + 4 import = 211 green, clippy 0)
✅ Phase 13 — 에러 처리: try/catch/throw (VM) — 범용 언어로의 1순위 기능
             문법: try { } catch e { } + throw <식> (finally는 v1 제외)
             키워드 추가: try, catch, throw. 예외는 임의의 값(throw)이며 내장 런타임
             에러도 catch로 잡힘(메시지 문자열로 바인딩). 호출 스택 가로질러 전파.
             VM 구현: exec_until을 exec_dispatch 래퍼로 감싸 Err를 가로채 핸들러 스택으로
             되감기(OP_SETUP_TRY/OP_POP_TRY/OP_THROW). 던진 값은 self.pending_exception에 보관.
             break/continue/return의 try 핸들러 정리, 미캐치 시 "잡히지 않은 예외" 종료.
             인터프리터/AOT는 "VM 전용" 명확한 에러 반환. 예제: examples/error_handling.bang.
             테스트: vm_test +8. (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 36 vm + 8 transpile + 7 cli + 4 import = 219 green, clippy 0)
✅ Phase 14 — 값 컨테이너 copy-on-write (VM 성능) — 의미 불변, 성능만 개선
             VmValue::List/Map을 Arc<Vec>/Arc<HashMap>로 변경. clone은 Arc 공유(O(1)),
             변경은 Arc::make_mut으로 공유 중일 때만 실제 복사(copy-on-write).
             관찰되는 값 의미론·동시성 안전은 깊은 복사와 동일(별칭 격리 유지).
             효과: 큰 리스트를 바인딩/인자/반환/spawn에 넘길 때 O(n)→O(1).
             (벤치: 1만 원소 리스트 1만회 전달 0.02s). 인덱스/push 등 변경 지점만 make_mut.
             테스트: vm_test +5 COW(별칭격리/맵/인자/push/대용량). 
             (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 41 vm + 8 transpile + 7 cli + 4 import = 224 green, clippy 0)
✅ Phase 15 — 선택적 타입 힌트 (런타임 검증, gradual typing) — 범용 언어 3순위
             문법: let x: T = .., fn f(a: T) -> T { }. 타입: int/float/bool/str/nil/list/map/fn/any.
             점진적: 힌트 생략 가능, 표기 경곗값만 런타임 검증(불일치=try/catch로 잡히는 에러).
             렉서: -> (Arrow) 토큰. AST: TypeAnn enum + Let.ty/Function.param_types,ret_type.
             컴파일러: OP_CHECK_TYPE(peek) emit — 타입된 let/파라미터(진입)/명시적 return.
             VM: OP_CHECK_TYPE가 Future 해소 후 검사, any/미표기는 통과. VM 전용(interp/AOT 무시).
             예제: examples/type_hints.bang. 테스트: vm_test +8.
             (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 49 vm + 8 transpile + 7 cli + 4 import = 232 green, clippy 0)
✅ Phase 16 — stdlib 확장 + 패키지 시스템(import 캐싱) — 범용 언어 5순위
             신규 빌트인(58-63, VM): slice(seq,s,e), has(map,k), get(map,k,default),
             merge(m1,m2), repeat(str,n), index_of(list,x). resolver+vm BUILTINS 끝에 추가.
             패키지: import 모듈 캐싱 — 같은 파일은 1회만 실행(싱글톤), 정규화 경로 키.
             모듈 top-level print를 메인 출력으로. (module_cache OnceLock<Mutex<HashMap>>)
             테스트: vm_test +5(stdlib), import_test +1(캐싱). 
✅ Phase 17 — 킬러 데모 + 벤치마크 — 범용 언어 4순위
             examples/concurrency_demo.bang(결정적 출력), bench/(loop_seq/par, fib_seq/par, fib.py).
             결과: 거친 단위 CPU작업 8개 병렬 = 순차 24s→11s (~2.2x, GIL 없음).
             정직한 한계 보고: 세밀한 재귀(fib)는 호출당 프레임 힙할당(Arc<Mutex<Vec>>)
             경합으로 병렬 이득 없음 → 향후 VM 과제(호출 경로 프레임 할당 제거).
             (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 54 vm + 8 transpile + 7 cli + 5 import = 238 green, clippy 0)
✅ Phase 18 — VM 호출 경로 최적화 (성능) — 의미 불변
             Phase 17 벤치가 드러낸 병목 해결:
             1) 호출 프레임 locals 풀링 — 호출당 Arc<Mutex<Vec>> 힙 할당을 재사용으로 교체
                (strong_count==1만 회수). do_call에서 중간 args Vec도 제거. fib 순차 14s→8.3s.
             2) spawn 시 모듈 전역 완전 격리 — deep_clone_closure가 globals 복사 시 그 안의
                형제 함수들의 globals Arc를 새 복사본으로 재지정. 이전엔 함수가 원본 globals
                Arc를 공유해 다중 스레드가 같은 Mutex를 경합(system 116s). 
                fib 병렬 35s→1.9s (순차 대비 4.4x), loop 병렬 2.3x.
             값 의미론·spawn 격리는 오히려 더 정확해짐(작업별 독립 전역). 238 green, clippy 0.
✅ Phase 19 — stdlib 실무 핵심: JSON + 시간 + 난수 (VM 빌트인 64-68)
             json_parse(str)→value (손수 짠 재귀하강 파서: object→Map, array→List,
             number→Int/Float, null→Nil, \uXXXX 지원), json_stringify(value)→str
             (키 정렬로 안정 출력, 함수/채널은 직렬화 에러 → try/catch로 잡힘).
             now_ms()→epoch millis, random()→[0,1) float, random_int(lo,hi)→[lo,hi] 정수.
             난수는 xorshift64 PRNG(시간 시드, 의존성 없음). 예제: examples/json_demo.bang.
             테스트: vm_test +6. (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 60 vm + 8 transpile + 7 cli + 5 import = 244 green, clippy 0)
✅ Phase 20 — stdlib 폭 확장 (VM 빌트인 69-76)
             파일시스템: list_dir(path)→names, file_exists(path), is_dir(path) (std::fs 래퍼).
             list 유틸: sort_by(list,keyfn)→키 정렬(고차, vm_cmp), unique(list)→순서유지 dedup.
             시간: format_time(ms)→"YYYY-MM-DD HH:MM:SS" UTC (civil-date 변환, 의존성 없음).
             문자: ord(str)→코드포인트, chr(int)→문자. (정규식은 별도 엔진이라 향후 과제)
             테스트: vm_test +5. (tests: 90 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 65 vm + 8 transpile + 7 cli + 5 import = 249 green, clippy 0)
✅ Phase 21 — 정규식 엔진 (자체 구현, 외부 의존성 없음) (VM 빌트인 77-80)
             src/regex.rs: 패턴→AST→백트래킹 바이트코드 VM (스텝 상한으로 폭주 방어).
             지원: 리터럴 . * + ? {n}{n,}{n,m} [..][^..] 범위 \d\w\s\D\W\S ^ $ ( ) | 이스케이프.
             빌트인: regex_match/regex_find/regex_find_all/regex_replace. 패턴 오류는 try/catch로 잡힘.
             (캡처 그룹 추출/역참조는 미지원 — 향후 과제). 예제: examples/regex_demo.bang.
             테스트: regex.rs 단위 7 + vm_test 통합 6.
             (tests: 97 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 71 vm + 8 transpile + 7 cli + 5 import = 262 green, clippy 0)
✅ Phase 22 — 정규식 캡처 그룹 (VM 빌트인 81)
             regex 엔진에 캡처 추가: (...) 가 그룹 인덱스를 갖고, 백트래킹 VM이 Save(슬롯)로
             각 그룹 위치 기록(스레드별 saves, Split 시 복제). 그룹0=전체 매치.
             regex_groups(s, pat) → [전체, g1, g2, ...] (미참여 그룹은 nil), 매치 없으면 nil.
             (역참조는 여전히 미지원). 테스트: regex 단위 +1, vm_test +1.
             (tests: 98 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 72 vm + 8 transpile + 7 cli + 5 import = 264 green, clippy 0)
✅ Phase 23 — 패키지 시스템 (git 기반, 레지스트리 없음)
             A) 이름 기반 모듈 해석(vm.rs resolve_module): 바레 이름 import는
                ./<name>.bang → bang_modules/<name>/{name,main,lib}.bang → BANG_PATH 순서로 검색.
                .bang/경로 구분자 포함이면 기존처럼 직접 경로.
             B) 매니페스트(src/pkg.rs): bang.toml [dependencies] 최소 파서/직렬화.
                CLI: bang add <name> <git-url[@rev]> (git clone → bang_modules/ + bang.toml 기록),
                bang install (bang.toml 의존성 일괄 clone). git/네트워크는 thin 래퍼.
             테스트: pkg 단위 4, import 통합 +1(이름 해석). git clone은 수동 검증.
             (tests: 102 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 72 vm + 8 transpile + 7 cli + 6 import = 269 green, clippy 0)
✅ Phase 24 — math 확장 + 집합 연산 (VM 빌트인 82-95)
             math: gcd, clamp(원본 타입 유지), sign, sin/cos/tan, log/log10/exp, pi(), e().
             집합(리스트 기반, 새 타입 도입 안 함): union/intersect/difference (모두 중복 제거).
             (Set 새 값 타입은 코어 전체 영향이라 의도적으로 리스트 기반으로 대체.)
             헬퍼 num_of(Int/Float→f64) 추가. 테스트: vm_test +3.
             (tests: 102 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 75 vm + 8 transpile + 7 cli + 6 import = 272 green, clippy 0)
✅ Phase 25 — 정적 타입 검사 (gradual) — src/typeck.rs, bang check 통합
             타입 힌트를 실행 전 정적 분석. 동적 언어이므로 "둘 다 구체 타입이고 서로
             다른 확실한 충돌"만 보고(Unknown/any는 통과 → 거짓 양성 회피).
             검사: 타입된 let, 함수 호출 인자(최상위/지역 시그니처), 타입된 반환값.
             식 타입 추론(리터럴/연산/호출 반환/변수 스코프). bang check에 통합(오류 시 exit 1).
             참고: VM 미세 최적화(전역 Arc 복제 제거)는 측정상 무효과 → globals Mutex는
             병목 아님 확인. 진짜 순차 성능엔 unsafe/재작성 필요(보류).
             테스트: typeck 단위 5. (tests: 107 unit + 26 interp + 3 lexer + 9 parser
             + 36 resolver + 75 vm + 8 transpile + 7 cli + 6 import = 277 green, clippy 0)
✅ Phase 26 — 문서: docs/TUTORIAL.md (진행형 입문 튜토리얼)
             설치~동시성~에러처리~타입~모듈/패키지~stdlib까지 따라 하며 배우는 구성.
             모든 코드 스니펫은 실행 검증. README 문서 섹션 추가, GUIDE.md에 최신 안내 배너.
             (문서만 변경 — 코드/테스트/버전 불변)
✅ Phase 27 — TCP 네트워킹 빌트인 (웹 프레임워크 1단계 전제)
             VmValue에 참조 타입 2개 추가: TcpListener(Arc<TcpListener>),
             TcpConn(Arc<Mutex<TcpStream>>). 채널처럼 Arc 공유(참조 의미론).
             빌트인(96-100): tcp_listen(addr), tcp_accept(s)→블로킹, tcp_read(c)→str(4096),
             tcp_write(c,s), tcp_close(c). 렉서에 \r \0 이스케이프 추가(HTTP CRLF).
             예제: examples/http_server.bang — 연결마다 spawn하는 동시성 HTTP 서버
             (curl로 동시 요청 10개 처리 검증). 정규식+JSON으로 요청 파싱/응답.
             제약: 블로킹 I/O + 고정 스레드풀 → 고동시성엔 추후 논블로킹 I/O 필요.
             웹 프레임워크 자체는 별도 패키지(bang_modules)로 분리 예정.
             (tests: 107 unit + 26 interp + 3 lexer + 9 parser + 36 resolver
             + 75 vm + 8 transpile + 7 cli + 6 import = 277 green, clippy 0)
✅ Phase 28 — 네트워킹 제약 보완 (탄력 풀 + 전체 읽기 + 타임아웃)
             1) scheduler.rs 탄력 풀: idle==0이고 cap 미만이면 임시 워커 생성,
                임시 워커는 유휴 10s 후 종료(base는 유지). cap=max(base,512).
                → 블로킹 핸들러가 고정 풀(num_cpus)을 초과해도 동시 처리(검증: 동시 30/30).
             2) tcp_read_until(conn, marker): marker까지 누적 읽기(HTTP 헤더 전체).
             3) tcp_set_timeout(conn, ms): 읽기 타임아웃(멈춘 클라이언트가 워커 영구 점유 방지).
             빌트인 101-102 추가. http_server.bang이 read_until + set_timeout 사용.
             (tests: 277 green, clippy 0)
✅ Phase 29 — 버그 수정: 채널이 List/Map을 Nil로 떨구던 문제 (이벤트 처리)
             원인: 채널은 runtime::Value를 나르는데 to_runtime/from_runtime이 스칼라만
             변환하고 컨테이너는 Nil로 떨굼 → 채널로 맵/리스트 전송 시 손실(+spawn 에러는 조용히 삼켜져 증상이 숨음).
             수정: to_runtime/from_runtime이 List/Map을 재귀 변환. (함수 등 참조타입은 여전히 채널 전송 불가→Nil)
             bang의 이벤트 모델: 이벤트 루프/리액터 없음. 채널+spawn+일급함수로 디스패치.
             예제: examples/event_loop.bang. 테스트: vm_test +3(채널 맵/리스트/이벤트디스패치).
             (tests: 280 green, clippy 0)
✅ Phase 30 — spawn 에러 가시화 + select(멀티 채널 대기)
             (1) spawn 에러 가시화: run/run_spawned 조인 시 에러를 삼키지 않고 stderr 경고
                 (warn_if_spawn_err). 제어 흐름 불변(부모 중단 안 함). parallel 블록은 기존대로 전파.
             (2) select(channels) → [index, value] (먼저 준비된 채널), 모두 닫히면 nil.
                 BangChannel::try_recv(논블로킹) 추가, 폴링(1ms) 기반. 빌트인 103.
             테스트: vm_test +2(select ready/all-closed). 
             (tests: 282 green, clippy 0)
✅ Phase 31 — 컴파일러 무한루프(종료성) 버그 수정 — v0.23.1
             증상: import 모듈 함수에 다중문(let 포함) 클로저를 인자로 넘기면 bang check가 무한루프(CPU 100%).
             근본 원인 2개 (resolver/typeck가 아니라 parser+lexer):
             (1) 파서 에러 복구의 진행 보장 누락: parse_program/parse_block 루프가
                 parse_stmt()==None인데 토큰을 소비 못 하면(예: 떠도는 '}') 무한 반복.
                 → 진행 안 되면 에러 기록 후 1토큰 강제 스킵(종료 보장).
             (2) 렉서가 '(' 안의 '{ }' 블록에서도 줄바꿈을 억제 → 다중문 클로저 인자가
                 한 줄로 뭉쳐 파스 에러/불균형 '}' 생성 → (1)의 무한루프 유발.
                 → paren_depth(usize)를 브래킷 스택(Vec<char>)으로 교체: 가장 안쪽이
                 '{'면 줄바꿈 유효, '('/'['면 억제. 다중라인 맵은 parse_map이 줄바꿈을
                 skip하므로 영향 없음.
             둘 다 의미 불변(정상 프로그램 동일), 종료성/파싱 정확성만 개선.
             회귀: parser +3(떠도는'}'종료/다중문클로저인자/다중라인맵), lexer +2(블록내 줄바꿈).
             (tests: 287 green, clippy 0)
✅ Phase 32 — VM 상수 풀 인덱스 폭 버그 수정 (u8→u16) — v0.23.2
             증상: 한 청크에 상수가 256개를 넘으면 OP_CONST 피연산자(u8)가 wrap-around되어
             엉뚱한 상수를 읽음(예: s260이 VAL_4 출력). bang-web test/run.bang도 288상수로 깨짐.
             수정: 상수 인덱스를 u16으로 확장. Chunk::add_constant→u16(+중복제거),
             get_constant(u16), 컴파일러 add_const/add_constant→u16.
             u8→u16 emit·디코드 일치: OP_CONST, OP_CLOSURE(fn_const_idx), OP_FIELD_GET(name_idx).
             중복제거: const_eq로 Int/Float/Bool/Str/Nil 동일값 1슬롯 재사용(함수는 매번).
             JIT(jit.rs)도 OP_CONST 크기 2→3, 피연산자 u16 디코드로 일치.
             (참고: --features jit 자체는 cranelift 버전 비호환으로 기존부터 빌드 불가 — 별개.)
             회귀: vm_test +2(상수 270개 인덱스 정확 / 중복 300회). 의미 불변.
             (tests: 289 green, clippy 0)
✅ Phase 33 — 바이너리 파일 소켓 전송 빌트인 (104-105) — v0.23.3
             read_file는 UTF-8 전용, tcp_write도 Str(UTF-8)라 바이너리 무손실 전송 불가했음.
             file_size(path)→Int (std::fs::metadata().len(), Content-Length용),
             tcp_send_file(conn, path)→Int (std::fs::read 후 write_all, UTF-8 변환 없이 전송,
             보낸 바이트 수 반환). 잘못된 경로는 RuntimeError.
             → bang-web가 헤더는 tcp_write, 본문은 tcp_send_file로 보내 바이너리(이미지 등) 무손실.
             회귀: tests/binary_test.rs +3 (file_size / 없는파일 에러 / 소켓 바이너리 왕복 cmp).
             기존 빌트인 번호·시그니처 불변(추가만). (tests: 292 green, clippy 0)
🚧 Phase 34 — 논블로킹 I/O (C10K) — 단계 0: 리액터 골격 (진행 중)
             목표: 적은 스레드로 수많은 동시연결. 단계적 진행(실행모델 변경이라 큼).
             단계 0(완료, 추가만 — VM/스케줄러/표면API 무변경):
               - 의존성 polling 3.x 추가(epoll/kqueue/IOCP 크로스플랫폼 래퍼).
               - src/reactor.rs: Reactor(add_readable/modify_readable/deregister/wait).
                 fd readiness 대기. 단위 테스트 3(생성/접속시 readable/타임아웃).
             남은 단계(별 PR): set_nonblocking 소켓 + WouldBlock→yield, 리액터 readiness시
               태스크 resume(대기태스크↔fd 매핑 + 준비큐), 타임아웃·정리.
               난점: VM 중단/재개(yield/resume) — 현재 스케줄러는 태스크를 끝까지 실행.
               접근 결정 필요(코루틴 vs VM 상태머신). CPU 병렬(OS스레드 spawn)과의 양립도 과제.
             단계0은 표면 변경 없어 릴리스 없음. (tests: 295 green, clippy 0)