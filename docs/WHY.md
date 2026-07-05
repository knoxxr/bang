# 왜 bang인가 — Python 동시성에 지친 사람에게

Python으로 CPU 바운드 작업이나 동시 I/O를 해 본 적 있다면, 셋 중 하나는 겪었을 것이다:
`threading`은 GIL 때문에 안 빨라지고, `multiprocessing`은 pickle 지옥이며,
`asyncio`는 함수를 async로 물들인다. bang은 이 셋을 **하나의 방법**으로 없앤다 — `spawn`.

> 정직하게: bang의 **순차** 실행은 CPython보다 3~4배 느리다([벤치마크](../bench/README.md)).
> bang이 이기는 건 한 코어가 아니라 **머신 전체**, 그리고 **코드의 단순함**이다.

---

## 고통 1 — threading: 8스레드를 띄워도 1코어

```python
# Python — GIL이 CPU 바운드 병렬을 막는다
import threading
results = [0] * 8
def work(i): results[i] = sum(x % 7 for x in range(5_000_000))
ts = [threading.Thread(target=work, args=(i,)) for i in range(8)]
for t in ts: t.start()
for t in ts: t.join()          # 8스레드가 GIL 때문에 사실상 순차
```

```
// bang — spawn 은 진짜 병렬 (GIL 없음)
let results = parallel_map(range(8), fn(i) {
    return sum(map(range(5000000), fn(x) { return x % 7 }))
})
```

Python 스레드는 공유 메모리(`results`)에 직접 쓴다 — 여기선 인덱스가 겹치지 않아
운 좋게 안전하지만, 조금만 복잡해지면 락이 필요하다. bang은 `results`가 값으로
반환되므로 공유 자체가 없다 — **데이터 레이스가 불가능**하다.

## 고통 2 — multiprocessing: pickle 지옥과 보일러플레이트

```python
# Python — 진짜 병렬을 하려면 별도 프로세스 + 직렬화
from multiprocessing import Pool
def work(n): return sum(x % 7 for x in range(n))
if __name__ == "__main__":           # 이 가드가 없으면 Windows에서 폭발
    with Pool(8) as p:
        results = p.map(work, [5_000_000] * 8)
    # 주고받는 모든 값이 pickle 가능해야 함(람다·소켓·열린 파일 불가)
```

```
// bang — 별도 프로세스도, 직렬화도, __main__ 가드도 없음
let inputs = map(range(8), fn(i) { return 5000000 })
let results = parallel_map(inputs, fn(n) {
    return sum(map(range(n), fn(x) { return x % 7 }))
})
```

`multiprocessing`은 함수와 인자를 pickle로 직렬화해 다른 프로세스로 보낸다 —
람다를 못 쓰고, 클로저를 못 넘기고, `if __name__ == "__main__"` 가드를 잊으면
Windows에서 무한 프로세스 폭탄이 된다. bang의 `spawn`은 그냥 작업일 뿐이다.

## 고통 3 — asyncio: 함수 색칠(function coloring)

```python
# Python — 동시 I/O 는 함수를 async 로 물들인다
import asyncio, aiohttp                        # pip install aiohttp

async def fetch(session, url):                 # async 색칠 1
    async with session.get(url) as r:
        return r.status

async def main():                              # async 색칠 2 (fetch 를 부르니 전염)
    async with aiohttp.ClientSession() as s:
        return await asyncio.gather(*[fetch(s, u) for u in urls])

codes = asyncio.run(main())                    # 동기/비동기 경계 의식
```

```
// bang — 색칠 없음. 그냥 함수다.
let codes = parallel_map(urls, fn(u) { return http_get(u).status })
```

이것이 bang의 핵심이다. Python에서 `await`를 한 번 쓰면 그 함수는 `async`가 되고,
그걸 부르는 함수도 `async`가 되어야 하고... 결국 프로그램이 두 세계(sync/async)로
쪼개진다. 라이브러리도 `requests`(sync)와 `aiohttp`(async)로 나뉜다. bang에는
색이 하나뿐이다 — 모든 함수가 그냥 함수고, `spawn`/`parallel_map`으로 병렬이 된다.

## 요약

| | Python threading | Python multiprocessing | Python asyncio | **bang** |
|---|---|---|---|---|
| CPU 병렬 | ❌ GIL | ✅ (무거움) | ❌ | ✅ `spawn` |
| I/O 동시성 | 스레드 | 프로세스 | ✅ (색칠) | ✅ `spawn` |
| 함수 색칠 | 없음 | 없음 | **있음** | **없음** |
| 데이터 레이스 | 가능(락 필요) | 프로세스 격리 | N/A | **불가능**(값 의미론) |
| 직렬화 제약 | 없음 | **pickle** | 없음 | 없음 |
| 추가 설치 | — | — | aiohttp 등 | — (자급) |

**bang이 당신의 언어인 경우**: 8코어를 다 쓰고 싶은데 GIL·색칠·pickle이 지겨울 때,
스크립트~중간 규모 서비스에서 동시성 코드를 순차 코드만큼 쉽게 쓰고 싶을 때.

**bang이 당신의 언어가 아닌 경우**: 순차 실행 속도가 전부일 때(CPython이 더 빠르다),
거대한 기존 생태계(numpy/pandas 등)가 필요할 때, 대규모 프로덕션 팀의 안정성이 우선일 때.

→ 더 배우기: [튜토리얼](TUTORIAL.md) · [철학](PHILOSOPHY.md) · [벤치마크](../bench/README.md)
