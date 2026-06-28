// Bang — 패키지 매니페스트 (bang.toml)
//
// 중앙 레지스트리 없이 git 기반 의존성을 관리한다. 매니페스트는 최소 형식:
//
//   [dependencies]
//   mathutils = "https://github.com/user/mathutils@v1.0.0"
//   util      = "https://github.com/user/util"
//
// 값은 "git-url" 또는 "git-url@rev"(태그/브랜치/커밋). bang_modules/<name>/ 에 설치된다.

/// 하나의 의존성.
#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub url: String,
    pub rev: Option<String>,
}

/// bang.toml 텍스트 → 의존성 목록. [dependencies] 섹션의 key = "value" 만 읽는다.
pub fn parse_manifest(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_deps = line == "[dependencies]";
            continue;
        }
        if !in_deps {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let name = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            if name.is_empty() || val.is_empty() {
                continue;
            }
            let (url, rev) = match val.rsplit_once('@') {
                // '@' 가 스킴(://) 뒤에 있는 경우만 rev로 취급
                Some((u, r)) if u.contains("://") => (u.to_string(), Some(r.to_string())),
                _ => (val.clone(), None),
            };
            deps.push(Dependency { name, url, rev });
        }
    }
    deps
}

/// 의존성 목록 → bang.toml 텍스트 ([dependencies] 섹션).
pub fn serialize_manifest(deps: &[Dependency]) -> String {
    let mut out = String::from("[dependencies]\n");
    for d in deps {
        let val = match &d.rev {
            Some(r) => format!("{}@{}", d.url, r),
            None => d.url.clone(),
        };
        out.push_str(&format!("{} = \"{}\"\n", d.name, val));
    }
    out
}

/// 의존성 추가/갱신 (같은 이름이면 교체).
pub fn upsert(deps: &mut Vec<Dependency>, dep: Dependency) {
    if let Some(existing) = deps.iter_mut().find(|d| d.name == dep.name) {
        *existing = dep;
    } else {
        deps.push(dep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let m = "\
[package]
name = \"app\"

[dependencies]
mathutils = \"https://github.com/u/mathutils@v1.0.0\"
util = \"https://github.com/u/util\"
";
        let deps = parse_manifest(m);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0], Dependency {
            name: "mathutils".into(),
            url: "https://github.com/u/mathutils".into(),
            rev: Some("v1.0.0".into()),
        });
        assert_eq!(deps[1].rev, None);
    }

    #[test]
    fn ignores_non_dep_sections() {
        let m = "[package]\nname = \"x\"\nversion = \"1\"\n";
        assert!(parse_manifest(m).is_empty());
    }

    #[test]
    fn roundtrip() {
        let deps = vec![
            Dependency { name: "a".into(), url: "https://h/a".into(), rev: Some("v1".into()) },
            Dependency { name: "b".into(), url: "https://h/b".into(), rev: None },
        ];
        let text = serialize_manifest(&deps);
        assert_eq!(parse_manifest(&text), deps);
    }

    #[test]
    fn upsert_replaces() {
        let mut deps = vec![Dependency { name: "a".into(), url: "u1".into(), rev: None }];
        upsert(&mut deps, Dependency { name: "a".into(), url: "u2".into(), rev: Some("v2".into()) });
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].url, "u2");
        assert_eq!(deps[0].rev, Some("v2".into()));
    }
}
