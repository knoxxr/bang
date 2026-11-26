//! 권한 모드 (Phase 64) — AI가 쓴 코드나 남의 스크립트를 안심하고 실행하기 위한 Deno식 허가 목록.
//!
//! `bang run --sandbox app.bang` 또는 `--allow-*` 플래그가 하나라도 있으면 권한 모드가 켜지고,
//! 허가하지 않은 파일 읽기·쓰기, 네트워크, 프로세스 실행, 환경 변수 읽기는 `permission` 에러가 된다
//! (catch 가능). 플래그가 없으면 지금까지처럼 모두 허용한다.
//!
//! 검사는 빌트인 디스패치 한 곳(`Vm::call_builtin`)에서 이름·인자를 보고 한다 — 직접 호출·메서드 문법·
//! spawn·콜백 어떤 경로로 불러도 같은 검사를 거친다. 권한은 프로세스 전체에 한 번 설정되고 스크립트가
//! 바꿀 수 없다(모든 작업이 같은 권한).
//!
//! 경로는 심볼릭 링크를 실제 경로로 풀어(존재하지 않으면 가장 가까운 존재하는 상위 디렉터리를 풀고 나머지를
//! 붙여) 허가 디렉터리 아래인지 본다 — `./data/../../etc/passwd`나 링크로 빠져나갈 수 없다.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::vm::VmValue;

/// 한 분야의 허가: 전부 / 목록.
#[derive(Debug, Clone, Default)]
pub enum Allow {
    /// 아무것도 허가하지 않음 (권한 모드의 기본)
    #[default]
    None,
    /// 이 분야 전부 허가 (`--allow-read`처럼 값 없이)
    All,
    /// 이 목록만 허가 (`--allow-read=./data,/tmp`)
    List(Vec<String>),
}

/// 권한 설정. `Permissions::default()`는 권한 모드에서 "모두 거부".
#[derive(Debug, Clone, Default)]
pub struct Permissions {
    pub read: Allow,
    pub write: Allow,
    pub net: Allow,
    pub run: Allow,
    pub env: Allow,
    /// read/write 목록을 실제 경로로 풀어 둔 것 (install 때 계산)
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
}

static ACTIVE: OnceLock<Permissions> = OnceLock::new();

/// CLI 전용: 권한 모드를 켠다 (프로세스에서 한 번 — 두 번째 호출은 무시).
pub fn install(mut p: Permissions) {
    p.read_roots = roots(&p.read);
    p.write_roots = roots(&p.write);
    let _ = ACTIVE.set(p);
}

/// 권한 모드가 켜져 있는가.
pub fn active() -> bool {
    ACTIVE.get().is_some()
}

fn roots(a: &Allow) -> Vec<PathBuf> {
    match a {
        Allow::List(items) => items.iter().map(|s| real_path(Path::new(s))).collect(),
        _ => Vec::new(),
    }
}

/// CLI 플래그 하나를 해석해 설정에 반영한다. 권한 플래그가 아니면 false.
/// `--sandbox`, `--allow-all`, `--allow-read[=a,b]`, `--allow-write[=..]`, `--allow-net[=..]`,
/// `--allow-run[=..]`, `--allow-env[=..]`.
pub fn parse_flag(p: &mut Permissions, arg: &str) -> Result<bool, String> {
    if arg == "--sandbox" {
        return Ok(true);
    }
    if arg == "--allow-all" {
        p.read = Allow::All;
        p.write = Allow::All;
        p.net = Allow::All;
        p.run = Allow::All;
        p.env = Allow::All;
        return Ok(true);
    }
    let Some(rest) = arg.strip_prefix("--allow-") else { return Ok(false) };
    let (cat, list) = match rest.split_once('=') {
        Some((c, l)) => (c, Some(l)),
        None => (rest, None),
    };
    let slot = match cat {
        "read" => &mut p.read,
        "write" => &mut p.write,
        "net" => &mut p.net,
        "run" => &mut p.run,
        "env" => &mut p.env,
        other => {
            return Err(format!(
                "알 수 없는 권한 '--allow-{other}' (가능: read, write, net, run, env, all)"
            ))
        }
    };
    match list {
        None => *slot = Allow::All,
        Some(l) => {
            let items: Vec<String> = l.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            if items.is_empty() {
                return Err(format!("'--allow-{cat}=' 뒤에 값이 없음 (예: --allow-{cat}=값1,값2)"));
            }
            match slot {
                Allow::All => {}
                Allow::List(v) => v.extend(items),
                Allow::None => *slot = Allow::List(items),
            }
        }
    }
    Ok(true)
}

/// 경로를 실제 경로로: 존재하면 canonicalize, 아니면 존재하는 가장 가까운 상위를 풀고 나머지를 붙인다.
/// 나머지에 `..`가 있으면 어휘적으로 정리한다(존재하지 않는 경로라 링크가 끼어들 수 없다).
fn real_path(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    if let Ok(c) = std::fs::canonicalize(&abs) {
        return c;
    }
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = abs.as_path();
    loop {
        match cur.parent() {
            Some(parent) => {
                if let Some(name) = cur.file_name() {
                    tail.push(name.to_os_string());
                } else if cur.components().next_back() == Some(Component::ParentDir) {
                    tail.push("..".into());
                }
                if let Ok(c) = std::fs::canonicalize(parent) {
                    let mut out = c;
                    for t in tail.iter().rev() {
                        if t == ".." {
                            out.pop();
                        } else {
                            out.push(t);
                        }
                    }
                    return out;
                }
                cur = parent;
            }
            None => return abs,
        }
    }
}

fn path_allowed(allow: &Allow, roots: &[PathBuf], target: &str) -> bool {
    match allow {
        Allow::All => true,
        Allow::None => false,
        Allow::List(_) => {
            let real = real_path(Path::new(target));
            roots.iter().any(|r| real.starts_with(r))
        }
    }
}

/// "host:port" / "[::1]:80" / "host" → (소문자 host, port).
fn split_host_port(addr: &str) -> (String, Option<u16>) {
    let addr = addr.trim();
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some((h, after)) = rest.split_once(']') {
            let port = after.strip_prefix(':').and_then(|p| p.parse().ok());
            return (h.to_ascii_lowercase(), port);
        }
    }
    match addr.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h.to_ascii_lowercase(), p.parse().ok()),
        _ => (addr.to_ascii_lowercase(), None),
    }
}

/// URL에서 (host, port). 포트가 없으면 스킴 기본값(http 80, https 443).
fn url_host_port(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let default = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let (h, p) = split_host_port(authority);
    Some((h, p.unwrap_or(default)))
}

fn net_allowed(allow: &Allow, host: &str, port: Option<u16>) -> bool {
    match allow {
        Allow::All => true,
        Allow::None => false,
        Allow::List(items) => items.iter().any(|item| {
            let (h, p) = split_host_port(item);
            h == host && (p.is_none() || p == port)
        }),
    }
}

fn name_allowed(allow: &Allow, name: &str) -> bool {
    match allow {
        Allow::All => true,
        Allow::None => false,
        Allow::List(items) => items.iter().any(|i| i == name),
    }
}

fn str_at(args: &[VmValue], i: usize) -> Option<&str> {
    match args.get(i) {
        Some(VmValue::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// 빌트인 호출 전에 권한을 확인한다. 권한 모드가 아니면 항상 통과.
/// 인자 타입이 틀린 호출은 여기서 막지 않는다 — 빌트인이 argument 에러를 낸다(그때는 아무 일도 일어나지 않음).
pub fn check(name: &str, args: &[VmValue]) -> Result<(), String> {
    let Some(p) = ACTIVE.get() else { return Ok(()) };
    let deny_path = |cat: &str, path: &str| {
        format!("권한 없음: {name}()의 {cat} '{path}' — 허용하려면 --allow-{cat}={path} (또는 상위 디렉터리)")
    };
    let read = |path: Option<&str>| -> Result<(), String> {
        match path {
            Some(t) if !path_allowed(&p.read, &p.read_roots, t) => Err(deny_path("read", t)),
            _ => Ok(()),
        }
    };
    let write = |path: Option<&str>| -> Result<(), String> {
        match path {
            Some(t) if !path_allowed(&p.write, &p.write_roots, t) => Err(deny_path("write", t)),
            _ => Ok(()),
        }
    };
    let net_addr = |addr: Option<&str>| -> Result<(), String> {
        let Some(a) = addr else { return Ok(()) };
        let (h, port) = split_host_port(a);
        if net_allowed(&p.net, &h, port) {
            Ok(())
        } else {
            Err(format!("권한 없음: {name}()의 net '{a}' — 허용하려면 --allow-net={a}"))
        }
    };
    let net_url = |url: Option<&str>| -> Result<(), String> {
        let Some(u) = url else { return Ok(()) };
        let Some((h, port)) = url_host_port(u) else { return Ok(()) }; // 잘못된 URL은 빌트인이 에러
        if net_allowed(&p.net, &h, Some(port)) {
            Ok(())
        } else {
            Err(format!("권한 없음: {name}()의 net '{h}:{port}' — 허용하려면 --allow-net={h} (또는 {h}:{port})"))
        }
    };
    match name {
        "read_file" | "file_read_bytes" | "file_exists" | "is_dir" | "list_dir" | "file_size" => read(str_at(args, 0)),
        "tcp_send_file" => read(str_at(args, 1)),
        "write_file" | "file_append" | "dir_create" | "file_remove" => write(str_at(args, 0)),
        "file_move" => {
            write(str_at(args, 0))?;
            write(str_at(args, 1))
        }
        "tcp_listen" | "tcp_connect" | "serve_event" => net_addr(str_at(args, 0)),
        "http_get" | "http_post" => net_url(str_at(args, 0)),
        "http_request" => net_url(str_at(args, 1)),
        "proc_run" => {
            let prog = match args.first() {
                Some(VmValue::List(l)) => match l.first() {
                    Some(VmValue::Str(s)) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            };
            match prog {
                Some(prog) if !name_allowed(&p.run, &prog) => {
                    Err(format!("권한 없음: proc_run()의 run '{prog}' — 허용하려면 --allow-run={prog}"))
                }
                _ => Ok(()),
            }
        }
        "env_get" => match str_at(args, 0) {
            Some(v) if !name_allowed(&p.env, v) => {
                Err(format!("권한 없음: env_get()의 env '{v}' — 허용하려면 --allow-env={v}"))
            }
            _ => Ok(()),
        },
        // 모듈 불러오기는 코드 적재라 허용하되, .bang 소스만 (다른 파일을 import로 읽어 에러 메시지로 새게 하지 않음)
        "import" => match str_at(args, 0) {
            Some(s) if (s.contains('/') || s.contains('\\')) && !s.ends_with(".bang") => Err(format!(
                "권한 없음: 권한 모드에서 import()는 .bang 파일만 불러올 수 있음, '{s}'"
            )),
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flags() {
        let mut p = Permissions::default();
        assert!(parse_flag(&mut p, "--allow-net=api.example.com,localhost:8080").unwrap());
        assert!(parse_flag(&mut p, "--allow-read").unwrap());
        assert!(parse_flag(&mut p, "--sandbox").unwrap());
        assert!(!parse_flag(&mut p, "--dump-ast").unwrap());
        assert!(parse_flag(&mut p, "--allow-disk").is_err());
        assert!(parse_flag(&mut p, "--allow-env=").is_err());
        assert!(matches!(p.read, Allow::All));
        assert!(net_allowed(&p.net, "api.example.com", Some(443)));
        assert!(net_allowed(&p.net, "localhost", Some(8080)));
        assert!(!net_allowed(&p.net, "localhost", Some(9090)));
        assert!(!net_allowed(&p.net, "evil.com", Some(443)));
        assert!(matches!(p.write, Allow::None));
    }

    #[test]
    fn host_parsing() {
        assert_eq!(url_host_port("https://API.example.com/v1?x=1"), Some(("api.example.com".into(), 443)));
        assert_eq!(url_host_port("http://u:p@h:8080/"), Some(("h".into(), 8080)));
        assert_eq!(url_host_port("http://[::1]:9000/x"), Some(("::1".into(), 9000)));
        assert_eq!(url_host_port("ftp://x"), None);
        assert_eq!(split_host_port("0.0.0.0:80"), ("0.0.0.0".into(), Some(80)));
    }

    #[test]
    fn paths_cannot_escape() {
        let dir = std::env::temp_dir().join(format!("bang_perm_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let roots = vec![real_path(&dir.join("data"))];
        let allow = Allow::List(vec![]);
        let d = dir.join("data");
        assert!(path_allowed(&allow, &roots, d.join("a.txt").to_str().unwrap()));
        assert!(path_allowed(&allow, &roots, d.join("new/sub/b.txt").to_str().unwrap()));
        assert!(!path_allowed(&allow, &roots, d.join("../secret.txt").to_str().unwrap()));
        assert!(!path_allowed(&allow, &roots, d.join("nope/../../secret.txt").to_str().unwrap()));
        assert!(!path_allowed(&allow, &roots, dir.join("data2/x").to_str().unwrap()));
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink("/etc", d.join("link"));
            assert!(!path_allowed(&allow, &roots, d.join("link/passwd").to_str().unwrap()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
