# bang for VS Code

[bang 언어](https://github.com/knoxxr/bang)의 VS Code 지원 —
문법 하이라이팅, 스니펫, 실행/검사 명령.

## 기능

- **문법 하이라이팅** — 키워드(`spawn`/`parallel`/`try`/`catch` 등), 133개 빌트인,
  문자열·숫자·연산자·주석
- **스니펫** — `fn`, `for`, `spawn`, `parallel` 등 자주 쓰는 패턴
- **명령** — `Bang: 파일 실행`(에디터 상단 ▶), `Bang: 오류 검사`
- **설정** — `bang.executablePath`(실행 파일 경로), `bang.runMode`(vm/interp)

## 요구사항

`bang` 실행 파일이 PATH에 있거나, `bang.executablePath` 설정으로 경로를 지정.
설치: `brew install knoxxr/tap/bang` (또는 [설치 안내](https://github.com/knoxxr/bang#설치)).

## 설치 (로컬)

마켓플레이스 게시 전에는 로컬 빌드로 설치한다:

```bash
cd editors/vscode-bang
npx @vscode/vsce package        # bang-lang-x.y.z.vsix 생성
code --install-extension bang-lang-0.1.0.vsix
```

## 라이선스

MIT
