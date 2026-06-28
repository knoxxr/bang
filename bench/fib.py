# Python 동등 코드 (순차) — 비교용
def fib(n):
    if n <= 1: return n
    return fib(n-1) + fib(n-2)
total = 0
for _ in range(8):
    total += fib(32)
print(total)
