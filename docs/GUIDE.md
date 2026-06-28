# Bang 언어 사용 가이드

> 처음이라면 **[TUTORIAL.md](TUTORIAL.md)** 를 먼저 보세요 (최신 기능 포함, 따라 하기 쉬움).
> 이 가이드는 초기 버전 기준이라 일부 최신 기능(에러 처리·타입 힌트·JSON·정규식·패키지)은
> 튜토리얼과 [SPEC.md](SPEC.md), [../README.md](../README.md)를 참고하세요.

> Bang은 동적 타입 범용 프로그래밍 언어입니다. **투명 동시성(Transparent Concurrency)** 을 핵심으로, async/await 없이 자연스럽게 병렬 코드를 작성할 수 있습니다.

---

## 목차

1. [설치 및 빌드](#1-설치-및-빌드)
2. [CLI 명령어](#2-cli-명령어)
3. [기본 문법](#3-기본-문법)
4. [타입과 값](#4-타입과-값)
5. [연산자](#5-연산자)
6. [제어 흐름](#6-제어-흐름)
7. [함수](#7-함수)
8. [컬렉션 — 리스트와 맵](#8-컬렉션--리스트와-맵)
9. [동시성](#9-동시성)
10. [내장 함수](#10-내장-함수)
11. [AOT 컴파일](#11-aot-컴파일)
12. [예제 모음](#12-예제-모음)

---

## 1. 설치 및 빌드

### 요구사항

- Rust 1.82 이상 ([rustup.rs](https://rustup.rs) 설치)
- MSVC 링커 (Windows): Visual Studio Build Tools 설치 후 **C++ 빌드 도구** 선택
- AOT 컴파일 시: GCC 또는 Clang (`cc` 명령)

### 빌드

```powershell
cd C:\Users\사용자\bang
cargo build --release
```

실행 파일 위치: `target\release\bang.exe`

### PATH 등록 (선택)

```powershell
# 현재 세션
$env:PATH += ";C:\Users\사용자\bang\target\release"

# 영구 등록
[Environment]::SetEnvironmentVariable("PATH", $env:PATH, "User")
```

등록 후 `bang` 명령을 어디서나 사용할 수 있습니다.

---

## 2. CLI 명령어

```
bang run     <파일.bang>          파일 실행 (기본: 바이트코드 VM)
bang run     --interp <파일>      트리워킹 인터프리터로 실행
bang run     --dump-ast <파일>    AST 구조 출력 (디버그)
bang compile -o <출력> <파일>     네이티브 바이너리 AOT 컴파일
bang check   <파일>               문법·의미 오류 검사 (실행 없음)
bang build   <파일>               바이트코드 통계 출력
bang repl                         대화형 셸(REPL) 시작
bang parse   <파일>               AST 덤프
bang tokenize <파일>              토큰 목록 출력 (디버그)
```

### 예시

```powershell
# 파일 실행
.\target\release\bang.exe run examples\fibonacci.bang

# REPL 시작
.\target\release\bang.exe repl

# 오류만 확인
.\target\release\bang.exe check examples\hello.bang

# 네이티브 바이너리 생성 (cc 필요)
.\target\release\bang.exe compile -o fib.exe examples\fibonacci.bang
.\fib.exe
```

---

## 3. 기본 문법

### 세미콜론 없음

문장은 **줄바꿈**으로 끝납니다. 세미콜론은 쓰지 않습니다.

```bang
let x = 10
let y = 20
print(x + y)
```

### 주석

```bang
// 한 줄 주석
```

### 변수 선언

```bang
let name = "Bang"
let count = 0
let pi = 3.14
let flag = true
let nothing = nil
```

변수는 `let`으로 선언하고, 이후 재대입은 `=`으로 합니다.

```bang
let x = 1
x = 2       // 재대입 (let 없이)
```

---

## 4. 타입과 값

| 타입 | 리터럴 예시 | 설명 |
|------|------------|------|
| `Int` | `42`, `-7`, `0` | 64비트 정수 |
| `Float` | `3.14`, `-0.5` | 64비트 부동소수점 |
| `Bool` | `true`, `false` | 논리값 |
| `Nil` | `nil` | 값 없음 |
| `String` | `"hello"` | UTF-8 문자열 |
| `List` | `[1, 2, 3]` | 동적 배열 (값 의미론) |
| `Map` | `{"key": val}` | 문자열 키 사전 (값 의미론) |
| `Function` | `fn(x) { ... }` | 일급 함수 (참조 의미론) |

### 값 의미론

리스트·맵·문자열은 **값으로 복사**됩니다. 함수·채널은 참조로 공유됩니다.

```bang
let a = [1, 2, 3]
let b = a       // b는 a의 복사본
b = b + [4]     // a는 변하지 않음
print(a)        // [1, 2, 3]
print(b)        // [1, 2, 3, 4]
```

---

## 5. 연산자

### 산술

```bang
10 + 3    // 13
10 - 3    // 7
10 * 3    // 30
10 / 3    // 3  (정수 나눗셈)
10.0 / 3  // 3.3333...
10 % 3    // 1  (나머지)
-x        // 단항 부정
```

### 비교

```bang
a == b    // 같음
a != b    // 다름
a < b
a <= b
a > b
a >= b
```

### 논리

```bang
a and b   // 단축 평가, a가 거짓이면 a 반환
a or b    // 단축 평가, a가 참이면 a 반환
not a     // 논리 부정
```

### 문자열 연결

```bang
"hello" + " " + "world"   // "hello world"
"n=" + str(42)            // "n=42"
```

### 리스트 연결

```bang
[1, 2] + [3, 4]   // [1, 2, 3, 4]
```

---

## 6. 제어 흐름

### if / else

```bang
if x > 0 {
    print("양수")
} else if x < 0 {
    print("음수")
} else {
    print("영")
}
```

`if`는 식으로도 사용 가능합니다 (블록의 마지막 값).

### while

```bang
let i = 0
while i < 5 {
    print(i)
    i = i + 1
}
```

### for-in

```bang
let fruits = ["apple", "banana", "cherry"]
for fruit in fruits {
    print(fruit)
}

// 맵 순회 (키 목록 활용)
let person = {"name": "Alice", "age": 30}
let keys = ["name", "age"]
for k in keys {
    print(k + ": " + str(person[k]))
}
```

### break / continue

```bang
let i = 0
while i < 10 {
    if i == 5 { break }
    if i % 2 == 0 { i = i + 1; continue }
    print(i)
    i = i + 1
}
```

---

## 7. 함수

### 함수 선언

```bang
fn add(a, b) {
    return a + b
}

print(add(3, 4))   // 7
```

`fn 이름(...)` 문법은 `let 이름 = fn(...) { ... }` 의 문법 설탕입니다. 이름이 본문 안에서 보이므로 재귀가 가능합니다.

### 익명 함수 (람다)

```bang
let double = fn(x) { return x * 2 }
print(double(5))   // 10
```

### 일급 함수 / 고차 함수

```bang
fn apply(f, x) {
    return f(x)
}

fn square(x) { return x * x }

print(apply(square, 4))   // 16
```

### 클로저

함수는 외부 변수를 캡처합니다.

```bang
fn make_adder(x) {
    return fn(y) { return x + y }
}

let add10 = make_adder(10)
print(add10(5))    // 15
print(add10(20))   // 30
```

### 재귀

```bang
fn fib(n) {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}

print(fib(10))   // 55
```

---

## 8. 컬렉션 — 리스트와 맵

### 리스트

```bang
let nums = [1, 2, 3, 4, 5]

// 인덱스 접근 (0-based)
print(nums[0])    // 1
print(nums[4])    // 5

// 길이
print(len(nums))  // 5

// 연결로 추가
nums = nums + [6]
print(nums)       // [1, 2, 3, 4, 5, 6]

// 문자열처럼 인덱스로 문자 접근
let s = "hello"
print(s[0])       // "h"
```

### 맵

```bang
let person = {
    "name": "Alice",
    "age": 30,
    "city": "Seoul"
}

// 접근
print(person["name"])    // Alice
print(person.age)        // 30  (점 표기법도 가능)

// 수정
person["age"] = 31

// 새 키 추가
person["email"] = "alice@example.com"
```

---

## 9. 동시성

Bang은 **투명 동시성** 모델을 채택합니다. `async`/`await` 없이 `spawn` 한 줄로 병렬 실행이 가능합니다.

### spawn — 비동기 작업

`spawn`은 식을 새 작업으로 실행하고 **Future**를 즉시 반환합니다.

```bang
fn slow_add(x, y) {
    return x + y
}

let f1 = spawn slow_add(10, 20)
let f2 = spawn slow_add(30, 40)

// Future가 피연산자로 사용될 때 자동으로 완료를 기다림
print(f1)        // 30
print(f2)        // 70
print(f1 + f2)   // 100
```

`await` 키워드는 없습니다. Future가 값으로 필요한 시점에 자동으로 조인됩니다.

### parallel 블록

여러 작업을 병렬로 실행하고 블록이 끝날 때 모두 완료를 기다립니다.

```bang
let result_a = nil
let result_b = nil

parallel {
    result_a = spawn fetch(1)
    result_b = spawn fetch(2)
}

// parallel 블록 이후 result_a, result_b 모두 완료됨
print(result_a)
print(result_b)
```

### channel — 작업 간 통신

```bang
fn producer(ch) {
    send(ch, 42)
    close(ch)
}

fn consumer(ch) {
    let v = recv(ch)   // 값이 올 때까지 대기
    print(v)           // 42
}

let ch = channel()
spawn producer(ch)
consumer(ch)
```

채널 관련 내장 함수:

| 함수 | 설명 |
|------|------|
| `channel()` | 새 채널 생성 |
| `send(ch, val)` | 채널에 값 전송 |
| `recv(ch)` | 채널에서 값 수신 (블로킹) |
| `close(ch)` | 채널 닫기 |
| `wait(future)` | Future 명시적 대기 |

---

## 10. 내장 함수

### 출력

| 함수 | 설명 | 예시 |
|------|------|------|
| `print(...)` | 값 출력 (개행 포함, 여러 인자 공백 구분) | `print("hi")`, `print(1, 2, 3)` |

### 타입 변환

| 함수 | 설명 | 예시 |
|------|------|------|
| `str(x)` | 문자열로 변환 | `str(42)` → `"42"` |
| `int(x)` | 정수로 변환 | `int("42")` → `42`, `int(3.9)` → `3` |
| `float(x)` | 실수로 변환 | `float("3.14")` → `3.14` |
| `bool(x)` | 논리값으로 변환 | `bool(0)` → `false` |
| `type(x)` | 타입 이름 반환 | `type(42)` → `"Int"` |

### 컬렉션

| 함수 | 설명 |
|------|------|
| `len(x)` | 리스트·맵·문자열 길이 |
| `keys(m)` | 맵의 키 리스트 반환 |
| `values(m)` | 맵의 값 리스트 반환 |
| `push(list, val)` | 리스트 끝에 값 추가 |
| `pop(list)` | 리스트 마지막 값 제거·반환 |
| `contains(list, val)` | 포함 여부 확인 |

### 수학

| 함수 | 설명 |
|------|------|
| `abs(x)` | 절댓값 |
| `sqrt(x)` | 제곱근 |
| `floor(x)` | 내림 |
| `ceil(x)` | 올림 |
| `round(x)` | 반올림 |
| `pow(x, y)` | 거듭제곱 |
| `min(a, b)` | 최솟값 |
| `max(a, b)` | 최댓값 |

### 기타

| 함수 | 설명 |
|------|------|
| `assert(cond)` | 조건이 거짓이면 오류 종료 |
| `exit(code)` | 프로그램 종료 |

---

## 11. AOT 컴파일

`bang compile`은 소스를 C11로 변환한 뒤 `cc -O2`로 네이티브 바이너리를 생성합니다.

```powershell
bang compile -o fib.exe examples\fibonacci.bang
.\fib.exe
```

### AOT 지원 범위

| 항목 | 지원 |
|------|:----:|
| Int / Float / Bool / Nil / String | ✅ |
| 산술·비교·논리 연산자 | ✅ |
| let, 대입, if/else, while | ✅ |
| return, break, continue | ✅ |
| 최상위 fn 선언 (재귀 포함) | ✅ |
| List / Map | ❌ |
| for-in | ❌ |
| spawn / parallel / channel | ❌ |
| 클로저 / 익명 함수 | ❌ |

AOT 미지원 기능을 사용하면 `bang compile` 시 오류 메시지를 출력합니다. 동시성이 필요하면 `bang run`(VM 모드)을 사용하세요.

---

## 12. 예제 모음

### Hello World

```bang
print("hello world")
```

### 피보나치

```bang
fn fib(n) {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}

let i = 0
while i < 10 {
    print(fib(i))
    i = i + 1
}
```

### 클로저 카운터

```bang
fn make_counter() {
    let count = 0
    return fn() {
        count = count + 1
        return count
    }
}

let c = make_counter()
print(c())   // 1
print(c())   // 2
print(c())   // 3
```

### 리스트 처리

```bang
let numbers = [1, 2, 3, 4, 5]
let total = 0
for n in numbers {
    total = total + n
}
print("합계: " + str(total))   // 합계: 15
```

### 병렬 작업

```bang
fn fetch(id) {
    return id * 100
}

let a = nil
let b = nil
let c = nil

parallel {
    a = spawn fetch(1)
    b = spawn fetch(2)
    c = spawn fetch(3)
}

print(a + b + c)   // 600
```

### 채널 통신

```bang
fn producer(ch) {
    let i = 0
    while i < 5 {
        send(ch, i)
        i = i + 1
    }
    close(ch)
}

let ch = channel()
spawn producer(ch)

for v in ch {
    print(v)
}
// 0 1 2 3 4
```

---

## 오류 출력 형식

Bang은 오류 발생 위치를 소스 컨텍스트와 함께 출력합니다.

```
[3:5] 오류: 정의되지 않은 변수 'foo'
  |
3 | print(foo)
  |     ^
```

---

*Bang 언어 소스: [github.com/knoxxr/bang](https://github.com/knoxxr/bang)*
