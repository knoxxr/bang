# bench — 정직한 벤치마크

> 이 페이지는 bang이 **지는 것부터** 말한다. 신생 언어가 축적할 수 있는 유일한
> 신뢰 자산은 정직이기 때문이다. bang은 "빠른 언어"가 아니다 — "색칠·락 없이
> 병렬을 쉽게 쓰는 언어"다.

측정 환경: **Apple M2, 8코어, macOS**, release 빌드(`cargo build --release`).
자기 머신에서 재현: `bash bench/run.sh`.

## 1. bang이 지는 것 — 순차 성능 (한 코어 정면 대결)

`fib(32)`을 8번 (수백만 회의 함수 호출).

| 실행 | 벽시계 | 비고 |
|------|------:|------|
| **CPython 3** (`fib.py`) | **2.5s** | bang이 진다 |
| bang 순차 (`fib_seq.bang`) | 9.9s | **약 3.9배 느림** |

bang의 트리워킹/바이트코드 실행은 CPython의 성숙한 인터프리터보다 느리다.
**순차 성능이 전부라면 bang은 당신의 언어가 아니다.** bigint 자동 승격도 없다
(오버플로는 정직한 에러).

## 2. bang이 이기는 것 — 코어 전체를 쓰는 단순함

같은 `fib(32)×8`을 8개 작업으로 `spawn`. 코드 변경은 `total = total + fib(32)`를
`let a = spawn fib(32)` 로 바꾼 것뿐 — `await`·스레드 API·락 없이.

| 실행 | 벽시계 | CPU |
|------|------:|----:|
| bang 순차 (`fib_seq`) | 9.9s | 1코어 |
| **bang 병렬 (`fib_par`)** | **2.1s** | ~7코어 |
| CPython `threading` ×8 | ~2.5s+ | **1코어** (GIL) |

- bang 병렬은 자기 순차 대비 **약 4.7배** 빠르다.
- Python `threading`은 **GIL** 때문에 CPU 바운드를 병렬화하지 못한다 — 8스레드를
  띄워도 단일 코어에 묶인다. 진짜 병렬을 하려면 `multiprocessing`(별도 프로세스 +
  pickle 직렬화 비용 + 보일러플레이트)이 필요하다.
- bang은 `spawn`만으로 진짜 병렬이고, 값 의미론 덕에 데이터 레이스가 불가능하다.

**요점**: bang의 성능 단위는 한 코어가 아니라 **머신 전체**다. 한 코어에서는
CPython에 지고, 여덟 코어에서는 이긴다 — 그리고 그 병렬 코드가 순차 코드만큼 단순하다.

### 거친 CPU 작업 (`loop_seq` / `loop_par`)

| 실행 | 벽시계 | CPU |
|------|------:|----:|
| 순차 (`loop_seq`) | ~25s | 1코어 |
| 병렬 (`loop_par`) | ~10.7s | ~5코어 |

→ 약 2.3배. (speedup은 작업 입도·코어 수·경합에 따라 다르다.)

## 3. 코드 대 코드 — I/O 팬아웃

`bench/fanout.py`(Python aiohttp)와 `examples/fanout.bang`을 비교하라. 실행 속도가
아니라 **형태**가 요점이다:

- Python: `pip install aiohttp` + `async def` + `async with` + `await` + `gather`
  + `asyncio.run` — 그리고 이 함수를 부르는 모든 함수가 `async`로 물든다(함수 색칠).
- bang: `let pages = parallel_map(urls, fn(u) { return http_get(u) })` — 4줄, 색칠 없음.

## 재현

```bash
bash bench/run.sh          # 순차 bang vs CPython vs 병렬 bang 을 각각 계측
```

수치는 머신·부하에 따라 다르다. 중요한 것은 절대값이 아니라 **관계**다:
순차는 CPython보다 몇 배 느리고, 병렬은 코어 수만큼 스케일하며, Python은 GIL로
그 스케일링 자체가 막혀 있다.

## 참고 — VM 호출 경로 최적화 이력

초기엔 세밀한 재귀의 병렬 실행이 오히려 느렸다. 두 원인을 해결했다:
1. **호출 프레임 풀링** — 호출마다 지역변수 버퍼를 힙에 새로 할당하던 것을 재사용 풀로.
2. **spawn 시 모듈 전역 완전 격리** — 여러 스레드가 같은 전역 `Mutex`를 다투던 것 제거.
   병렬 fib의 `system` time 116s → 0.04s.
