// Bang — 소형 정규식 엔진 (외부 의존성 없음)
//
// 지원 문법:
//   리터럴, .  (개행 제외 임의 문자)
//   * + ?      (greedy 수량자)
//   [...] [^...]  문자 클래스 (범위 a-z, 부정 ^)
//   \d \w \s \D \W \S  단축 클래스, \n \t \r, 이스케이프 \. \* 등
//   ^ $        앵커 (문자열 시작/끝)
//   ( ... )    그룹화, |  교대(alternation)
//
// 구현: 패턴 → AST → 백트래킹 바이트코드 VM. 무한/지수 백트래킹은
// 스텝 상한으로 방어한다(상한 초과 시 매치 실패로 처리).

const STEP_LIMIT: usize = 2_000_000;

// ============================================================================
// AST
// ============================================================================

#[derive(Debug, Clone)]
enum ClassItem {
    Ch(char),
    Range(char, char),
    Shorthand(char), // 'd' 'w' 's' 'D' 'W' 'S'
}

#[derive(Debug, Clone)]
enum Re {
    Char(char),
    Any,
    Class { neg: bool, items: Vec<ClassItem> },
    Start,
    End,
    Concat(Vec<Re>),
    Alt(Vec<Re>),
    Star(Box<Re>),
    Plus(Box<Re>),
    Opt(Box<Re>),
    Empty,
}

// ============================================================================
// 파서
// ============================================================================

struct ReParser {
    chars: Vec<char>,
    pos: usize,
}

impl ReParser {
    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() { self.pos += 1; }
        c
    }

    fn parse_alt(&mut self) -> Result<Re, String> {
        let mut branches = vec![self.parse_concat()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.parse_concat()?);
        }
        if branches.len() == 1 {
            Ok(branches.pop().unwrap())
        } else {
            Ok(Re::Alt(branches))
        }
    }

    fn parse_concat(&mut self) -> Result<Re, String> {
        let mut nodes = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' { break; }
            nodes.push(self.parse_quant()?);
        }
        match nodes.len() {
            0 => Ok(Re::Empty),
            1 => Ok(nodes.pop().unwrap()),
            _ => Ok(Re::Concat(nodes)),
        }
    }

    fn parse_quant(&mut self) -> Result<Re, String> {
        let atom = self.parse_atom()?;
        match self.peek() {
            Some('*') => { self.pos += 1; Ok(Re::Star(Box::new(atom))) }
            Some('+') => { self.pos += 1; Ok(Re::Plus(Box::new(atom))) }
            Some('?') => { self.pos += 1; Ok(Re::Opt(Box::new(atom))) }
            // {n} {n,} {n,m} — 숫자가 바로 뒤따를 때만 수량자로 취급
            Some('{') if self.chars.get(self.pos + 1).is_some_and(|c| c.is_ascii_digit()) => {
                self.parse_brace(atom)
            }
            _ => Ok(atom),
        }
    }

    /// {n} {n,} {n,m} 를 반복으로 디슈가한다.
    fn parse_brace(&mut self, atom: Re) -> Result<Re, String> {
        self.pos += 1; // {
        let read_num = |s: &mut Self| -> Option<usize> {
            let start = s.pos;
            while s.peek().is_some_and(|c| c.is_ascii_digit()) { s.pos += 1; }
            if s.pos == start { return None; }
            s.chars[start..s.pos].iter().collect::<String>().parse().ok()
        };
        let n = read_num(self).ok_or("{ } 안에 숫자 필요")?;
        let (min, max): (usize, Option<usize>) = match self.peek() {
            Some('}') => { self.pos += 1; (n, Some(n)) }
            Some(',') => {
                self.pos += 1;
                if self.peek() == Some('}') {
                    self.pos += 1;
                    (n, None) // {n,}
                } else {
                    let m = read_num(self).ok_or("{n,m} 의 m 필요")?;
                    if self.peek() != Some('}') { return Err("닫는 '}' 없음".into()); }
                    self.pos += 1;
                    (n, Some(m))
                }
            }
            _ => return Err("{ } 문법 오류".into()),
        };
        if let Some(mx) = max {
            if mx < min { return Err("{n,m} 에서 m < n".into()); }
        }
        // 디슈가: n개 필수 + (max-n)개 선택 / 또는 무제한이면 Star
        let mut parts: Vec<Re> = Vec::new();
        for _ in 0..min { parts.push(atom.clone()); }
        match max {
            None => parts.push(Re::Star(Box::new(atom))),
            Some(mx) => { for _ in min..mx { parts.push(Re::Opt(Box::new(atom.clone()))); } }
        }
        Ok(match parts.len() {
            0 => Re::Empty,
            1 => parts.pop().unwrap(),
            _ => Re::Concat(parts),
        })
    }

    fn parse_atom(&mut self) -> Result<Re, String> {
        match self.next() {
            None => Err("예상치 못한 패턴 끝".into()),
            Some('(') => {
                let inner = self.parse_alt()?;
                if self.next() != Some(')') {
                    return Err("닫는 ')' 없음".into());
                }
                Ok(inner)
            }
            Some('[') => self.parse_class(),
            Some('.') => Ok(Re::Any),
            Some('^') => Ok(Re::Start),
            Some('$') => Ok(Re::End),
            Some('\\') => {
                let e = self.next().ok_or("패턴이 \\ 로 끝남")?;
                Ok(match e {
                    'd' | 'w' | 's' | 'D' | 'W' | 'S' => Re::Class { neg: false, items: vec![ClassItem::Shorthand(e)] },
                    'n' => Re::Char('\n'),
                    't' => Re::Char('\t'),
                    'r' => Re::Char('\r'),
                    other => Re::Char(other),
                })
            }
            Some(')') | Some('|') => Err("예상치 못한 메타문자".into()),
            Some('*') | Some('+') | Some('?') => Err("수량자 앞에 원자 없음".into()),
            Some(c) => Ok(Re::Char(c)),
        }
    }

    fn parse_class(&mut self) -> Result<Re, String> {
        let mut neg = false;
        if self.peek() == Some('^') { neg = true; self.pos += 1; }
        let mut items = Vec::new();
        loop {
            match self.next() {
                None => return Err("닫는 ']' 없음".into()),
                Some(']') => break,
                Some('\\') => {
                    let e = self.next().ok_or("클래스가 \\ 로 끝남")?;
                    match e {
                        'd' | 'w' | 's' | 'D' | 'W' | 'S' => items.push(ClassItem::Shorthand(e)),
                        'n' => items.push(ClassItem::Ch('\n')),
                        't' => items.push(ClassItem::Ch('\t')),
                        'r' => items.push(ClassItem::Ch('\r')),
                        other => items.push(ClassItem::Ch(other)),
                    }
                }
                Some(c) => {
                    // 범위 a-z 검사
                    if self.peek() == Some('-') && self.chars.get(self.pos + 1).is_some_and(|&n| n != ']') {
                        self.pos += 1; // '-'
                        let hi = self.next().unwrap();
                        items.push(ClassItem::Range(c, hi));
                    } else {
                        items.push(ClassItem::Ch(c));
                    }
                }
            }
        }
        Ok(Re::Class { neg, items })
    }
}

fn shorthand_match(kind: char, c: char) -> bool {
    match kind {
        'd' => c.is_ascii_digit(),
        'D' => !c.is_ascii_digit(),
        'w' => c.is_alphanumeric() || c == '_',
        'W' => !(c.is_alphanumeric() || c == '_'),
        's' => c.is_whitespace(),
        'S' => !c.is_whitespace(),
        _ => false,
    }
}

fn class_match(neg: bool, items: &[ClassItem], c: char) -> bool {
    let mut hit = false;
    for it in items {
        let m = match it {
            ClassItem::Ch(x) => *x == c,
            ClassItem::Range(a, b) => *a <= c && c <= *b,
            ClassItem::Shorthand(k) => shorthand_match(*k, c),
        };
        if m { hit = true; break; }
    }
    hit ^ neg
}

// ============================================================================
// 바이트코드 컴파일
// ============================================================================

#[derive(Debug, Clone)]
enum Inst {
    Char(char),
    Any,
    Class { neg: bool, items: Vec<ClassItem> },
    Start,
    End,
    Jmp(usize),
    Split(usize, usize),
    Match,
}

fn compile_node(re: &Re, prog: &mut Vec<Inst>) {
    match re {
        Re::Empty => {}
        Re::Char(c) => prog.push(Inst::Char(*c)),
        Re::Any => prog.push(Inst::Any),
        Re::Start => prog.push(Inst::Start),
        Re::End => prog.push(Inst::End),
        Re::Class { neg, items } => prog.push(Inst::Class { neg: *neg, items: items.clone() }),
        Re::Concat(nodes) => { for n in nodes { compile_node(n, prog); } }
        Re::Alt(branches) => {
            // Split b0, b1; ... 마지막은 직접
            let mut jmp_ends = Vec::new();
            for (i, b) in branches.iter().enumerate() {
                if i + 1 < branches.len() {
                    let split_pos = prog.len();
                    prog.push(Inst::Split(0, 0)); // backpatch
                    let l1 = prog.len();
                    compile_node(b, prog);
                    let jmp_pos = prog.len();
                    prog.push(Inst::Jmp(0)); // backpatch to end
                    jmp_ends.push(jmp_pos);
                    let l2 = prog.len();
                    prog[split_pos] = Inst::Split(l1, l2);
                } else {
                    compile_node(b, prog);
                }
            }
            let end = prog.len();
            for j in jmp_ends { prog[j] = Inst::Jmp(end); }
        }
        Re::Star(inner) => {
            let l1 = prog.len();
            prog.push(Inst::Split(0, 0)); // backpatch
            let l2 = prog.len();
            compile_node(inner, prog);
            prog.push(Inst::Jmp(l1));
            let l3 = prog.len();
            prog[l1] = Inst::Split(l2, l3);
        }
        Re::Plus(inner) => {
            let l1 = prog.len();
            compile_node(inner, prog);
            let split_pos = prog.len();
            prog.push(Inst::Split(0, 0));
            let l3 = prog.len();
            prog[split_pos] = Inst::Split(l1, l3);
        }
        Re::Opt(inner) => {
            let split_pos = prog.len();
            prog.push(Inst::Split(0, 0));
            let l2 = prog.len();
            compile_node(inner, prog);
            let l3 = prog.len();
            prog[split_pos] = Inst::Split(l2, l3);
        }
    }
}

// ============================================================================
// 공개 API
// ============================================================================

pub struct Regex {
    prog: Vec<Inst>,
}

pub fn compile(pattern: &str) -> Result<Regex, String> {
    let mut p = ReParser { chars: pattern.chars().collect(), pos: 0 };
    let ast = p.parse_alt()?;
    if p.pos != p.chars.len() {
        return Err("패턴을 끝까지 파싱하지 못함".into());
    }
    let mut prog = Vec::new();
    compile_node(&ast, &mut prog);
    prog.push(Inst::Match);
    Ok(Regex { prog })
}

impl Regex {
    /// start 위치에서 시작하는 매치를 시도, 끝 위치(char index) 반환.
    fn run_at(&self, chars: &[char], start: usize) -> Option<usize> {
        let mut stack: Vec<(usize, usize)> = vec![(0, start)];
        let mut steps = 0usize;
        while let Some((mut pc, mut sp)) = stack.pop() {
            loop {
                steps += 1;
                if steps > STEP_LIMIT { return None; }
                match &self.prog[pc] {
                    Inst::Match => return Some(sp),
                    Inst::Char(c) => {
                        if sp < chars.len() && chars[sp] == *c { pc += 1; sp += 1; } else { break; }
                    }
                    Inst::Any => {
                        if sp < chars.len() && chars[sp] != '\n' { pc += 1; sp += 1; } else { break; }
                    }
                    Inst::Class { neg, items } => {
                        if sp < chars.len() && class_match(*neg, items, chars[sp]) { pc += 1; sp += 1; } else { break; }
                    }
                    Inst::Start => { if sp == 0 { pc += 1; } else { break; } }
                    Inst::End => { if sp == chars.len() { pc += 1; } else { break; } }
                    Inst::Jmp(a) => { pc = *a; }
                    Inst::Split(a, b) => { stack.push((*b, sp)); pc = *a; }
                }
            }
        }
        None
    }

    /// 가장 왼쪽 매치 (start, end) char index 반환.
    pub fn search(&self, chars: &[char]) -> Option<(usize, usize)> {
        for start in 0..=chars.len() {
            if let Some(end) = self.run_at(chars, start) {
                return Some((start, end));
            }
        }
        None
    }

    pub fn is_match(&self, chars: &[char]) -> bool {
        self.search(chars).is_some()
    }

    /// 비겹침 매치들의 (start, end) 목록.
    pub fn find_all(&self, chars: &[char]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i <= chars.len() {
            if let Some(end) = self.run_at(chars, i) {
                out.push((i, end));
                // 빈 매치면 1칸 전진(무한 루프 방지)
                i = if end > i { end } else { i + 1 };
            } else {
                i += 1;
            }
        }
        out
    }

    /// 모든 매치를 repl 문자열로 치환.
    pub fn replace_all(&self, chars: &[char], repl: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for (s, e) in self.find_all(chars) {
            out.extend(&chars[last..s]);
            out.push_str(repl);
            last = e;
        }
        out.extend(&chars[last..]);
        out
    }
}

// ============================================================================
// 단위 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pat: &str, text: &str) -> bool {
        compile(pat).unwrap().is_match(&text.chars().collect::<Vec<_>>())
    }
    fn find(pat: &str, text: &str) -> Option<String> {
        let c: Vec<char> = text.chars().collect();
        compile(pat).unwrap().search(&c).map(|(s, e)| c[s..e].iter().collect())
    }

    #[test]
    fn literals_and_dot() {
        assert!(m("abc", "xxabcyy"));
        assert!(!m("abc", "abx"));
        assert!(m("a.c", "axc"));
        assert!(!m("a.c", "a\nc"));
    }

    #[test]
    fn quantifiers() {
        assert!(m("ab*c", "ac"));
        assert!(m("ab*c", "abbbc"));
        assert!(m("ab+c", "abc"));
        assert!(!m("ab+c", "ac"));
        assert!(m("ab?c", "ac"));
        assert!(m("ab?c", "abc"));
    }

    #[test]
    fn classes_and_shorthand() {
        assert!(m("[a-z]+", "hello"));
        assert!(m("[0-9]+", "abc123"));
        assert!(m(r"\d+", "x42"));
        assert!(!m(r"^\d+$", "12a"));
        assert!(m(r"\w+", "hello_world"));
        assert!(m("[^0-9]", "a"));
        assert!(!m("[^0-9]", "5"));
    }

    #[test]
    fn anchors() {
        assert!(m("^abc", "abcdef"));
        assert!(!m("^abc", "xabc"));
        assert!(m("abc$", "xxabc"));
        assert!(!m("abc$", "abcx"));
    }

    #[test]
    fn alternation_and_groups() {
        assert!(m("cat|dog", "I have a dog"));
        assert!(m("(ab)+", "abab"));
        assert!(m("(cat|dog)s?", "cats"));
        assert!(!m("^(cat|dog)$", "fish"));
    }

    #[test]
    fn brace_quantifiers() {
        assert_eq!(find(r"\d{4}", "2023-11-14"), Some("2023".to_string()));
        assert!(m("[a-z]{4}", "abcd"));
        assert!(!m("^[a-z]{4}$", "abc"));
        assert!(m("a{2,4}", "aaa"));
        assert!(m(r"^\d{4}-\d{2}-\d{2}$", "2023-11-14"));
        assert!(!m(r"^\d{4}$", "123"));
    }

    #[test]
    fn find_and_replace() {
        assert_eq!(find(r"\d+", "abc123def"), Some("123".to_string()));
        let c: Vec<char> = "a1b2c3".chars().collect();
        let re = compile(r"\d").unwrap();
        assert_eq!(re.find_all(&c).len(), 3);
        assert_eq!(re.replace_all(&c, "#"), "a#b#c#");
    }
}
