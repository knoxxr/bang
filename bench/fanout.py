# bang examples/fanout.bang 과 같은 일을 하는 Python 코드 — 비교용.
#
# 요점은 "실행 속도"가 아니라 "코드 형태"다. 여러 URL을 동시에 가져오려면
# Python에서는 aiohttp(외부 설치) + async def + async with + gather + asyncio.run 이
# 필요하고, 이 함수를 부르는 상위 함수도 전부 async 로 색칠된다(function coloring).
#
# bang 등가물(examples/fanout.bang):
#     let pages = parallel_map(urls, fn(url) { return http_get(url) })
# 색칠 없음, 외부 설치 없음, 4줄.
#
# 참고: 표준 라이브러리만 쓰면(requests/urllib) 동시 실행이 안 되어 순차로 느려지고,
# threading 을 쓰면 GIL 때문에 CPU 바운드에서는 병렬 이득이 없다. 진짜 동시 I/O 는
# 아래처럼 asyncio 를 강제한다.

import asyncio
import aiohttp  # pip install aiohttp (표준 라이브러리 아님)

URLS = [
    "http://127.0.0.1:8080/a",
    "http://127.0.0.1:8080/b",
    "http://127.0.0.1:8080/c",
]


async def fetch(session, url):          # async 색칠 1
    async with session.get(url) as resp:
        await resp.text()
        return resp.status


async def main():                       # async 색칠 2 (fetch 를 부르니 전염됨)
    async with aiohttp.ClientSession() as session:
        tasks = [fetch(session, u) for u in URLS]
        codes = await asyncio.gather(*tasks)
    print("가져온 페이지:", len(codes))
    print("상태 코드:", codes)


if __name__ == "__main__":
    asyncio.run(main())                 # 동기 세계와 비동기 세계의 경계 의식
