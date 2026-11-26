//! 문서가 실제 구현과 어긋나지 않게 지키는 테스트 (Phase 63).
//!
//! AI 시대에는 llms.txt가 가장 많이 읽히는 문서다 — 틀리면 LLM이 틀린 bang 코드를 쓴다.
//! 빌트인 테이블(src/stdlib/mod.rs BUILTINS)을 정답으로 삼아 llms.txt·위키·VS Code 문법을 대조하고,
//! llms.txt의 코드 블록이 실제 파서로 파싱되는지 본다. (위키 예제 실행은 wiki/tools/verify_wiki.py — CI에서 실행)

use std::collections::BTreeSet;

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn builtins() -> Vec<&'static str> {
    (0..bang::stdlib::builtin_count()).map(|i| bang::stdlib::builtin_name(i).unwrap()).collect()
}

/// 문서에서 `` `name(`` 형태로 적힌 호출 이름들 (`sin/cos/tan(x)`처럼 /로 묶은 것도 나눈다).
fn called_names(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for seg in text.split('`').skip(1).step_by(2) {
        // 백틱 안 조각: name(...) | a/b/c(...) | x.f(...) 등
        for part in seg.split(['|', ' ']) {
            let Some(head) = part.split('(').next() else { continue };
            if !part.contains('(') || head.contains('.') {
                continue;
            }
            for name in head.split('/') {
                let keyword = matches!(name, "fn" | "if" | "while" | "for" | "return" | "spawn");
                if !keyword && !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

fn section<'a>(text: &'a str, title: &str) -> &'a str {
    let start = text.find(title).unwrap_or_else(|| panic!("'{title}' 절 없음"));
    let rest = &text[start + title.len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn llms_txt_builtin_list_matches_table() {
    let llms = read("llms.txt");
    let names = builtins();
    let header = format!("## 내장 함수 ({}개)", names.len());
    assert!(llms.contains(&header), "llms.txt의 빌트인 개수 표기가 실제({})와 다름 — '{header}'", names.len());

    let listed = called_names(section(&llms, &header));
    let actual: BTreeSet<String> = names.iter().map(|s| s.to_string()).collect();
    let missing: Vec<_> = actual.difference(&listed).collect();
    let unknown: Vec<_> = listed.difference(&actual).collect();
    assert!(missing.is_empty(), "llms.txt 빌트인 목록에 빠진 함수: {missing:?}");
    assert!(unknown.is_empty(), "llms.txt에 있지만 실제로 없는 함수: {unknown:?}");
}

#[test]
fn llms_txt_code_blocks_parse() {
    let llms = read("llms.txt");
    let mut blocks = Vec::new();
    let mut cur: Option<String> = None;
    for line in llms.lines() {
        if line.starts_with("```") {
            match cur.take() {
                Some(b) => blocks.push(b),
                None => cur = Some(String::new()),
            }
        } else if let Some(b) = cur.as_mut() {
            b.push_str(line);
            b.push('\n');
        }
    }
    assert!(blocks.len() >= 5, "코드 블록이 너무 적음: {}", blocks.len());
    for b in blocks {
        // 셸 명령 블록은 건너뛴다
        if b.lines().all(|l| l.starts_with("bang") || l.trim().is_empty()) {
            continue;
        }
        let tokens = bang::lexer::Lexer::new(&b).tokenize().unwrap_or_else(|e| panic!("렉서 오류 {e:?}\n---\n{b}"));
        if let Err(e) = bang::parser::Parser::new(tokens).parse() {
            panic!("llms.txt 코드 블록 파스 오류: {}\n---\n{b}", e.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("; "));
        }
    }
}

#[test]
fn wiki_full_builtin_list_matches_table() {
    let page = read("wiki/표준-라이브러리.md");
    let names = builtins();
    assert!(page.contains(&format!("내장 함수 **{}개**", names.len())), "위키 표준 라이브러리의 개수 표기가 실제({})와 다름", names.len());
    let list = section(&page, "## 전체 목록 (번호순)");
    let mut pairs = Vec::new();
    let toks: Vec<&str> = list.split_whitespace().collect();
    for w in toks.windows(2) {
        if let Ok(n) = w[0].parse::<usize>() {
            pairs.push((n, w[1].to_string()));
        }
    }
    let expected: Vec<(usize, String)> = names.iter().enumerate().map(|(i, n)| (i, n.to_string())).collect();
    assert_eq!(pairs, expected, "위키 '전체 목록'이 BUILTINS(번호·이름)와 다름");
}

#[test]
fn vscode_grammar_lists_every_builtin() {
    let grammar = read("editors/vscode-bang/syntaxes/bang.tmLanguage.json");
    let readme = read("editors/vscode-bang/README.md");
    let names = builtins();
    let missing: Vec<_> = names
        .iter()
        .filter(|n| !grammar.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).any(|w| w == **n))
        .collect();
    assert!(missing.is_empty(), "VS Code 문법에 빠진 빌트인: {missing:?}");
    assert!(readme.contains(&format!("{}개 빌트인", names.len())), "VS Code README의 빌트인 개수가 실제({})와 다름", names.len());
}
