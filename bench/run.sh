#!/usr/bin/env bash
# 정직한 벤치마크 재현 — 순차 bang vs CPython vs 병렬 bang.
# 사용: bash bench/run.sh
set -e

cd "$(dirname "$0")/.."

BANG=./target/release/bang
if [ ! -x "$BANG" ]; then
    echo "release 빌드가 없습니다. 빌드 중..."
    cargo build --release
fi

echo "== 측정 환경 =="
uname -sm
echo

run() { echo "-- $1 --"; /usr/bin/time -p $2 2>&1 | grep -E "real|^[0-9]"; echo; }

echo "== 1) 순차 성능 (한 코어): bang이 CPython보다 느리다 =="
run "bang 순차 (fib_seq)" "$BANG bench/fib_seq.bang"
if command -v python3 >/dev/null 2>&1; then
    run "CPython (fib.py)" "python3 bench/fib.py"
else
    echo "(python3 없음 — CPython 비교 생략)"; echo
fi

echo "== 2) 병렬 (코어 전체): bang이 스케일한다 =="
run "bang 병렬 (fib_par)" "$BANG bench/fib_par.bang"

echo "== 3) 거친 CPU 작업 =="
run "순차 (loop_seq)" "$BANG bench/loop_seq.bang"
run "병렬 (loop_par)" "$BANG bench/loop_par.bang"

echo "요점: 순차는 CPython보다 몇 배 느리고, 병렬은 코어 수만큼 스케일한다."
echo "Python threading은 GIL로 CPU 바운드를 병렬화하지 못한다."
