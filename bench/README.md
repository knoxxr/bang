# bench — 투명 동시성 벤치마크

bang의 핵심 차별점인 **투명 동시성**(async/await 색칠 없음, GIL 없음, 락 없음)이
실제로 병렬 speedup을 내는지 측정한다.

실행:
```bash
cargo build --release
time ./target/release/bang bench/loop_seq.bang   # 순차
time ./target/release/bang bench/loop_par.bang   # 병렬 (spawn × 8)
```

## 결과 (8코어 머신, release 빌드)

CPU 작업 8개 (`work(20_000_000)` 루프 합):

| 실행 | 벽시계 시간 | CPU 사용률 | 비고 |
|------|-----------:|----------:|------|
| 순차 (`loop_seq`) | ~24.1s | 99% (1코어) | 8개를 차례로 |
| 병렬 (`loop_par`) | **~11.0s** | 583% (~6코어) | 8개를 동시에 |

→ 코드 변경은 `total = total + work(n)` 를 `let a = spawn work(n)` 로 바꾼 것뿐.
**약 2.2배 speedup**, `await`·스레드 API·락 없이.

### Python과의 비교

Python `threading`은 **GIL** 때문에 CPU 바운드 작업을 병렬화하지 못한다
(8스레드를 띄워도 순차와 비슷). 같은 일을 하려면 `multiprocessing`(별도 프로세스,
직렬화 비용)이 필요하다. bang은 `spawn`만으로 진짜 병렬 실행이 된다 —
공유 가변 상태가 없어 데이터 레이스도 원천적으로 불가능하다.

## 알려진 한계 (정직한 보고)

`fib(32)` 같은 **세밀한 재귀**(`bench/fib_*.bang`)는 현재 병렬화해도 빨라지지
않는다(오히려 느림). 원인: VM이 함수 호출마다 지역변수 프레임을 힙에 할당
(`Arc<Mutex<Vec>>`)하는데, 수백만 호출 × 다중 스레드에서 메모리 할당자 경합이
지배적이 된다(`system` time 급증).

→ 향후 VM 최적화 과제: 호출 경로의 프레임 할당 제거(평탄한 지역변수 스택).
   이 최적화 전에는 **세밀한 재귀보다 거친 단위(coarse-grained) 작업**을
   병렬화할 때 이득이 크다.
