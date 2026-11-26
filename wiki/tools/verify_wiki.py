#!/usr/bin/env python3
"""위키 문서의 bang 코드 블록을 실제로 실행해 적힌 출력과 비교한다.

규칙 (페이지마다 임시 디렉터리 하나를 쓰고, 블록을 위에서 아래로 처리):
  ```bang                 실행한다. exit 0이어야 한다.
  <!-- norun -->          바로 다음 bang 블록은 실행하지 않는다 (서버·네트워크 등).
  <!-- check -->          바로 다음 bang 블록은 `bang check`만 통과하면 된다.
  <!-- file: 이름 -->     바로 다음 bang 블록을 실행하지 않고 그 이름의 파일로 저장한다 (모듈 예제).
  bang 블록 뒤에 `출력:` 줄 + ```text 블록이 오면 stdout이 그 내용과 같아야 한다.
  bang 블록 뒤에 `에러 출력:` 줄 + ```text 블록이 오면 stderr가 같아야 하고 exit 코드는 무관하다.

사용법: python3 wiki/tools/verify_wiki.py [bang 바이너리 경로]
"""
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BANG = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(
    os.path.dirname(ROOT), "target", "release", "bang")

FENCE = re.compile(r"^```(\w*)\s*$")
MARK = re.compile(r"^<!--\s*(norun|check|file:\s*(\S+))\s*-->\s*$")


def blocks(lines):
    """(언어, 내용, 시작 줄 번호, 직전 표식, 직전 라벨) 목록."""
    out = []
    i = 0
    pending_mark = None
    last_label = None
    while i < len(lines):
        line = lines[i]
        m = MARK.match(line)
        if m:
            pending_mark = m.group(1)
            i += 1
            continue
        f = FENCE.match(line)
        if f:
            lang = f.group(1)
            start = i + 1
            body = []
            i += 1
            while i < len(lines) and not lines[i].startswith("```"):
                body.append(lines[i])
                i += 1
            out.append((lang, "\n".join(body) + "\n", start, pending_mark, last_label))
            pending_mark = None
            last_label = None
            i += 1
            continue
        s = line.strip()
        if s in ("출력:", "에러 출력:"):
            last_label = s
        elif s:
            last_label = None
            pending_mark = None if not MARK.match(line) else pending_mark
        i += 1
    return out


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=60)


def anchor(heading):
    """GitHub 방식 앵커: 소문자, 글자·숫자·공백·-·_만 남기고 공백은 -로."""
    h = heading.strip().lower()
    h = "".join(ch for ch in h if ch.isalnum() or ch in " -_")
    return h.replace(" ", "-")


def check_links(pages):
    """[글](페이지#앵커) 형태의 위키 내부 링크가 실제 페이지·제목을 가리키는지."""
    anchors = {}
    for page in pages:
        with open(os.path.join(ROOT, page), encoding="utf-8") as fh:
            text = fh.read()
        in_code = False
        heads = set()
        for line in text.split("\n"):
            if line.startswith("```"):
                in_code = not in_code
            elif not in_code and line.startswith("#"):
                heads.add(anchor(line.lstrip("#")))
        anchors[page[:-3]] = heads
    bad = 0
    for page in pages:
        with open(os.path.join(ROOT, page), encoding="utf-8") as fh:
            text = fh.read()
        for target in re.findall(r"\]\(([^)\s]+)\)", text):
            if "://" in target:
                continue
            name, _, frag = target.partition("#")
            name = name or page[:-3]
            if name not in anchors:
                bad += 1
                print(f"BROKEN LINK {page}: {target} (페이지 없음)")
            elif frag and frag not in anchors[name]:
                bad += 1
                print(f"BROKEN LINK {page}: {target} (제목 없음)")
    return bad


def main():
    pages = sorted(p for p in os.listdir(ROOT) if p.endswith(".md"))
    failures = check_links(pages)
    ran = 0
    for page in pages:
        with open(os.path.join(ROOT, page), encoding="utf-8") as fh:
            lines = fh.read().split("\n")
        bl = blocks(lines)
        with tempfile.TemporaryDirectory() as tmp:
            k = 0
            while k < len(bl):
                lang, body, start, mark, _ = bl[k]
                expect = None
                if k + 1 < len(bl) and bl[k + 1][0] == "text" and bl[k + 1][4]:
                    expect = (bl[k + 1][4], bl[k + 1][1])
                if lang != "bang" or mark == "norun":
                    k += 1
                    continue
                if mark and mark.startswith("file:"):
                    name = mark.split(":", 1)[1].strip()
                    os.makedirs(os.path.dirname(os.path.join(tmp, name)) or tmp, exist_ok=True)
                    with open(os.path.join(tmp, name), "w", encoding="utf-8") as fh:
                        fh.write(body)
                    k += 1
                    continue
                src = os.path.join(tmp, "snippet.bang")
                with open(src, "w", encoding="utf-8") as fh:
                    fh.write(body)
                where = f"{page}:{start}"
                ran += 1
                if mark == "check":
                    r = run([BANG, "check", "snippet.bang"], tmp)
                    if r.returncode != 0:
                        failures += 1
                        print(f"FAIL(check) {where}\n{r.stdout}{r.stderr}")
                    k += 1
                    continue
                r = run([BANG, "run", "snippet.bang"], tmp)
                if expect and expect[0] == "에러 출력:":
                    if r.stderr != expect[1]:
                        failures += 1
                        print(f"FAIL(stderr) {where}\n--- 기대\n{expect[1]}--- 실제\n{r.stderr}")
                    k += 2
                    continue
                if r.returncode != 0:
                    failures += 1
                    print(f"FAIL(exit {r.returncode}) {where}\n{r.stdout}{r.stderr}")
                elif expect and r.stdout != expect[1]:
                    failures += 1
                    print(f"FAIL(stdout) {where}\n--- 기대\n{expect[1]}--- 실제\n{r.stdout}")
                k += 2 if expect else 1
    print(f"{ran}개 블록 실행, 실패 {failures}개")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
