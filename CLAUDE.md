bang — 실제로 쓸 수 있는 동적 타입 범용 프로그래밍 언어의 컴파일러/런타임을
Rust로 처음부터 구현한다. 소스 확장자는 .bang, CLI/바이너리 이름은 bang
(bang run <file>, bang repl, bang build).

변경 금지 결정 (합의된 전제)


구현 언어: Rust (edition 2021).
렉서·파서는 직접 손으로 구현한다. 외부 파서 제너레이터(lalrpop, pest 등) 미사용.
백엔드 전략: 바이트코드 VM이 유일한 정본 실행 엔진이다. 네이티브 경로는 실험적 AOT C
트랜스파일러(bang compile)만 유지하며, 미지원 기능은 반드시 명확한 에러로 거절한다.
(이력: (1) 트리 워킹 인터프리터 → (2) VM 단계를 거쳤고, Phase 44(v0.30.0, 사용자 승인)에서
인터프리터와 Cranelift JIT를 제거했다 — 인터프리터는 빌트인 11/122 지원·타입 힌트를 조용히
무시했고, JIT는 빌드 불가였다. 새 백엔드는 VM과 같은 의미를 보장할 수 있을 때만 추가한다.)
에러는 항상 (line, col) span을 담아 사람이 읽기 좋게 출력한다.
한 번에 한 Phase만 진행한다. 사용자가 다음 Phase를 지시하기 전에는 범위를 넘지 않는다.


불변 규칙 (언어 의미 — 위반 금지)

값 의미론


컨테이너(list, map, string)는 값 의미론: 바인딩·인자 전달·반환·
채널 송신·spawn에서 모두 복제된다.
클로저는 변수를 붙잡는다(한 작업 안에서 공유 — Python식 늦은 바인딩). 작업 경계(spawn·
parallel_map·bang test)를 넘는 값은 vm::Isolator가 안에 든 클로저와 그 상태까지 복사한다
(Phase 53, 헌법 1조 개정). 새 작업 경계를 만들면 반드시 Isolator를 거칠 것.
함수·채널·Future는 참조 의미론(Arc 공유) — 단 작업 경계에서 함수(클로저 상태)는 Isolator가 복사하고
Future 결과도 격리된다. 작업끼리 값을 주고받는 명시적 통로는 채널과 작업의 반환값뿐이다.
이 규칙이 "작업 사이의 공유 가변 값 없음"을 보장한다(파일·소켓 등 외부 자원은 본래 공유됨). 성능을 이유로 컨테이너에 공유 가변 참조를
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
  compiler/     AST → 바이트코드 (Phase 5)
  vm/           스택 VM (Phase 5). 메모리는 참조 카운팅(Arc) + 세대별 순환 수집(cycles.rs, Phase 60, SPEC 15장)
  cycles        순환 참조 수집기 — 후보 상태 벡터에서 trial deletion (src/cycles.rs)
  runtime/      RuntimeError + 채널(BangChannel, try_recv는 select용) + 채널 전송용 데이터 전용 Value
  stdlib/       빌트인 단일 등록 테이블(mod.rs BUILTINS: 이름·인자 수·구현) + 분야별 구현 파일
                (core/concurrency/string/collections/math/json/time/fs/regex/net/sys/bytes/csv).
                resolver(이름·인자 수)와 VM(디스패치)이 이 테이블 하나를 읽는다 (Phase 44).
  scheduler/    M:N 스케줄러 (Phase 9) + 탄력 풀(Phase 28: idle 0이면 임시워커, cap 512)
  codegen/      [실험적] AOT C 트랜스파일러(transpile) — 정수·문자열·함수 부분집합, 미지원은 에러
  typeck/       정적 타입 검사 (Phase 25, bang check 통합)
  regex/        자체 정규식 엔진 (Phase 21~22, 백트래킹 VM + 캡처)
  pkg/          bang.toml 매니페스트 파서 (Phase 23, bang add/install)
  reactor/      논블로킹 I/O 리액터 (Phase 34, polling 기반; serve_event가 사용)
docs/SPEC.md    언어 명세 (동시성·serve_event 한계 포함). 의미 변경 시 함께 갱신.
docs/PHILOSOPHY.md 헌법(9조 + 영구 거부 목록 + 거절의 기록). 기능 제안은 이것과 대조.
docs/VISION.md  활성화 계획(철학+GTM 종합). P0~P2 실행 로드맵.
docs/TUTORIAL.md 진행형 입문 튜토리얼 (먼저 읽는 문서).
docs/GUIDE.md   사용자 가이드 (초기 버전 기준 — 최신은 TUTORIAL/SPEC).
wiki/           GitHub Wiki 사용자 매뉴얼 원본(한국어 14페이지). 사용자에게 보이는 동작이 바뀌면 함께 갱신하고
                python3 wiki/tools/verify_wiki.py 로 검증(bang 블록 실행·출력 비교 + 내부 링크 검사).
                게시: sh wiki/tools/publish.sh (위키 push 권한 필요).
정본 실행 엔진: VM. SPEC의 의미는 VM 기준이며, 다른 백엔드는 미지원 기능에서 명확한 에러를 내야 한다.

빌트인 추가 절차 (번호 = src/stdlib/mod.rs BUILTINS에서의 위치, 바이트코드에 박히는 대외 계약)
  1. 분야 파일(src/stdlib/<분야>.rs)에 구현 함수 작성:
     pub(crate) fn b_<이름>(vm: &mut Vm, args: Vec<VmValue>, span: Span, stop_depth: usize)
       -> Result<VmValue, RuntimeError>
     인자 검사는 req_args/str_arg/int_of/list_arg/bytes_arg/data_arg. 실패는 RuntimeError(catch 가능),
     unwrap/panic 금지. 쓰지 않는 매개변수는 _ 접두사.
  2. BUILTINS **끝에만** 한 행 추가: Builtin { name, arity(None=가변), func }, // 번호
     기존 행의 순서·이름·인자 수는 절대 바꾸지 않는다.
  3. tests/builtin_table_test.rs GOLDEN 끝에 같은 행 추가 (기존 행 수정 = 계약 위반).
  4. 결정적 동작이면 tests/fixtures/builtins_snapshot.bang에 호출 추가 + .expected 갱신.
  5. 문서: SPEC 해당 장, llms.txt(개수 포함), README 표, TUTORIAL(필요 시),
     editors/vscode-bang 문법 빌트인 목록·README 개수, CLAUDE.md 체크리스트.
빌트인 명명 규칙
  - 분야 접두사: tcp_* (소켓), regex_*, json_*, proc_* (프로세스), env_* (환경),
    file_* (파일), dir_* (디렉터리), bytes_* (바이트 변환), path_* (경로 문자열), csv_* (CSV),
    http_* (HTTP 클라이언트). 동사는 접두사 뒤: file_read_bytes.
  - 기존 비접두사 이름(read_file, list_dir, is_dir, sleep 등)은 번호 계약상 유지하며 개명하지 않는다.
  - 범용 연산(len, slice, push, map 등)은 접두사 없이 여러 타입을 받는 하나의 이름으로 확장한다.
의존성: polling(리액터), rustls+webpki-roots(https feature, 기본 켜짐 — 자급 코어의 유일한 예외).
        CI는 --no-default-features 빌드도 검증. (cranelift는 Phase 44에서 JIT와 함께 제거)
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
✅ Phase 35 — 논블로킹 I/O 단계 1: 이벤트 루프 서버 serve_event (106) — v0.24.0
             결정: B(의존성 없는 길) 선택 → VM 중단/재개(코루틴) 대신 "연결 단위 상태머신을
             런타임에" 두는 방식. VM 실행 모델·spawn·기존 빌트인 전부 무변경(추가만).
             serve_event(addr, handler): 단일 스레드가 리액터(polling)로 다수 연결을 멀티플렉싱.
             리스너+모든 연결 fd를 readiness 등록 → 준비된 것만 read, 요청 완성(\r\n\r\n) 시
             handler(req_str)→resp_str 호출(run_spawned 재사용) 후 write. keep-alive 재무장.
             유휴 keep-alive 연결이 스레드를 점유하지 않음 → C10K 1차 달성.
             핸들러 예외는 경고+연결 종료. 단일 스레드라 CPU 무거운 핸들러는 루프 블록(2차 과제).
             회귀: tests/event_server_test.rs +2(동시 100연결 멀티플렉싱 / keep-alive 3요청).
             (tests: 297 green, clippy 0)
             남은 2차: 활성 처리량을 스레드 cap 넘게 확장(핸들러 풀 오프로드 또는 진짜 VM 중단/재개).
✅ Phase 36 — 정수 오버플로 의미 통일 (P0-1)
             이전: debug=panic(불변규칙 위반)/release=조용히 wrap/인터프리터=wrapping — 전부 불일치.
             통일: 모든 int 연산(+,-,*,/,%,단항-,abs,sum,gcd) 오버플로 = try/catch로 잡히는
             RuntimeError. VM·인터프리터·상수폴딩(오버플로 시 폴딩 포기) 동일. debug==release.
             부수: 인터프리터 `10 % 0` panic 잠재버그 수정. SPEC 3.1 명시. vm_test +3. (300 green)
✅ Phase 37 — 테스트 CI (.github/workflows/ci.yml) (P1-3)
             push/PR마다 ubuntu/macos/windows에서 cargo test + clippy(-D warnings).
             Windows 첫 테스트에서 binary_test의 경로 백슬래시(이스케이프 충돌) 버그 발견·수정
             (bang 소스에 경로를 박을 땐 '/'로 정규화 — path_str 헬퍼).
✅ Phase 38 — REPL을 VM 기반으로 전환 (P0-2) — v0.25.0
             이전: REPL이 트리워킹 인터프리터라 try/catch·import·타입힌트·JSON/정규식/tcp 등
             VM 전용 기능 전부 불가(튜토리얼과 불일치, 신규 사용자 첫경험 문제).
             전환: compile_repl(시드 전역 name→slot 유지) + Vm::with_globals(공유 전역 Arc).
             스니펫마다 새 VM(잔여 상태 없음), 전역은 Arc로 지속. print는 VM이 직접 stdout.
             emit_load Unknown → 컴파일 에러(REPL에서 오타가 조용히 nil이 되지 않게;
             정상 경로는 resolver가 선차단이라 영향 없음). 에러 후에도 세션 유지.
             검증: 상태 유지/함수 정의/try·catch/json_parse/regex_find/타입힌트/import/exit 전부 REPL 동작.
             (tests: 300 green, clippy 0)
✅ Phase 39 — 크래시급 버그 4종 수정: 스택 오버플로 abort → 잡히는 에러
             재검토 실측으로 발견 (모두 try/catch 불가한 프로세스 abort였음):
             (1) 순환 import(a→b→a) 무한 재귀 → module_loading 집합으로 감지, 에러에 순환 경로 표시.
             (2) 깊은 JSON 중첩([[[…) → JsonParser depth 가드(512).
             (3) 깊은 괄호/블록 중첩 → 파서 enter_nest/exit_nest 가드(MAX_NEST_DEPTH=128;
                 spawn 워커/테스트 스레드 스택 2MB 기준 보수적. import 리팩터: import_module 메서드).
             (4) 정규식 깊은 그룹 중첩 + 거대 {n,m} 반복(디슈가 폭주) → 깊이 200/반복 1000 상한.
             참고: bang 자체 깊은 재귀(10만 프레임)는 정상 — VM 프레임이 힙에 있음(설계 검증).
             회귀: parser +1, import 통합 +1, vm_test +4. (tests: 306 green, clippy 0)
✅ Phase 40 — 활성화 계획 실행 P0 (VISION.md) — v0.26.0
             (0) docs/VISION.md 제정: 철학·언어채택사·GTM 3렌즈 병렬분석 종합(활성화 계획).
             (P0-1) 아웃바운드 네트워킹 빌트인 tcp_connect(107)/http_get(108, 자급 HTTP/1.1
                 클라이언트; Content-Length·chunked, https는 명시적 에러). 킬러 데모
                 examples/fanout.bang(parallel_map+http_get 4줄) + bench/fanout.py(aiohttp 대비).
                 http_client_test +5.
             (버그) 리졸버가 top-level 함수 전방 참조·상호 재귀를 막던 것 수정: 본문 해석 전
                 함수 바인딩 prescan(값 바인딩은 제외 — Nil 함정 회피). resolver_test +4.
             (P0-2) README 첫 화면: 구현이력→정체성 문장+팬아웃 데모+순차성능 정직 선공개.
             (P0-3) bench/README.md "지는 것부터": 순차 fib bang 9.9s vs CPython 2.5s(3.9x 느림),
                 병렬 2.1s(4.7x). 재현 bench/run.sh + 하드웨어 명세.
             (P0-4) docs/PHILOSOPHY.md 헌법 승격: 9조 + 영구 거부 목록 + 격언 6 + 거절의 기록.
             (tests: 315 green, clippy 0)
✅ Phase 41 — 활성화 계획 실행 P1 (VISION.md) — v0.27.0
             (P1) llms.txt: LLM이 파일 하나로 bang을 쓰게 하는 컨텍스트(문법·동시성·빌트인
                 108개 시그니처·관용구·"없는 것"). 훈련데이터 0 신생언어의 저비용 채택 레버.
             (버그) else-if 체인 뒤 문장 파스 에러 수정: parse_if가 항상 consume_stmt_end
                 하는데 else-if가 parse_if로 재귀→내부가 종결 먹음. parse_if_no_end 분리(최외곽만 소비).
                 else-if는 예제·테스트에 없어 드러나지 않은 오래된 버그. parser +1.
             (P1) docs/WHY.md: 'GIL 난민' 대상 — threading(GIL)/multiprocessing(pickle)/
                 asyncio(색칠) vs bang 코드 대 코드 비교. 스니펫 실측 검증.
             (P1) bang new <dir>: 프로젝트 스캐폴딩(main.bang+bang.toml+.gitignore). cli_test +1.
             (P1) VS Code 확장(editors/vscode-bang) 문법 갱신: try/catch/throw 키워드 +
                 빌트인 108개 전체 하이라이팅 + 확장 README. (확장 자체는 6월 기존 존재)
             (tests: 317 green, clippy 0)
✅ Phase 42 — 시스템 빌트인 (109-116) + 채널 참조 타입 송신 에러화 — v0.28.0
             proc_run(argv)→{code,stdout,stderr}: 셸 미경유(argv[0] 직접 실행), stdin=null,
               0 아닌 종료코드는 에러 아님, 시그널 종료 code=nil, 비-UTF8은 U+FFFD(바이트 타입 전 한계).
             env_get(name)→str|nil, env_cwd()→str, sleep(ms)(음수 에러), file_remove(파일만),
             file_move(같은 볼륨 rename), dir_create(create_dir_all, 멱등), file_append(없으면 생성).
             의존성 없음(std만). 신규 빌트인 명명: proc_/env_/file_/dir_ 접두사(기존 이름은 불변).
             (버그) 채널로 함수/채널/Future/Tcp 송신 시 조용히 Nil → catch 가능한 에러.
               to_runtime이 Result 반환, List/Map 안쪽까지 재귀 검사. SPEC 6.4 + 10장 갱신.
             검증: spawn 32개 × sleep(300) = 0.65s(순차 9.6s) → 탄력 풀이 블로킹 sleep 감당.
             발견(미수정, 별도 Phase 필요): VM parallel_map이 순차 실행(vm.rs 빌트인 11,
               "sequential in Part A" 잔재). 팬아웃 데모가 실제로는 병렬이 아님.
             참고: cargo fmt --check는 이 Phase 이전부터 약 1만 줄 불일치(정렬 공백) — 별도 정리 필요.
             회귀: tests/system_test.rs +10. (tests: 327 green, clippy 0)
✅ Phase 43 — 바이트 값 타입 + HTTPS — v0.29.0
             1부: VmValue::Bytes(Arc<Vec<u8>>) / runtime::Value::Bytes — 불변이라 Arc 공유가
               관찰상 깊은 복사와 동일(값 의미론 유지, COW도 불필요). 리터럴 문법 없음.
               연산: len, b[i]→Int, 음수 인덱스, slice, +, ==. b[i]=v는 에러(불변). for-in 미지원.
               타입 힌트 bytes(TypeAnn 태그 9, typeck Ty::Bytes). 표시 <bytes N: hex>(32바이트까지).
               빌트인(117-122): bytes_from_str, bytes_to_str(비-UTF8 에러), bytes_from_hex,
               bytes_to_hex, file_read_bytes, tcp_read_bytes. write_file/file_append/tcp_write는
               Str|Bytes 둘 다 받음(data_arg). 채널·spawn 전달 지원. json_stringify는 에러.
               PHILOSOPHY.md에 "수용 기록"(Set 거절과의 차이) 추가.
               회귀: tests/bytes_test.rs +9 (256값 파일 왕복, 비-UTF8 소켓 에코 포함).
               (tests: 336 green, clippy 0)
             2부(HTTPS): 사용자 결정 = rustls+ring + webpki-roots, `https` feature(기본 켜짐)로 격리.
               비교 실측(Windows, lto thin): native-tls +39KB(단 Linux는 시스템 OpenSSL 동적 링크 →
               단일 바이너리 깨짐, OS별 TLS 동작 상이) / rustls+ring +1.54MB / rustls+aws-lc +2.46MB.
               결과: bang dist 바이너리 1.47MB → 2.95MB. 헌법 5조의 유일한 의존성 예외(PR 이유는 Cargo.toml 주석).
               http_get: https:// 허용(기본 443), 스트림을 Box<dyn HttpStream>으로 추상화(HTTP 코드 공유),
               핸드셰이크를 tls_wrap에서 명시 완료(인증서 오류 = "TLS 핸드셰이크 실패"),
               close_notify 없는 종료(UnexpectedEof)는 EOF로 취급. ClientConfig는 OnceLock 1회 생성.
               --no-default-features면 https:// = "지원하지 않음" catch 가능 에러.
               CI: --no-default-features test+clippy 단계 추가(자급 코어 빌드 유지 보장).
               회귀: http_client_test 갱신(https 실패 catch / 미지원 스킴 / #[ignore] 실인터넷 https).
               실측: example.com·example.org https 200, badssl 만료·자체서명·호스트불일치 모두 거부.
               SPEC 12장 신설(한계: 사내 자체 CA 미지원, GET만, body는 str).
               (tests: 337 green 양쪽 feature, clippy 0)
             발견(미수정): 렉서가 UTF-8 BOM을 오류로 처리 — Windows 편집기/PowerShell 5.1이 저장한
               파일이 첫 글자에서 실패.
✅ Phase 44 — 빌트인 등록 단일화 + 실행 백엔드 정리 (VM 단일 정본) — v0.30.0
             2부(빌트인): src/stdlib/mod.rs BUILTINS(이름·인자 수·fn 포인터) 단일 테이블. resolver의 목록과
               vm.rs BUILTINS(&[&str])·call_builtin 거대 match(1,248줄) 제거 → 테이블 디스패치 1줄.
               분기 123개를 분야별 12개 파일 b_<이름> 함수로 이동(self.→vm.), 빌트인 전용 헬퍼
               (JSON 파서·HTTP 클라이언트·proc_run·hex·PRNG·시간포맷·regex 컴파일)도 이동.
               VM 메서드 5개·헬퍼 다수를 pub(crate)로 공개. 빌트인 추가 절차·명명 규칙은 위 "모듈 경계" 참고.
               리팩터링 전 작성한 계약 테스트: tests/builtin_table_test.rs(GOLDEN 123행 이름·번호·인자 수
               + fixtures/builtins_snapshot 동작 스냅샷 31줄) — 전후 동일하게 통과.
               성능(Windows release, 번갈아 실행): fib_seq 9회 중앙값 전 13.26s / 후 13.19s,
               빌트인 마이크로벤치 전 0.926s / 후 0.886s → 저하 없음.
             1부(백엔드, 사용자 승인): 트리 워킹 인터프리터(834줄)·Cranelift JIT(464줄+의존성 5) 삭제.
               --interp/--jit는 "v0.30.0에서 제거" 명확한 에러(파일 경로 오인 방지, cli_test +1).
               interpreter_test 26개 → tests/semantics_test.rs로 이식(단언 불변, 실행기만 VM).
               vm_test 예제 12개: 인터프리터 비교 → 예제 상단 "// 기대 출력:" 주석 비교로 전환
               (hello/conditionals 주석 신설, fibonacci 주석을 실제 출력 형식으로 정정).
               삭제한 테스트: test_vm_interp_flag_still_works 1개(인터프리터 존재 확인용).
               runtime.rs: 인터프리터 전용(BangFunction/Env/BangFuture/resolve 헬퍼) 제거, Value를 데이터
               전용으로 축소 → "채널은 데이터만" 타입 보장 + unsafe impl Send/Sync 제거.
               AOT는 [실험적] 유지: 타입 힌트를 조용히 무시하던 것 → 컴파일 에러(transpile 단위 +1).
               cc 없을 때 transpile_test 스킵을 stderr에 표시. SPEC 1장 "VM이 정본" 명시.
               CLAUDE.md 변경 금지 결정의 백엔드 전략 문구 갱신(사용자 승인).
               vm.rs 3,234 → 약 1,600줄. (tests: 343 green 양쪽 feature, clippy 0)
✅ Phase 45 — 기초 결함 수정: COW 실효화 + 중첩 경로 대입 + parallel_map 병렬 + BOM — v0.31.0
             (1) COW 무력화 버그: 대입이 변수를 스택에 로드(Arc 참조 2)한 뒤 make_mut → 매번 전체 복사.
               a[i]=v 2만 회 1.9s, 맵 2만 삽입 9.9s, 20만 삽입 >120s, xs=push(xs,v) 4만 회 38s (O(n²)).
               수정: OP_SET_PATH(79)·OP_UPDATE_VAR(80). 변수 슬롯이 로드한 복사본과 같은 Arc면 복사본을
               버리고 슬롯을 제자리 변경(make_mut이 공유 중일 때만 복사 — 관찰 의미 불변).
               키·값 평가 중 변수가 재대입되면 원래 의미대로 복사본을 고쳐 저장, 에러 시 원값 복원.
               x = push(x, e) / x = x + e 패턴도 제자리(push가 빌트인일 때만).
               결과: 위 작업 모두 수십 ms (맵 20만 삽입 0.17s).
             (2) 중첩 경로 대입 m["a"][0]=9, g[i][j]=v, cfg.db.port=1 (이전: "복잡한 인덱스 대입 미지원").
               중간 맵 키 없음/범위 초과는 catch 가능 에러, 루트가 변수가 아니면 컴파일 에러.
               대입식의 값 = 대입한 값(이전엔 컨테이너).
             (3) parallel_map 진짜 병렬화 (Phase 42 발견 결함): 원소마다 deep_clone_closure 격리,
               작업 최대 256개(초과 시 연속 구간 분할), 전부 조인 후 순서상 첫 에러. 빌트인 함수는 순차.
               검증: 8 × sleep(300) = 0.3s (이전 2.4s).
             (4) 렉서가 UTF-8 BOM 무시 (Phase 43 발견 결함).
             SPEC 2.1/4.2/6.5 갱신. 회귀: lexer +1, vm_test +9. (tests: 353 green, clippy 0)
✅ Phase 46 — 문법 편의 묶음 (Python 체감 1차) — v0.32.0
             렉서: 복합 대입 토큰(+= -= *= /= %=), /* */ 블록 주석(줄바꿈 포함 시 줄바꿈 1개로 취급,
               미종결 에러), """여러 줄 문자열""", 숫자 1_000·0xff·0b101·0o17·1e3·2.5e-3
               (숫자 뒤 문자 붙으면 에러), 유니코드(한글) 식별자.
             파서/AST: ExprKind::CompoundAssign·Slice, BinaryOp::In/NotIn(비교 수준, `not in` 2토큰),
               StmtKind::For{vars: Vec<String>} (for k, v in m).
             컴파일러/VM: 복합 대입 — 변수는 x = x op e로 디슈가(Phase 45 갱신 경로 재사용), 경로는
               OP_SET_PATH binop 피연산자(키 1회 평가, 마지막 맵 키 없으면 에러).
               OP_SLICE(81, Python식 음수·잘라맞춤, step 없음) / OP_MAKE_ITER_PAIRS(82) / OP_IN(83) /
               OP_UNPACK(84). for-in이 map(정렬 키)·str(문자)·bytes(정수)도 순회.
             AOT: x op= e(변수)만 지원, in/슬라이스/경로 복합 대입은 명확한 에러.
             결정: `not`의 우선순위는 그대로(단항) — `not x in xs`는 (not x) in xs. SPEC 5장에 `x not in xs`
               사용을 명시. slice() 빌트인은 번호 계약상 기존 동작(음수→0) 유지, 문법 슬라이스만 Python식.
             문서: SPEC 2.1/2.2/2.5/3.1/4.1/4.2/5/7/11, llms.txt(세미콜론 쓰던 잘못된 관용구도 수정),
               VS Code 문법(블록 주석·삼중 문자열·숫자 리터럴).
             회귀: lexer +5, vm_test +6. (tests: 364 green, clippy 0)
✅ Phase 47 — 표현력 (Python 체감 2차): 메서드 문법·기본값 매개변수·분해 선언·if 식 — v0.33.0
             메서드 문법(UFCS): x.f(args)에서 f가 빌트인 이름이면 OP_METHOD_CALL(86) — 맵에 키 f가 있으면
               그 값 호출(맵+클로저 객체 관용구 우선), 아니면 빌트인 f(x, args). 빌트인 이름이 아니면 기존 필드 호출.
               값 의미론 유지(xs.sort()는 새 리스트). 문장 위치에서 순수 빌트인(resolver::PURE_BUILTINS) 메서드의
               결과를 버리면 런타임 에러(xs.push(v) 습관 차단), 함수 표기 push(xs, v) 단독 문장은 리졸버 경고(호환).
               spawn x.f()는 thunk 경로로 컴파일.
             기본값 매개변수: fn f(a, b = 2). 뒤쪽에만, 호출마다 평가(가변 기본값 공유 함정 없음), 명시적 nil은
               유지. CompiledFn.min_arity + CallFrame.argc + OP_JUMP_IF_ARG(85). 리졸버 arity가 (최소, 최대) 범위,
               do_call/run_spawned 범위 검사. typeck는 기본값 타입이 표기와 충돌하면 보고.
             분해 선언 let [a, b] = expr (StmtKind::LetUnpack, OP_UNPACK 재사용, 전역/지역 모두).
             if 식: 표현식 자리 if c { a } else { b } (ExprKind::IfExpr, else 필수, 각 갈래 마지막 문장=값).
             파서 깊이 가드 회귀(디버그 빌드 스택 오버플로) 발견·수정: 슬라이스/not in/복합 대입/if 식 파싱을
               #[inline(never)] 보조 함수로 분리해 parse_expr_bp 재귀 프레임을 작게 유지.
             AOT: 기본값·분해·if 식은 명확한 에러.
             문서: SPEC 3.3/4.1/4.2, llms.txt, TUTORIAL 11.5 "Python에서 왔다면"(스니펫 실행 검증).
             보류(사용자 결정 대기): 문자열 보간 문법.
             회귀: resolver +3, vm_test +7. (tests: 374 green, clippy 0)
✅ Phase 49 — 도구: bang test / bang fmt + assert_eq(123) + 맵 동등 비교 결함 수정 — v0.34.0
             (Phase 48 에러 모델은 사용자 결정 대기로 건너뜀)
             bang test [경로...]: *_test.bang / test_*.bang 발견(bang_modules/.git/target 제외), 최상위 1회 실행 후
               test_* 최상위 함수를 선언 순서로 격리 실행(Vm::run_global_fn_isolated = deep_clone_closure +
               run_spawned → 테스트 간 전역 격리). 실패/파일 오류 시 exit 1, 실패 위치 소스 문맥 출력.
             bang fmt [--check] [경로...]: src/fmt.rs 줄 단위 재들여쓰기(4칸). 여는 괄호 스택에 "연 줄의 수준"을
               기록해 한 줄에서 여러 괄호를 열어도 한 단계만, 연산자로 끝난 줄 다음은 hanging indent, 줄 끝
               공백·연속 빈 줄·CRLF 정리, 삼중 문자열/블록 주석 안쪽은 보존. 전후 토큰열 비교로 안전 검증
               (다르면 쓰지 않음). 저장소의 모든 .bang(examples/tests/bench)이 포맷 안정.
             assert_eq(actual, expected) 빌트인 123 (끝에 추가, GOLDEN·스냅샷 갱신).
             (버그) vm_eq에 Map 비교가 없어 {"a":1} == {"a":1}이 false였음 → 키 집합·값 구조 비교.
               참조 타입(함수·채널·Future·TCP)은 Arc 동일성. in/unique/index_of/union 등도 영향(올바르게).
               assert_eq 스냅샷 작성 중 발견.
             발견(미수정): print가 vm.output Vec에도 모든 줄을 누적 — 장시간 서버에서 메모리 증가 (테스트용 캡처 잔재).
             문서: SPEC 5장(동등 비교)·13장 신설, README 명령/표, llms.txt(123개), TUTORIAL 11.6, VS Code 빌트인 목록.
             회귀: fmt 단위 +7, cli_test +2, vm_test +2, 스냅샷 +1줄. (tests: 385 green, clippy 0)
✅ Phase 48 — 에러 모델 (사용자 결정 2·3·4, 헌법 7조 개정) — v0.35.0  [호환성 깨짐]
             (결정 2) 잡힌 내장 에러 = 맵 {kind, message, line, col} (이전: 메시지 문자열). throw한 값은 그대로.
               RuntimeError.kind(&'static str) + runtime::kind 상수 11종(zero_division overflow index key type
               argument value io assertion import error — 대외 계약). VM 핵심 사이트 51곳 명시 태그,
               빌트인 에러는 stdlib::categorize_error가 분야(이름 접두사)로 분류(인자 헬퍼는 argument 우선).
               spawn/parallel_map을 건너도 kind 유지. catch에서 throw e로 다시 던진 에러 맵이 끝내 안 잡히면
               원래 메시지·위치·kind로 복원(rethrown_error). 새 값 타입은 만들지 않음(작은 표면 — 거절 기록).
               주의: print(e)는 맵 전체를 출력 — 메시지는 e.message.
             (결정 3) 없는 맵 키 읽기(m[k], m.k) = key 에러 (이전: 조용히 nil). get/has/in으로 명시.
             (결정 4) C FFI 보류 — PHILOSOPHY 거절의 기록 + VISION 부록.
             헌법 7조 개정: "없는 맵 키 읽기도 잡히는 에러" 명시.
             (발견·수정) examples 중 기대 출력 주석이 있는데 테스트에 빠진 6개(error_handling·type_hints·
               json_demo·regex_demo·concurrency_demo·event_loop)를 vm_test에 추가 — 전부 통과.
             문서: SPEC 8.1~8.3, README, llms.txt, TUTORIAL 8장. 회귀: vm_test +4(+예제 6). (tests: 395 green, clippy 0)
✅ Phase 50 — f-문자열 보간 + format() 빌트인(124) (사용자 결정 1) — v0.36.0
             렉서: f"..." / f"""...""" → TokenKind::FStr(Vec<FStrPart>) (글자 조각 + {식[:형식]} 조각, 식의
               원본 줄·열 기록). {{ }}는 글자 중괄호, 식 안의 문자열·괄호 중첩 추적(f"{m["k"]}" 가능).
             파서: 조각마다 하위 렉서·파서로 식 하나를 파싱, 토큰 위치를 원본 기준으로 이동(에러 위치 정확).
               형식 지정자는 파스 시 검증. ExprKind::FString. 키워드 추가 없음(f는 따옴표가 붙을 때만 접두사).
             VM: OP_FMT(87, 형식 지정) / OP_BUILD_STR(88, n개 이어 붙임, Future 깊게 해소).
             형식 지정자 [[채움]정렬][0][너비][,][.정밀도][f|d|x|b|%] — stdlib::string 공유 구현(apply_format),
               format(fmt, ...)도 같은 규칙({} / {:형식}, 인자 수 불일치 = argument 에러).
             (버그) spawn len(x)처럼 빌트인을 직접 spawn하면 "클로저 필요" 에러 → thunk 경로로 수정.
             fmt: f-문자열 안 따옴표 인식, 토큰 비교 시 f-문자열 조각 위치는 무시(들여쓰기로 바뀌는 게 정상).
             문서: SPEC 7장, llms.txt(124개), README, TUTORIAL 11.5, VS Code(f-문자열 하이라이팅·빌트인 목록).
             회귀: lexer +1, fmt +1, vm_test +4, 스냅샷 +1줄. (tests: 401 green, clippy 0)
✅ Phase 51 — 맵 삽입 순서 유지 (Python dict와 같음) — v0.37.0  [출력 순서 바뀜]
             src/omap.rs OrderedMap<V>(엔트리 Vec + 키→위치 HashMap, 의존성 없음). VmValue::Map·runtime::Value::Map
               저장소 교체. 출력·keys()·values()·for k in m·json_stringify가 삽입 순서(이전: 키 정렬 5곳 제거).
               json_parse는 문서 순서 보존, merge(a, b)는 a 순서 뒤에 b의 새 키. ==는 순서 무관.
               채널·spawn을 건너도 순서 유지. import 모듈 export 맵은 선언(슬롯) 순서(이전: 해시 순서 — 비결정적).
             동작 스냅샷 2줄 갱신(keys 순서, json_stringify 순서) — 의도된 의미 변경(사용자 승인).
             문서: SPEC 3.2(맵의 순서)·5장, llms.txt, TUTORIAL 11.5 출력(실행 결과로 재생성).
             회귀: omap 단위 +2, vm_test +2. (tests: 405 green, clippy 0)
✅ Phase 52 — 실무 빌트인: 경로·CSV·HTTP POST (125-133) — v0.38.0
             경로(fs.rs): path_join(가변)·path_basename·path_dirname·path_ext(점 없음)·path_abs(std::path::absolute,
               존재 여부 무관). 문자열만 다루고 구분자는 OS를 따름.
             CSV(새 파일 stdlib/csv.rs): csv_parse(RFC 4180 — 따옴표·""·필드 안 줄바꿈·CRLF, 미종결 따옴표 = value 에러),
               csv_stringify(필요한 필드만 따옴표, nil은 빈 칸).
             HTTP(net.rs): http_get을 http_request(fname, method, url, body, headers)로 일반화.
               http_post(url, body[, headers]) / http_request(method, url[, body[, headers]]).
               body: str/bytes/map·list(JSON 자동, application/json)/nil, Content-Length 자동, 사용자 헤더가 기본값 대체
               (Host·Connection·Content-Length는 고정). 헤더 주입 방지(이름 영숫자·-·_, 값 줄바꿈 금지), 메서드 대문자만.
             categorize_error: csv_* → value, http_post/http_request/path_abs → io.
             명명 규칙에 path_* / csv_* / http_* 추가, 모듈 경계에 csv 추가.
             문서: SPEC 12.1·14장, llms.txt(133개), README 표, VS Code 빌트인 목록, 스냅샷 +2줄.
             회귀: csv 단위 +1, http_client_test +3(Rust 스레드 에코 서버), vm_test +2. (tests: 411 green, clippy 0)
✅ Phase 53 — 클로저 의미 정식화 + 작업 간 상태 공유 결함 수정 (헌법 1조 개정) — v0.39.0
             (발견) 문서는 "클로저 캡처에서 복제"였지만 실제 클로저는 변수를 붙잡음(Python식 늦은 바인딩).
               더 심각: spawn 경계가 호출 대상 클로저만 복사해서, 인자·맵 안의 "객체"(맵+클로저)나 Future로 받은
               객체의 상태가 스레드 사이에 공유 → 갱신 유실 경쟁(같은 프로그램이 25464/31571/37952). 불변 규칙 위반.
             (수정) vm::Isolator: 작업 경계(OP_SPAWN 대상+인자, parallel_map 원소별, bang test, serve_event)에서
               값에 닿는 모든 클로저·상태 벡터(지역·모듈 전역)를 메모이즈 복사(같은 객체 메서드끼리는 복사본에서도 공유,
               순환 안전 — 자리 먼저 만들고 finish()에서 채움). Future는 derived Future로 감싸 결과를 처음 꺼낼 때 격리.
               성능: 큰 리스트를 spawn마다 훑는 회귀(98ms→2201ms)를 "클로저 없음" 캐시로 해소(→83ms). 캐시는 Weak만
               보관 — Weak가 있으면 Arc::make_mut이 새 할당으로 옮기므로 내용이 바뀐 컨테이너는 절대 적중하지 않음.
             (결정, 사용자 승인 A안) 한 작업 안에서는 클로저가 변수를 공유(객체 관용구의 근거), 작업 사이에는 공유 없음.
               헌법 1조·불변 규칙 문구 개정, SPEC 3.3 "클로저가 붙잡는 것" 신설·6.1 갱신.
             (버그 2개 추가 수정) 함수 안의 이름 있는 함수가 자기 자신을 재귀 호출하면 컴파일 에러였음 → 슬롯 선언 후 컴파일.
               인덱스·필드 접근 대상 Future를 자동 대기하지 않았음(SPEC 위반) → OP_INDEX_GET/OP_FIELD_GET auto_resolve.
             회귀: vm_test +5 (늦은 바인딩·객체 공유, spawn 격리, 캐시 후 클로저 추가, 중첩 재귀, Future 격리+필드 대기).
             성능 실측(4코어): fib_par 변화 없음(4.4s). 큰 리스트 인자 spawn 2000회 HEAD 90ms → 83ms.
               미해결 비용: 끝나지 않은 Future가 많이 든 전역 리스트를 두고 spawn을 반복하면 매번 훑음
               (2000개 누적 시 90ms → 150ms). 끝났고 클로저 없는 Future는 재포장하지 않음.
             (tests: 416 green, clippy 0).
✅ Phase 54 — print 출력 대상 분리 (메모리 누적 제거) — v0.40.0
             vm::Output { Stdout, Capture(Arc<Mutex<Vec<String>>>) }. 이전: print가 println + VM 목록에 누적(테스트만 읽음)
               → 서버 메모리 무한 증가, 모든 스레드 print가 한 잠금 경쟁, 잠금 unwrap(규약 위반).
             CLI(run/REPL/test)는 Stdout(버퍼 없음), 테스트·내장은 Capture(화면에 안 씀). Vm::new/with_globals는
               impl Into<Output>(Arc를 넘기던 기존 테스트 호환), run_spawned는 Output, spawn 작업은 부모 대상 상속.
             실측: 100만 줄 출력 최대 메모리 72MB → 12MB (출력량과 무관해짐), 시간 동일.
             파이프 닫힘(bang x | head): 이전 panic 메시지 → 조용히 exit 141(128+SIGPIPE). 그 밖 쓰기 실패는 io 에러.
             문서: SPEC 10장. 회귀: vm_test +2, cli_test +1(unix 파이프). (tests: 419 green, clippy 0)
✅ Phase 55 — 구조적 동시성 완성: 함수 본문 조인 + spawn 에러 전파 (헌법 3조 위반 수정) — v0.41.0
             (발견, 헌법 점검) 3조·불변 규칙은 "가장 가까운 함수 본문 또는 parallel 블록에 뿌리내려 스코프 종료 시
               조인"인데, 함수 본문 조인이 없었음 — 함수가 자식을 기다리지 않고 반환, 프로그램 끝에서만 조인.
               자식 에러는 경고만 내고 exit 0 → 실패 감지 불가.
             (수정) CallFrame.spawned: parallel 블록 밖 spawn은 실행 중인 프레임에 등록, OP_RETURN/암묵 반환 시 조인.
               SpawnScope{frame_depth}: 같은 프레임의 parallel 블록이면 블록에 등록(블록 안에서 호출한 함수의 spawn은
               그 함수 본문에). 프로그램 레벨 스코프 제거 — 최상위 spawn은 메인 프레임에 뿌리내림.
               에러: VmFuture.observed(값으로 받아 본 에러 표시) + join_unobserved — 아무도 받지 않은 첫 에러를 스코프
               종료 시 던짐(catch 가능, 미처리 시 exit 1). 이미 처리한 에러는 다시 던지지 않음.
               되감기(unwind_to)·에러 종료(drain_pending_tasks)도 남은 작업을 모두 조인(에러는 경고).
             (함께 수정한 결함) parallel 블록이 첫 에러에서 나머지 작업을 기다리지 않고 빠져나감 → 전부 조인 후 첫 에러.
               try 안 parallel에서 throw하면 spawn 스코프가 스택에 남던 문제 → TryHandler.spawn_depth로 정리.
             성능: fib_seq/fib_par 변화 없음(측정 잡음 범위).
             문서: SPEC 6.3·6.3-1(신설)·6.4-2. 회귀: vm_test +5. (tests: 424 green, clippy 0)
✅ Phase 56 — 에러 위치 정확화 (헌법 7조 위반 수정) — v0.42.0
             (발견, 헌법 점검) parallel_map/spawn의 인자 수 에러 등 일부 에러에 위치가 없었음(run_spawned가 Span(0,0)).
               추가 발견: import한 모듈 안의 에러가 메인 파일의 같은 줄 번호를 보여 줘 엉뚱한 줄(빈 줄)을 가리켰음.
             (수정) RuntimeError.file + CompiledFn.file(compile_with_file — import가 모듈 경로를 기록).
               exec_until이 위치 없는 에러에 현재 지점을, 파일 없는 에러에 실행 중 함수의 파일을 채움.
               표시: [모듈경로 줄:열], CLI(run/REPL/test)는 그 파일의 소스 줄을 보여 줌(runtime_error_text).
               run_spawned(call_span): 작업 시작 전 에러는 spawn/parallel_map 호출 지점.
               auto_resolve가 작업 에러의 위치를 "값을 쓴 줄"로 덮어쓰던 것 → 원래 난 줄 유지(위치 없을 때만 채움).
             문서: SPEC 8장. 회귀: import_test +1, vm_test +2. (tests: 427 green, clippy 0)
✅ Phase 57 — 몰래 바뀌는 동작 제거 ("암묵 변환 금지" 위반 3건, 사용자 승인) — v0.43.0  [호환성 깨짐]
             (1) 나눗셈: `/`는 항상 float (이전: 나누어떨어지면 int — 값에 따라 타입이 바뀜). 상수 폴딩도 동일.
               정수 몫은 빌트인 idiv(a, b)(134, 내림 — Python //와 같음, int/int → int). `//`는 bang 주석이라 연산자 불가
               → 추천안(//)을 함수로 대체. float 표시에 소수점 유지(4.0, 1e3 → 1000.0 — fmt_float, 출력·str·JSON 공통).
             (2) 참거짓: Python 규칙 — false·nil·0·0.0·""·[]·{}·빈 bytes가 거짓 (이전: false·nil만).
             (3) 맵 키: str 또는 int (1과 "1"은 다른 키). 그 밖의 키는 type 에러 (이전: 무엇이든 to_string).
               저장소는 문자열 키 유지 — int 키는 표식(\u{1}i…)으로 인코딩, 표식으로 시작하는 문자열 키는 이스케이프
               (map_key/encode_str_key/decode_key/key_display). keys()·for·출력·JSON에서 원래 타입으로 복원.
               리터럴 정수 키 {1: "a"}, {-2: x} (MapKey::Int). 채널·spawn을 건너도 유지.
             동작 스냅샷 갱신(float 표시 3줄, bool() 1줄) — 의도된 의미 변경. 기존 테스트 기대값 5곳 갱신.
             문서: SPEC 3.1(나눗셈·float 표시·참거짓)·3.2(맵 키), llms.txt(134개), TUTORIAL 3·8장, README, VS Code.
             회귀: vm_test +4, 스냅샷 +1줄. (tests: 431 green, clippy 0)
✅ Phase 58 — AOT를 VM 의미에 맞춤 (헌법 4조 위반 수정) — v0.44.0
             (발견, 헌법 점검) bang compile 결과가 VM과 조용히 달랐음: 정수 오버플로 wrap, 7/2 → 3, 참거짓, float 표시(%g),
               len이 UTF-8 바이트 수, int("abc") → 0, float(true) → 1.0, 문자열 비교 불가, floor/ceil/round가 float,
               % float 미지원, float 리터럴 1e21이 C 정수 상수로 넘침, exit() 인자 없으면 실패, min/max 리스트 인자.
             (수정) C 런타임 재작성: __builtin_*_overflow 검사, / 는 float(int/int 0은 에러, float은 IEEE), Python 참거짓,
               bv_fmt_float(가장 짧게 왕복되는 자릿수를 지수 없이 + ".0" — VM fmt_float와 같음), UTF-8 문자 수,
               엄격한 int/float 변환(trim 후 전체 파싱), 문자열 비교(strcmp = Rust 바이트 순), floor/ceil/round → int
               (Rust as i64처럼 포화), pow int 규칙, fmod, 에러 = "런타임 오류: …" + exit 1. float 리터럴은 {:e}로 출력.
               min/max 인자 2개 아니면 컴파일 에러, assert 메시지 VM과 같게.
             VM↔AOT 동치 테스트(tests/transpile_test.rs aot_matches_vm_on_corpus): 23개 프로그램(에러 경우 포함)을
               두 엔진으로 실행해 stdout·종료 코드 비교 — 전부 일치. 한계: AOT 에러 메시지에 줄·열 없음.
             문서: SPEC 1장(AOT 지원 범위·동치 보장). (tests: 432 green, clippy 0)
✅ Phase 59 — 헌법·문서를 현실에 맞춤 (헌법 점검 B·C 항목) — 문서만 변경, 버전 불변
             헌법: 정체성 문장 "공유 가변 상태 제거" → "작업 사이의 공유 가변 값"(파일·소켓 등 외부 자원은 본래 공유).
               5조 의존성 정정(polling + https용 rustls/ring/webpki-roots, 기능 플래그로 제외 가능).
               9조 명확화("타입 힌트가 필수가 되는 일은 없다" — 하위 호환 약속 아님, 구조 오류는 항상 거절).
               거부 목록: 락 행("채널이 유일한 통로" → 채널·반환값), "컨테이너 공유 가변 참조" → "작업 사이의"(1조 개정과 일치).
             언어 정책 이행: README 영어 요약 섹션 신설(정체성·데모·구조적 동시성·정직한 한계·문서 안내).
             SPEC 15장 신설: 참조 카운팅, 순환 참조 누수(클로저가 끼는 두 경우·실측 298MB vs 12MB·회피법 — 실측 검증).
             SPEC 6장: 자동 대기는 얕다([spawn g()] == [3]은 false, wait로 꺼내 비교 — 실측 검증).
             README 값 의미론 문구 과장 수정, VISION 의존성 문구, CLAUDE.md 불변 규칙 문구·vm "GC" 표기 정정, llms.txt 순환 주의.
✅ Phase 60 — 세대별 순환 참조 수집 (메모리 누수 제거) — v0.45.0
             (발견) SPEC 15장에 적은 두 경우(자기 참조 객체·지역 재귀 함수)보다 심각한 누수: spawn·parallel_map·
               serve_event 요청마다 Isolator가 복사한 모듈 전역 ↔ 그 안의 함수 순환이 해제되지 않아 작업마다 약 260B 누수
               (spawn 20만 회 75MB, 횟수에 비례) — 장시간 서버에 치명적.
             (구현) src/cycles.rs: 후보 상태 벡터에서 닿는 부분 그래프(상태 벡터·클로저·upvalue·클로저를 품은 리스트/맵;
               클로저 없는 컨테이너·Future는 건너뜀)의 내부 참조 수를 세어, 외부 참조가 없는 순환만 상태 벡터를 비워 끊음
               (trial deletion). 후보: (1) 클로저가 붙잡은 채 반환된 프레임 locals(release_locals, 되감기 포함) —
               젊은 세대 2048개마다 수집, 생존자는 늙은 세대(살아남은 수의 2배가 되면 재검사, 최소 8192),
               (2) 작업 경계 Isolator가 만든 상태 벡터 — 작업 종료 시(OP_SPAWN·parallel_map 원소·serve_event 요청·bang test),
               (3) 작업 VM의 후보 전부 — run_spawned 끝에서 VM을 버린 뒤. isolate_closure가 (클로저, 새 상태 벡터) 반환.
             (함께 수정한 성능 결함) Isolator가 끝난 Future의 결과를 "격리할 게 있나" 검사하며 실제로 통째로 복사(버려지는 일)
               → 전역에 객체 Future를 모으며 spawn하면 O(n²) (300회 1.1초). contains_closure 순수 검사로 교체 → 14ms.
             안전 검증: 수집 주기를 1(매 반환마다 수집)로 낮춘 빌드로 전체 테스트 436개 통과, 예제·검증 스크립트 29개의
               출력이 일반 빌드와 동일. 회귀 테스트: 살아 있는 객체 다수 수집 후 유지, 실행 중 프레임 보호, 작업 결과 객체 유지,
               쓰레기 순환 실제 해제(cycles::freed_cells_total).
             실측: 순환 객체 30만 개 298MB → 14MB, spawn 20만 회 75MB → 12MB, parallel_map 반복 19MB → 12MB.
               fib_seq/fib_par·큰 리스트 spawn 시간은 잡음 범위.
             남은 한계: 오래 산 뒤 버려진 객체는 늙은 세대 재검사 때까지 남음(SPEC 15장).
             문서: SPEC 15장 재작성, README 영어 요약, llms.txt(순환 주의 문구 제거), VISION(예외 기록).
             회귀: vm_test +4. (tests: 436 green, clippy 0)
✅ Phase 61 — AOT 에러를 VM과 글자 단위로 같게 + VM 산술 암묵 변환 결함 수정 — v0.46.0
             AOT: 실패할 수 있는 연산·빌트인마다 인자를 먼저 계산한 뒤(GNU 문장식으로 순서 고정) 소스 위치 기록
               (Transpiler::at), bv_fail이 VM과 같은 "[줄:열] 런타임 오류: …" + 원본 소스 줄·^ 표시. 원본 소스 줄을
               실행 파일에 포함(transpile_with_source, bang compile이 사용). VM 문구 재현(bv_tname — 타입 이름 포함 문구,
               as_float 문구, len/abs/int/float/exit/assert 문구). 에러 전 fflush(stdout)로 출력 순서도 VM과 같음.
             (VM 결함, 수집 중 발견) vm_arith가 한쪽이 float이면 숫자 아닌 쪽을 조용히 0.0으로 바꿈 → 2.5 * "x" = 0.0,
               1.5 - nil = 1.5, 7.5 % "a" = NaN. → type 에러. exit("x")가 조용히 exit 0 → argument 에러.
               (unreachable!() 2곳도 함께 제거)
             VM↔AOT 동치 테스트 확장: 36개 프로그램(에러 13개)으로 stdout·종료 코드·에러 출력 블록까지 비교 — 전부 일치.
             문서: SPEC 1장(AOT 동치 범위)·3.1(산술의 타입). 회귀: vm_test +2, transpile 동치 +13. (tests: 438 green, clippy 0)
✅ Phase 62 — 위키 매뉴얼 작성 중 발견한 결함 6건 수정 — v0.47.0  [동작 바뀜: input EOF·args·import 기준]
             (매뉴얼) wiki/ 한국어 14페이지 + verify_wiki.py(예제 77블록 실행·출력 비교 + 내부 링크 검사) + publish.sh.
             (1) 스크립트 인자: bang app.bang a b가 마지막 인자를 파일로 오인 → 경로 뒤 인자는 스크립트의 것.
               args()는 Python sys.argv 모양 [스크립트, 인자...] (이전: 프로세스 argv 전체). REPL·bang test는 [].
               stdlib::set_script_args(OnceLock) — CLI가 실행 전 1회 설정.
             (2) 타입 표기 fn·nil이 키워드라 파스 오류 → parse_type이 Fn/Nil 토큰을 타입으로 받음.
             (3) import 상대 경로가 현재 디렉터리 기준 → import를 쓴 파일의 디렉터리 기준(CompiledFn.src_dir,
               compile_main(메인: 표시용 file은 None 유지)·compile_with_file(모듈)). 바레 이름은 기준/name.bang →
               기준에서 위로 올라가며 bang_modules/ (Node식 — 패키지 안에서 프로젝트 의존성 import 가능) → BANG_PATH.
               REPL·stdin은 현재 디렉터리. SPEC 16장 신설.
             (4) bang add가 bang.toml을 [dependencies]만으로 다시 써 [package] 삭제 → pkg::update_manifest(섹션만 교체).
             (5) 작업·콜백을 건넌 throw 값이 {kind: error, message: "잡히지 않은 예외: …"} 맵으로 바뀜 → RuntimeError.thrown에
               원래 값을 실어 바깥 catch에 그대로 전달(spawn·parallel_map·함수 조인·map 등 콜백). derived Future는
               에러의 thrown 값도 Isolator로 격리(작업 간 공유 금지). 잡히지 않으면 메시지는 이전과 같음.
             (6) input()이 EOF에서도 "" → nil (빈 줄과 구별, Ruby gets와 같음).
             (문서 오류 수정) 빌트인 개수 134→135(llms.txt·VS Code README), TUTORIAL json_stringify 주석, llms.txt contains·min/max.
             문서: SPEC 8.1·10장·16장, llms.txt, README, wiki. 회귀: pkg +1, vm_test +4, cli_test +2, import_test +1.
             (tests: 446 green 양쪽 feature, clippy 0)
✅ Phase 63 — AI 시대 대응 1차: LLM API를 실제로 부를 수 있게 + 문서 검증 CI — v0.48.0
             (계기) HTTP 읽기 대기가 10초 고정·스트리밍 불가 → LLM API(응답 수십 초~분, SSE) 호출 불가.
             옵션 맵(http_post 4번째·http_request 5번째 인자, http_get은 번호 계약상 그대로): timeout_ms(기본 10000,
               0=무제한 — 연결·전송·읽기 각각의 상한), on_line(2xx 본문을 받는 대로 줄 단위로 콜백, false면 중단, body=nil;
               2xx 아니면 body에 모음). 모르는 키·잘못된 타입 = argument 에러. 콜백은 같은 작업에서 동기 호출(스레드 없음).
             본문 수신 재작성(BodyDecoder: chunked 점진 해석·Length·EOF·본문 없음(HEAD/1xx/204/304)) + LineSplitter.
             (기존 결함 수정) 대기 시간 초과·연결 끊김 시 잘린 본문을 조용히 반환 → io 에러. 데이터 안 "0\r\n\r\n"을
               chunked 끝으로 오판 → 점진 디코더로 해소. connect_timeout 적용.
             예제 examples/llm_fanout.bang (Anthropic Messages API: parallel_map 병렬 + SSE 스트리밍, ANTHROPIC_BASE_URL 지원).
             문서 검증: tests/docs_test.rs — llms.txt 빌트인 개수·이름 양방향·코드 블록 파싱, 위키 전체 목록=BUILTINS,
               VS Code 문법 빌트인·개수. 바로 발견·수정: llms.txt에 format·log10 누락, `break / continue`·`{ ... }` 파스 불가
               표기, VS Code 문법 log10 누락. CI docs 작업: verify_wiki.py(위키 예제 실행·링크) + bang fmt --check.
             문서: SPEC 12장·12.2 신설, llms.txt(옵션·SSE 관용구), README, wiki(네트워킹 옵션·LLM 절, FAQ).
             회귀: http_stream_test +5(타임아웃·잘린 응답·chunk 데이터·점진 스트리밍·중단/throw/에러 상태/옵션 검증·
               LLM 예제 종단(가짜 Messages API, 병렬 확인)), docs_test +4. lexer/parser 예제 개수 20→21.
             (tests: 455 green 양쪽 feature·디버그/릴리스, clippy 0, 위키 77블록)
