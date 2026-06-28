# Bang 튜토리얼 — 따라 하며 배우기

Bang은 **투명 동시성(transparent concurrency)** 을 핵심으로 하는 동적 타입
스크립팅 언어입니다. `async`/`await` 색칠도, GIL도, 락도 없이 평범한 함수를
`spawn`만으로 병렬 실행할 수 있습니다.

이 문서는 처음 시작하는 사람이 위에서 아래로 따라 하며 익히도록 구성했습니다.
모든 예제는 실제로 실행해 출력을 확인한 것입니다.

> 더 깊은 레퍼런스: 언어 명세는 [SPEC.md](SPEC.md), 전체 빌트인은 [../README.md](../README.md).

---

## 1. 설치와 첫 실행

```bash
# macOS
brew install knoxxr/tap/bang

# Linux / macOS (스크립트)
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/knoxxr/bang/releases/latest/download/bang-installer.sh | sh

# 소스에서
cargo install --git https://github.com/knoxxr/bang
```

`hello.bang`:

```
print("hello, bang")
```

```bash
bang hello.bang        # 파일 실행 (run 생략 가능)
bang                   # 인자 없이 → REPL
echo 'print(1+2)' | bang -   # 표준 입력
```

shebang을 넣고 실행 권한을 주면 직접 실행도 됩니다:

```
#!/usr/bin/env bang
print("직접 실행")
```

---

## 2. 값과 변수

선언은 `let`, 재대입은 `=` (선언 없이 `=`만 쓰면 오류):

```
let name = "Alice"
let age = 30
age = 31              // 재대입은 let 없이
```

기본 타입: `int`, `float`, `bool`, `str`, `nil`.
컨테이너: `list`, `map`. 함수도 일급 값입니다.

```
print(type(42))       // Int
print(type(3.14))     // Float
print(type("hi"))     // Str
print(type([1,2]))    // List
```

> **세미콜론은 없습니다.** 문장은 줄바꿈으로 끝납니다.
> 한 줄에 여러 문장을 `;`로 쓰는 건 허용되지 않습니다.

**값 의미론**: list/map/string은 전달·대입 시 복제됩니다(관찰상). 원본은 안전합니다.

```
let a = [1, 2, 3]
let b = a
b[0] = 99
print(a)              // [1, 2, 3]  ← 그대로
print(b)              // [99, 2, 3]
```

---

## 3. 연산자와 문자열

```
print(7 + 2 * 3)      // 13
print(10 / 4)         // 2.5   (정수끼리도 나누면 float)
print(10 % 3)         // 1
print("foo" + "bar")  // foobar  (+ 로 문자열 연결)
print(1 < 2 and 3 >= 3)   // true
print(not false)      // true
```

문자열 도구 일부: `len`, `upper`, `lower`, `trim`, `split`, `join`,
`replace`, `contains`, `slice`, `repeat`, `ord`, `chr`.

```
print(upper("hi"))                  // HI
print(split("a,b,c", ","))          // [a, b, c]
print(join(["x", "y"], "-"))        // x-y
print(slice("hello", 0, 3))         // hel
```

---

## 4. 제어 흐름

```
let n = 7
if n % 2 == 0 {
    print("짝수")
} else {
    print("홀수")
}

let i = 0
while i < 3 {
    print(i)
    i = i + 1
}

for x in [10, 20, 30] {
    print(x)
}
```

`break`, `continue`도 있습니다. `{ }`는 문장 자리에선 블록, 식 자리에선 맵 리터럴입니다.

---

## 5. 함수와 클로저

`fn name(...) { ... }` 로 선언하며, 이름이 본문 안에서 보여 **재귀**가 됩니다.

```
fn fib(n) {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}
print(fib(10))        // 55
```

함수는 값이라 인자로 넘기고 반환할 수 있습니다(**고차 함수**):

```
let nums = [1, 2, 3, 4, 5]
let evens = filter(nums, fn(x) { return x % 2 == 0 })
let doubled = map(evens, fn(x) { return x * 2 })
print(reduce(doubled, fn(a, b) { return a + b }, 0))   // 12
```

**클로저**는 바깥 변수를 캡처해 상태를 유지합니다:

```
fn make_counter(start) {
    let count = start
    return {
        "inc": fn() {
            count = count + 1
            return count
        }
    }
}
let c = make_counter(10)
print(c.inc())        // 11
print(c.inc())        // 12
```

---

## 6. 컬렉션 — 리스트와 맵 (= 객체)

Bang에는 클래스가 없습니다. **객체는 맵으로** 표현합니다.

```
let person = {"name": "Alice", "age": 30}
print(person.name)          // 점 접근
print(person["age"])        // 대괄호 접근
person.city = "Seoul"       // 필드 추가
```

리스트 유틸: `push`, `pop`, `sort`, `sort_by`, `reverse`, `map`, `filter`,
`reduce`, `sum`, `unique`, `slice`, `index_of`, `range`, `enumerate`, `zip`.

```
let xs = [3, 1, 2]
print(sort(xs))                     // [1, 2, 3]
print(sort_by(["bbb", "a", "cc"], fn(s) { return len(s) }))   // [a, cc, bbb]
print(unique([1, 1, 2, 3, 2]))      // [1, 2, 3]
```

> 값 의미론 때문에 `push`/`sort`는 원본을 바꾸지 않고 **새 값을 반환**합니다.
> `let ys = push(xs, 9)` 처럼 결과를 받으세요.

맵/집합 도구: `keys`, `values`, `has`, `get`, `merge`, `union`, `intersect`, `difference`.

---

## 7. 동시성 — Bang의 핵심

`spawn <식>` 은 식을 새 작업으로 실행하고 **Future**를 즉시 반환합니다.
그 값이 **필요한 순간 자동으로 조인**됩니다 — `await`이 없습니다.

```
fn work(n) {
    let s = 0
    let i = 0
    while i < n {
        s = s + i
        i = i + 1
    }
    return s
}

let a = spawn work(100)
let b = spawn work(200)
print(a + b)          // 24850  ← a, b 를 더하는 순간 둘 다 자동 조인
```

이 코드는 진짜로 병렬 실행됩니다(멀티코어). Python `threading`은 GIL 때문에
이런 CPU 작업을 병렬화하지 못합니다 — Bang의 차별점입니다.

**`parallel` 블록** 은 안의 모든 `spawn`이 끝날 때까지 블록을 벗어나지 않습니다(구조적 동시성):

```
let ra = nil
let rb = nil
parallel {
    ra = spawn work(100)
    rb = spawn work(200)
}
// 여기서 ra, rb 모두 완료 보장
```

**채널** 로 작업 간 메시지를 안전하게 주고받습니다(공유 가변 상태 없음):

```
fn producer(ch) {
    send(ch, 42)
    close(ch)
}
let ch = channel()
spawn producer(ch)
print(recv(ch))       // 42
```

**`parallel_map`** 은 리스트를 병렬로 처리합니다:

```
let results = parallel_map([1, 2, 3], fn(x) { return x * x })
print(results)        // [1, 4, 9]
```

---

## 8. 에러 처리

런타임 오류는 `try`/`catch`로 잡고, `throw`로 임의의 값을 던집니다.

```
fn safe_div(x, y) {
    try {
        return x / y
    } catch e {
        return -1
    }
}
print(safe_div(10, 2))    // 5
print(safe_div(10, 0))    // -1   (0 나눗셈도 catch로 잡힘)
```

`throw`는 문자열·맵 등 어떤 값이든 던질 수 있고, 호출 스택을 가로질러
가장 가까운 `try`로 전파됩니다:

```
fn check(n) {
    if n < 0 {
        throw {"code": 400, "msg": "음수 불가"}
    }
    return n
}
try {
    check(-1)
} catch e {
    print(e.code)         // 400
}
```

---

## 9. 타입 힌트 (선택적)

변수·파라미터·반환값에 타입을 **선택적으로** 표기할 수 있습니다.
표기하면 런타임에 검증되고, `bang check`로 **실행 전 정적 검사**도 됩니다.

```
let count: int = 42

fn area(w: int, h: int) -> int {
    return w * h
}
print(area(3, 4))         // 12
```

타입: `int float bool str nil list map fn any`. 표기를 생략하면 완전 동적입니다(점진적 타이핑).

```bash
$ bang check app.bang
# 타입 오류: 'x'은 int 인데 str 값이 대입됨   ← 확실한 충돌만 보고
```

`any`는 모든 타입을 허용하고, 정적으로 알 수 없는 동적 값은 통과시킨 뒤
런타임에 검증합니다(거짓 양성 없음).

---

## 10. 모듈과 패키지

`import("경로")` 는 다른 `.bang` 파일을 실행하고 그 최상위 바인딩을 맵으로 줍니다.
모듈은 **한 번만 실행**되고 캐시됩니다(싱글톤).

```
// math.bang
let pi = 3.14159
fn square(x) { return x * x }
```

```
// main.bang
let math = import("math.bang")
print(math.square(5))     // 25
print(math.pi)
```

**패키지**는 git으로 관리합니다(중앙 레지스트리 없음):

```bash
bang add mathutils https://github.com/user/mathutils@v1.0.0
bang install          # bang.toml 의 모든 의존성 설치
```

설치된 패키지는 **바레 이름**으로 import합니다 (`bang_modules/` 에서 해석):

```
let mathutils = import("mathutils")
```

---

## 11. 표준 라이브러리 둘러보기

별도 import 없이 바로 쓰는 빌트인입니다.

**JSON** — 설정·API·데이터 교환:

```
let data = json_parse("{\"name\": \"Al\", \"nums\": [1, 2, 3]}")
print(data.name)                  // Al
print(sum(data.nums))             // 6
print(json_stringify({"ok": true, "n": 5}))   // {"n":5,"ok":true}
```

**정규식** — 매칭·추출·치환 (캡처 그룹 지원):

```
print(regex_match("2023-11-14", "^\\d{4}-\\d{2}-\\d{2}$"))   // true
print(regex_find_all("a1 b22 c333", "\\d+"))                 // [1, 22, 333]
let g = regex_groups("2023-11-14", "(\\d{4})-(\\d{2})-(\\d{2})")
print(g[1])                       // 2023
```

**시간·난수**:

```
print(format_time(0))             // 1970-01-01 00:00:00
let r = random_int(1, 6)          // 주사위
```

**파일시스템**:

```
write_file("out.txt", "내용")
print(read_file("out.txt"))
print(file_exists("out.txt"))
print(list_dir("."))
```

**수학**: `abs sqrt pow floor ceil round min max gcd clamp sign sin cos tan log exp pi() e()`.

---

## 12. 다음 단계

- 예제 모음: [../examples/](../examples/) — 각 파일 상단 주석이 기대 출력입니다.
- 동시성 벤치마크: [../bench/](../bench/) — 순차 vs 병렬 speedup 측정.
- 언어 명세: [SPEC.md](SPEC.md).

작은 스크립트부터 시작해 `spawn`으로 병렬화해 보세요 — 그게 Bang의 진가입니다.
