//! Phase 66: LLM 정답률 측정기(eval/run.bang)와 과제 세트 검증 — API 비용 없이.
//! (1) 정답 모드: 모든 과제의 reference가 채점을 통과 (2) 빈 답 모드: 모든 과제가 실패(채점이 실제로 검사함)
//! (3) 가짜 Messages API로 실행기 전체를 종단 실행: 첫 시도·수리·재시도(429)·요청 형식.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

const BANG: &str = env!("CARGO_BIN_EXE_bang");
const ROOT: &str = env!("CARGO_MANIFEST_DIR");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn out_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bang_eval_{}_{n}", std::process::id()))
}

fn run_eval(args: &[&str], envs: &[(&str, String)]) -> (i32, String, String) {
    let out = out_dir();
    let mut cmd = Command::new(BANG);
    cmd.arg(format!("{ROOT}/eval/run.bang")).args(args).arg("--out").arg(&out).env("BANG_BIN", BANG);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let o = cmd.output().expect("실행 실패");
    let results = std::fs::read_to_string(out.join("results.json")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&out);
    let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(results.starts_with("{\"mode\":"), "results.json 없음\n{stdout}\n{}", String::from_utf8_lossy(&o.stderr));
    (o.status.code().unwrap_or(-1), stdout, String::from_utf8_lossy(&o.stderr).into_owned())
}

fn task_count() -> usize {
    std::fs::read_dir(format!("{ROOT}/eval/tasks")).unwrap().filter(|e| e.as_ref().unwrap().path().is_dir()).count()
}

#[test]
fn references_pass_every_task() {
    let n = task_count();
    assert!(n >= 30, "과제 수: {n}");
    let (code, out, err) = run_eval(&["--reference"], &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains(&format!("첫 시도 통과 (pass@1): {n}/{n} (100.0%)")), "{out}");
}

#[test]
fn empty_answers_fail_every_task() {
    let n = task_count();
    let (code, out, err) = run_eval(&["--empty"], &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains(&format!("첫 시도 통과 (pass@1): 0/{n} (0.0%)")), "{out}");
}

fn json_escape(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c => o.push(c),
        }
    }
    o
}

fn read_request(c: &mut TcpStream) -> Option<(String, String)> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    let head_end = loop {
        if let Some(p) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        let n = c.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&buf[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let len: usize = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
        .unwrap_or(0);
    let mut body = raw[head_end + 4..].to_vec();
    while body.len() < len {
        let n = c.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    Some((head, String::from_utf8_lossy(&body).into_owned()))
}

struct Mock {
    saw_hint_in_repair: AtomicBool,
    saw_bad_request: AtomicBool,
    rate_limited_once: AtomicBool,
}

/// 가짜 Messages API: 과제 ID를 프롬프트에서 읽어 정답(또는 일부러 틀린 답)을 돌려준다.
fn handle(mut c: TcpStream, mock: Arc<Mock>) {
    let Some((head, body)) = read_request(&mut c) else { return };
    let lower = head.to_ascii_lowercase();
    let well_formed = lower.contains("x-api-key: test-key")
        && lower.contains("anthropic-version: 2026-10-21")
        && body.contains("\"model\":\"claude-opus-5-5\"")
        && body.contains("\"cache_control\":{\"type\":\"ephemeral\"}")
        && body.contains("parallel_map"); // 시스템 프롬프트에 llms.txt 본문이 들어 있다
    if !well_formed {
        mock.saw_bad_request.store(true, Ordering::SeqCst);
    }
    let id: String = body
        .split("과제 ID: ")
        .nth(1)
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let repair = body.contains("\"role\":\"assistant\"");
    // 01: 첫 요청에 429 (retry-after 0) → 실행기가 다시 시도해야 함
    if id == "01_sum_evens" && !mock.rate_limited_once.swap(true, Ordering::SeqCst) {
        let b = r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
        let _ = c.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\nContent-Length: {}\r\n\r\n{b}", b.len()).as_bytes());
        return;
    }
    let reference = std::fs::read_to_string(format!("{ROOT}/eval/tasks/{id}/reference.bang")).unwrap_or_default();
    let code = match (id.as_str(), repair) {
        // Python 습관으로 틀린 첫 답 → 수리 요청에 런타임 오류와 힌트가 담겨 와야 함
        ("06_squares", false) => "fn squares(n) {\n    let out = []\n    for i in range(1, n + 1) {\n        out.append(i * i)\n    }\n    return out\n}\n".to_string(),
        ("06_squares", true) => {
            if body.contains("힌트") && body.contains("append") {
                mock.saw_hint_in_repair.store(true, Ordering::SeqCst);
            }
            reference
        }
        // 끝까지 틀리는 과제
        ("12_emails", _) => "fn extract_emails(text) {\n    return []\n}\n".to_string(),
        _ => reference,
    };
    let text = format!("설명은 생략합니다.\n```bang\n{code}```\n");
    let b = format!(
        r#"{{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{{"type":"thinking","thinking":"","signature":"sig"}},{{"type":"text","text":"{}"}}],"stop_reason":"end_turn","usage":{{"input_tokens":120,"cache_read_input_tokens":4000,"output_tokens":60}}}}"#,
        json_escape(&text)
    );
    let _ = c.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{b}", b.len()).as_bytes());
}

#[test]
fn runner_end_to_end_against_mock_api() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let mock = Arc::new(Mock {
        saw_hint_in_repair: AtomicBool::new(false),
        saw_bad_request: AtomicBool::new(false),
        rate_limited_once: AtomicBool::new(false),
    });
    let m2 = mock.clone();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let m = m2.clone();
            std::thread::spawn(move || handle(c, m));
        }
    });
    let n = task_count();
    let (code, out, err) = run_eval(
        &[],
        &[("ANTHROPIC_API_KEY", "test-key".into()), ("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}"))],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains(&format!("첫 시도 통과 (pass@1): {}/{n}", n - 2)), "{out}");
    assert!(out.contains(&format!("수리 1회 후 통과:      {}/{n}", n - 1)), "{out}");
    assert!(out.contains("06_squares           수리 후 통과"), "{out}");
    assert!(out.contains("12_emails            실패"), "{out}");
    assert!(out.contains("01_sum_evens         통과"), "429 재시도 후 통과해야 함\n{out}");
    assert!(out.contains("첫 시도 실패: 06_squares, 12_emails"), "{out}");
    // 토큰 합계: 과제 n개 첫 요청 + 수리 2번 (429 응답은 세지 않음)
    let calls = n + 2;
    assert!(out.contains(&format!("토큰: 입력 {} (+ 캐시 읽기 {}) · 출력 {}", fmt_thousands(120 * calls), fmt_thousands(4000 * calls), fmt_thousands(60 * calls))), "{out}");
    assert!(mock.saw_hint_in_repair.load(Ordering::SeqCst), "수리 요청에 테스트 출력·힌트가 담겨야 함");
    assert!(!mock.saw_bad_request.load(Ordering::SeqCst), "요청 형식(헤더·모델·캐시·llms.txt)이 잘못됨");
}

fn fmt_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
