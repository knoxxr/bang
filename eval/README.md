# LLM 정답률 측정 (eval)

훈련 데이터에 bang 코드가 없는 LLM이 **`llms.txt` 하나만 읽고** bang 코드를 얼마나 맞게 쓰는지 잽니다.
AI가 코드를 쓰는 시대에 새 언어가 쓰이려면 이 숫자가 높아야 합니다. `llms.txt`, 에러 메시지, 힌트를 고칠 때마다
이 숫자로 효과를 확인합니다.

## 무엇을 재나

- **과제** `eval/tasks/<id>/`: `task.md`(문제) · `reference.bang`(정답) · `solution_test.bang`(채점, 모델에게 보여 주지 않음).
  32개 — 기본 문법, 컬렉션, 값 의미론(`xs.append` 같은 Python 습관이 함정), 클로저 객체, f-문자열 형식, JSON·정규식·CSV·바이트,
  동시성(`parallel_map`·`spawn`·채널·`select`), 에러 처리(`kind`·throw 값), 타입 힌트, 오버플로.
- **첫 시도 통과율 (pass@1)**: 과제 문장 + `llms.txt`만 주고 받은 첫 답이 테스트를 통과한 비율.
- **수리 1회 후 통과율**: 실패하면 테스트 출력과 `bang check --json` 진단(다른 언어 습관 힌트 포함)을 돌려주고 한 번 더 받은 뒤의 통과율.
  AI 에이전트가 실제로 쓰는 "실행 → 오류 읽기 → 고치기" 흐름에서 bang의 진단이 얼마나 도움이 되는지를 보여 줍니다.
- 생성된 코드는 **권한 모드(`bang test --sandbox`)**로 채점합니다 — 파일·네트워크·프로세스 접근이 막힌 채로 실행됩니다.
  coreutils `timeout`이 있으면 과제마다 60초 상한도 겁니다.

## 실행

```sh
# 1) API 없이 과제·채점 검증 (cargo test가 CI에서 자동 실행)
bang eval/run.bang --reference      # 모든 정답이 통과해야 함
bang eval/run.bang --empty          # 빈 답은 모두 실패해야 함 (채점이 실제로 검사하는지)

# 2) 실제 측정 — Anthropic API 키 필요, 비용 발생
ANTHROPIC_API_KEY=sk-... bang eval/run.bang
ANTHROPIC_API_KEY=sk-... bang eval/run.bang --model claude-sonnet-5-5 --effort low
ANTHROPIC_API_KEY=sk-... bang eval/run.bang --tasks 06,14,27 --no-repair   # 일부만
```

옵션: `--model`(기본 `claude-opus-5-5`) `--effort`(기본 `medium`) `--tasks` `--no-repair` `--jobs N`(동시 과제 수, 기본 8)
`--out DIR`(기본 `eval/out`, git에서 제외됨). `BANG_BIN`으로 채점에 쓸 bang 바이너리를 정할 수 있습니다.

출력 예 (가짜 API로 돌린 모양 — 실제 숫자 아님):

```text
과제 32개 · 모드 api · 모델 claude-opus-5-5 (effort medium)
  01_sum_evens         통과
  06_squares           수리 후 통과
  12_emails            실패
  ...
첫 시도 통과 (pass@1): 30/32 (93.8%)
수리 1회 후 통과:      31/32 (96.9%)
첫 시도 실패: 06_squares, 12_emails
토큰: 입력 4,080 (+ 캐시 읽기 136,000) · 출력 2,040
결과: eval/out/results.json
```

과제별 생성 코드는 `eval/out/<id>/1/solution.bang`(첫 시도), `.../2/`(수리)에 남습니다 — 어디서 틀렸는지 읽고
`llms.txt`나 힌트를 고칠 근거로 씁니다.

## 비용

한 번 전체 실행(32과제, 수리 포함)의 대략적인 추정 — `claude-opus-5-5`($4 / $20 per 1M 토큰, 캐시 읽기 $0.20) 기준:

- 시스템 프롬프트(`llms.txt`, 약 5천 토큰)는 프롬프트 캐시로 두 번째 요청부터 캐시 읽기 가격.
- 과제당 입력 수백 토큰, 출력(생각 포함) 1~2천 토큰 가정 → **1회 실행 약 1~3달러**. 출력 길이는 effort와 과제에 따라 크게 달라지므로,
  처음에는 `--tasks`로 몇 개만 돌려 토큰 합계를 보고 전체 비용을 가늠하세요.

## 과제 추가

`eval/tasks/NN_이름/`에 세 파일을 만들고 `bang eval/run.bang --reference --tasks NN`과 `--empty --tasks NN`으로 확인합니다.
`solution_test.bang`은 `let s = import("solution.bang")`로 시작하고 `fn test_...()` 안에서 `assert_eq`로 검사합니다.
과제 문장에는 함수 이름·인자·반환 형태를 분명히 적고, 다른 언어 습관이 함정이 될 만한 지점을 일부러 넣습니다.
