#!/bin/sh
# wiki/*.md 를 GitHub 위키(knoxxr/bang.wiki.git)에 올린다.
#
# 처음 한 번: GitHub 저장소 → Wiki 탭 → "Create the first page"로 아무 페이지나 저장해야
#             위키 git 저장소가 생긴다(그 전에는 clone이 실패한다).
# 사용법:     sh wiki/tools/publish.sh            (저장소 루트에서, push 권한이 있는 git 인증 필요)
set -eu

REPO_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WIKI_URL=${WIKI_URL:-https://github.com/knoxxr/bang.wiki.git}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

python3 "$REPO_ROOT/wiki/tools/verify_wiki.py" "${BANG:-$REPO_ROOT/target/release/bang}"

git clone --depth 1 "$WIKI_URL" "$TMP/wiki"
# 위키에서 지운 페이지도 반영되도록 기존 .md를 비우고 새로 복사한다.
find "$TMP/wiki" -maxdepth 1 -name '*.md' -delete
cp "$REPO_ROOT"/wiki/*.md "$TMP/wiki/"

cd "$TMP/wiki"
git add -A
if git diff --cached --quiet; then
    echo "위키가 이미 최신입니다."
    exit 0
fi
git commit -m "매뉴얼 갱신 (bang 저장소 wiki/ 에서 동기화)"
git push origin HEAD
echo "게시 완료: https://github.com/knoxxr/bang/wiki"
