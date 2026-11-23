# Python에서 왔다면

Python 손버릇 대부분이 그대로 통합니다. 이 페이지는 **같은 것**, **이름만 다른 것**, **일부러 다르게 만든 것**을 정리합니다.

## 그대로 통하는 것

```bang
let words = "the cat and the hat".split(" ")
let counts = {}
for w in words {
    counts[w] = get(counts, w, 0) + 1
}
for k, v in counts {
    print(f"{k}: {v}")
}

let xs = [5, 3, 8, 1]
xs += [9]
print(xs.sort()[:3], xs[-2:], 3 in xs, "z" not in counts)

fn describe(n, unit = "개") {
    let size = if n > 3 { "많음" } else { "적음" }
    return f"{n}{unit} ({size})"
}
print(describe(5), describe(2, "명"))

let [lo, hi] = [min(xs), max(xs)]
print(lo, hi, 1_000 + 0xff, 7 / 2)
```

출력:

```text
the: 2
cat: 1
and: 1
hat: 1
[1, 3, 5] [1, 9] true true
5개 (많음) 2명 (적음)
1 9 1255 3.5
```

f-문자열과 형식 지정(`{x:.2f}`, `{n:,}`), 슬라이스, `in`/`not in`, `for k, v`, 복합 대입, 기본값 매개변수, 분해 대입, 참거짓 규칙, `/`가 실수인 것, 맵의 삽입 순서 유지 — 모두 Python과 같습니다.

## 문법 대응표

| Python | bang |
|---|---|
| `def f(a, b=2):` | `fn f(a, b = 2) { ... }` |
| 들여쓰기 블록 | `{ }` 블록 (들여쓰기는 `bang fmt`가 정리) |
| `elif` | `else if` |
| `None`, `True`, `False` | `nil`, `true`, `false` |
| `x if c else y` | `if c { x } else { y }` |
| `a // b` | `idiv(a, b)` (`//`는 주석) |
| `a ** b` | `pow(a, b)` |
| `lambda x: x * 2` | `fn(x) { return x * 2 }` |
| `xs.append(v)` | `xs += [v]` |
| `xs.sort()` (제자리) | `xs = xs.sort()` |
| `xs.pop()` | `let v = xs[-1]` 그리고 `xs = xs[:-1]` |
| `d.get(k, 0)` | `get(d, k, 0)` 또는 `d.get(k, 0)` |
| `del d[k]` | 없음 — 필요한 키만 골라 새 맵 ([컬렉션](컬렉션#키-지우기)) |
| `len(s)`, `s.upper()` | 같음 (`len(s)`, `s.upper()`) |
| `isinstance(x, int)` | `type(x) == "Int"` |
| `raise ValueError("...")` | `throw "..."` (아무 값이나) |
| `except ValueError as e:` | `catch e { if e.kind == "value" { ... } }` |
| `with open(p) as f: f.read()` | `read_file(p)` |
| `import json; json.loads(s)` | `json_parse(s)` (import 불필요) |
| `subprocess.run([...])` | `proc_run([...])` |
| `class Foo:` | 맵 + 클로저 ([함수와 클로저](함수와-클로저#클래스-대신-맵--클로저)) |
| `async def` / `await` | 없음 — 아무 함수나 `spawn` |
| `ThreadPoolExecutor().map(f, xs)` | `parallel_map(xs, f)` |
| `queue.Queue()` | `channel()` |
| `if __name__ == "__main__":` | 없음 — 테스트는 `bang test`로 분리 |

## 일부러 다르게 만든 것

### 1. 리스트·맵은 값이다

```bang
let a = [1, 2]
let b = a
b += [3]
print(a)        // Python이면 [1, 2, 3]
```

출력:

```text
[1, 2]
```

대입·인자 전달·반환할 때 복사됩니다(실제 복사는 바꿀 때만 일어나므로 빠릅니다). 그래서 `xs.sort()`·`xs.push(v)`는 새 값을 돌려주고 `xs`는 그대로입니다. 결과를 버리면 실수를 알려 주는 에러가 납니다. 이 규칙이 [동시성](동시성)을 락 없이 안전하게 만드는 근거입니다.

### 2. 없는 키, 오버플로, 타입 섞기는 에러

| 상황 | Python | bang |
|---|---|---|
| `d["없는키"]` | `KeyError` | `key` 에러 (같음) |
| `d.없는키` | `AttributeError` | `key` 에러 |
| 정수 오버플로 | 큰 정수로 자동 확장 | `overflow` 에러 (64비트) |
| `"a" + 1` | `TypeError` | `type` 에러 (같음) |
| `1 < 2 < 3` | 비교 연결 | 에러 — `1 < 2 and 2 < 3` |

### 3. 숫자 세부

| | Python | bang |
|---|---|---|
| `-7 % 3` | `2` | `-1` (부호는 왼쪽을 따름) |
| `round(2.5)` | `2` (은행가 반올림) | `3` (0에서 먼 쪽) |
| `round(2.567)` | `3` (정수) | `3` (정수) |
| `int` 크기 | 무제한 | 64비트 |
| `[0] * 3` | `[0, 0, 0]` | 지원 안 함 — `range(3).map(fn(_) { return 0 })` |
| `"ab" * 3` | `"ababab"` | 지원 안 함 — `repeat("ab", 3)` |

### 4. 함수는 `return`이 있어야 값을 돌려준다

블록의 마지막 식이 자동으로 반환되지 않습니다(`if` 식은 예외). `return`이 없으면 `nil`입니다.

### 5. 한 줄에 문장 하나

`;`가 없으므로 `if x { return 1 } return 2`처럼 한 줄에 두 문장을 쓸 수 없습니다. 줄을 나누세요.

### 6. 맵 리터럴의 따옴표 없는 키는 문자열

`{name: 1}`의 `name`은 변수가 아니라 문자열 `"name"`입니다(JavaScript와 같음).

### 7. 키워드 인자·가변 인자가 없다

`f(x, verbose=True)`나 `*args`, `**kwargs`가 없습니다. 선택 항목이 많으면 맵을 넘기세요: `connect({"host": "a", "port": 1})`.

## 동시성: 가장 큰 차이

```text
# Python — 스레드는 GIL 때문에 CPU 작업이 병렬로 안 돌고,
#          multiprocessing은 pickle이 필요하고, asyncio는 함수를 색칠한다.
from concurrent.futures import ProcessPoolExecutor
with ProcessPoolExecutor() as ex:
    results = list(ex.map(heavy, items))
```

```bang
fn heavy(n) {
    let total = 0
    for i in range(n) {
        total += i * i
    }
    return total
}

let results = parallel_map([10000, 20000, 30000], heavy)   // 진짜 병렬, 피클링 없음
print(results)
```

출력:

```text
[333283335000, 2666466670000, 8999550005000]
```

더 자세한 비교: [docs/WHY.md](https://github.com/knoxxr/bang/blob/main/docs/WHY.md)

## 아직 Python이 나은 것

- 순차 실행 속도 (bang이 CPython보다 약 4배 느림 — [벤치마크](https://github.com/knoxxr/bang/blob/main/bench/README.md))
- 라이브러리 생태계 (bang은 내장 135개 + git 패키지)
- 클래스, 데코레이터, 제너레이터, 컴프리헨션, 키워드 인자

[한계와 FAQ](한계와-FAQ)도 함께 보세요.
