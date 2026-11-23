# 한계와 FAQ

## 알려진 문제 (v0.46.0)

매뉴얼을 쓰며 실제로 확인한 문제들입니다. 고쳐지면 이 목록에서 빠집니다.

| 문제 | 우회 방법 |
|---|---|
| 스크립트 뒤에 인자를 붙이면(`bang app.bang foo`) 마지막 인자를 스크립트 파일로 착각합니다. | 환경 변수(`env_get`)나 설정 파일, 표준 입력(`input()`)으로 값을 넘기세요. |
| 타입 표기 `fn`·`nil`이 파스 오류를 냅니다(키워드와 충돌). | 함수 값에는 `any`를 쓰거나 표기를 생략하세요. |
| `import`의 상대 경로가 import한 파일이 아니라 **현재 디렉터리** 기준입니다. | 프로젝트 루트에서 실행하고 루트 기준 경로를 쓰거나, `BANG_PATH`를 지정하세요. |
| `bang add`가 `bang.toml`의 `[package]` 섹션을 지웁니다. | 지금은 `[package]`를 읽는 기능이 없어 동작에는 영향이 없습니다. |
| 작업(`spawn`) 안에서 `throw`한 값은 밖으로 나오면서 `{kind: "error", message: "잡히지 않은 예외: …"}`로 바뀝니다. | 작업이 실패를 에러 대신 값(`{"ok": false, "reason": ...}`)으로 돌려주게 하세요. |
| `input()`이 입력 끝(EOF)에서도 `""`를 돌려줘 빈 줄과 구별되지 않습니다. | 끝 표식 줄을 정하거나 `read_file("/dev/stdin")`으로 한 번에 읽으세요(유닉스). |

## 아직 없는 것

| 없는 것 | 대신 |
|---|---|
| 클래스, 상속 | 맵 + 클로저 ([함수와 클로저](함수와-클로저#클래스-대신-맵--클로저)) |
| `async`/`await` | `spawn` — 영원히 추가하지 않습니다 (설계 원칙) |
| 락, 뮤텍스, 공유 메모리 | 채널, 작업의 반환값 — 영원히 추가하지 않습니다 |
| 맵에서 키 삭제 | 필요한 키만 골라 새 맵 |
| 키워드 인자, 가변 인자 | 맵 하나를 인자로 |
| 컴프리헨션, 제너레이터 | `map`/`filter`, 채널 |
| `finally` | `catch`에서 정리 후 다시 `throw` |
| 집합 타입 | 리스트 + `unique`/`union`/`intersect`/`difference` |
| 날짜 계산, 시간대 | `now_ms()`(밀리초) 산술 + `format_time`(UTC) |
| C 확장(FFI) | `proc_run`으로 외부 프로그램 호출 |
| 중앙 패키지 레지스트리 | git 저장소 (`bang add`) |

## 성능

- **순차 실행은 CPython보다 느립니다** (재귀 함수 벤치마크 기준 약 3.9배). 바이트코드 VM이며 JIT가 없습니다.
- **병렬 실행은 코어 수에 비례해 빨라집니다.** GIL이 없어 CPU 작업이 진짜로 동시에 돕니다.
- 그래서 bang이 유리한 곳은 "독립적인 일이 여러 개" 있는 작업입니다: API 여러 개 호출, 파일 여러 개 처리, 요청마다 독립적인 서버.
- 숫자 계산 핫 루프는 [`bang compile`](도구#bang-compile-실험적)로 네이티브로 만들 수 있습니다(지원 범위가 좁음).

## 메모리

값은 참조 카운팅으로 관리하고, 클로저가 만드는 순환 참조는 세대별 순환 수집기가 정리합니다. 오래 살아남은 뒤 버려진 순환 객체는 다음 정리 주기까지 잠시 남아 있을 수 있습니다. 자세한 내용: [SPEC 15장](https://github.com/knoxxr/bang/blob/main/docs/SPEC.md#15-메모리-관리)

## FAQ

### 왜 리스트를 복사하나요? 느리지 않나요?

작업 사이에 같은 리스트를 동시에 바꾸는 일을 **원천적으로 없애기** 위해서입니다. 그래서 bang에는 락이 필요 없고 데이터 경쟁이 일어날 수 없습니다.

실제 복사는 **두 곳에서 공유 중인 값을 바꿀 때만** 일어납니다(copy-on-write). 큰 리스트를 함수에 넘기는 것은 복사 없이 즉시이고, 변수 하나만 쥔 리스트를 `xs[i] = v`나 `xs += [v]`로 바꾸는 것도 제자리에서 일어납니다.

### `spawn`한 작업이 끝나지 않으면 어떻게 되나요?

작업을 시작한 함수(또는 `parallel` 블록)가 끝날 때 기다립니다. 작업이 영원히 돌면 그 함수도 끝나지 않습니다. 잊힌 작업이 백그라운드에 남는 일은 없습니다. 서버처럼 계속 도는 루프는 최상위에서 시작하세요.

### 작업 사이에 상태를 공유하고 싶어요

공유하지 말고 **주고받으세요**. 작업이 결과를 반환하게 하거나(`parallel_map`), 채널로 보내세요. 카운터 같은 공유 상태가 꼭 필요하면 그 상태를 쥔 작업 하나를 두고 채널로 요청을 받게 만드세요.

```bang
fn counter_service(requests, replies) {
    let count = 0
    for req in requests {
        count += req
        send(replies, count)
    }
}

let requests = channel()
let replies = channel()
spawn counter_service(requests, replies)
for n in [1, 2, 3] {
    send(requests, n)
    print(recv(replies))
}
close(requests)
```

출력:

```text
1
3
6
```

### Windows에서 되나요?

됩니다. PowerShell 설치 스크립트를 쓰세요. 경로 함수는 `\` 구분자를 씁니다. `bang compile`에는 MinGW `gcc`나 `clang`이 필요합니다.

### https가 회사 네트워크에서 실패해요

사내 프록시가 자체 인증 기관(CA)으로 TLS를 가로채는 환경입니다. bang은 내장된 Mozilla 루트 인증서만 신뢰하고 검증을 끄는 옵션이 없어, 현재 이런 환경에서는 https를 쓸 수 없습니다.

### LLM(ChatGPT, Claude 등)에게 bang 코드를 쓰게 하려면?

저장소의 [`llms.txt`](https://github.com/knoxxr/bang/blob/main/llms.txt) 한 파일을 대화에 붙여 넣으세요. 문법, 동시성 규칙, 내장 함수 전체, "하지 말 것" 목록이 들어 있습니다.

### 버그를 찾았어요 / 기능을 제안하고 싶어요

[GitHub Issues](https://github.com/knoxxr/bang/issues)에 올려 주세요. 기능 제안은 [설계 원칙(헌법)](https://github.com/knoxxr/bang/blob/main/docs/PHILOSOPHY.md)과 "영구 거부 목록"을 먼저 확인하면 좋습니다 — 예를 들어 `async`/`await`, 락, 작업 사이 공유 가변 참조는 의도적으로 넣지 않습니다.
