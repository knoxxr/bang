# bang 사용자 매뉴얼

**bang**은 순차적으로 생각하고 쓰면 병렬로 실행되는 동적 타입 범용 프로그래밍 언어입니다.

- `async`/`await`가 없습니다. 어떤 함수든 앞에 `spawn`을 붙이면 병렬로 돌고, 결과가 필요한 순간에 자동으로 기다립니다.
- GIL이 없습니다. CPU 작업이 실제로 여러 코어에서 동시에 돕니다.
- 락이 없습니다. 리스트·맵·문자열은 **값**이라 작업 사이에 공유되지 않으므로 데이터 경쟁이 생길 수 없습니다.
- 문법은 Python과 비슷합니다: f-문자열, 슬라이스, `in`, `for k, v in m`, 메서드 문법, 기본값 매개변수.

```bang
fn fetch(id) {
    sleep(100)              // 느린 작업이라고 치자
    return id * 10
}

let results = parallel_map([1, 2, 3, 4], fetch)   // 4개가 동시에 실행 (총 0.1초)
print(results)
```

출력:

```text
[10, 20, 30, 40]
```

이 매뉴얼은 bang **v0.47.0** 기준입니다. 모든 예제는 실제로 실행해 출력을 확인했습니다.

## 차례

| 페이지 | 내용 |
|---|---|
| [설치와 첫 실행](설치와-첫-실행) | 설치, 파일 실행, REPL, 새 프로젝트 |
| [기본 문법](기본-문법) | 값과 변수, 연산자, 문자열, 제어 흐름 |
| [컬렉션](컬렉션) | 리스트, 맵, 슬라이스, 값 의미론 |
| [함수와 클로저](함수와-클로저) | 함수, 기본값, 클로저, 메서드 문법, "객체" 만들기 |
| [동시성](동시성) | `spawn`, 자동 대기, `parallel`, `parallel_map`, 채널, `select` |
| [에러 처리](에러-처리) | `try`/`catch`/`throw`, 에러 종류, 에러 메시지 읽기 |
| [타입 힌트](타입-힌트) | 선택적 타입 표기와 `bang check` |
| [모듈과 패키지](모듈과-패키지) | `import`, `bang add`, `bang install` |
| [표준 라이브러리](표준-라이브러리) | 내장 함수 135개 분야별 정리 |
| [네트워킹](네트워킹) | HTTP 클라이언트, TCP 서버, 이벤트 루프 서버 |
| [도구](도구) | `run` `repl` `check` `test` `fmt` `compile` 등 명령어 |
| [Python에서 왔다면](Python에서-왔다면) | Python과 같은 점, 다른 점 |
| [한계와 FAQ](한계와-FAQ) | 아직 없는 것, 자주 묻는 질문 |

## 더 깊이

- 언어 명세: [docs/SPEC.md](https://github.com/knoxxr/bang/blob/main/docs/SPEC.md)
- 설계 원칙(헌법): [docs/PHILOSOPHY.md](https://github.com/knoxxr/bang/blob/main/docs/PHILOSOPHY.md)
- 예제 모음: [examples/](https://github.com/knoxxr/bang/tree/main/examples) — 각 파일 맨 위 주석이 기대 출력입니다.
- LLM에게 bang을 가르칠 때: [llms.txt](https://github.com/knoxxr/bang/blob/main/llms.txt) 한 파일을 컨텍스트로 주면 됩니다.
