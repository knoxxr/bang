//! 빌트인: 파일시스템

use super::prelude::*;

/// 52: read_file(path) → Str
pub(crate) fn b_read_file(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("read_file", &args, 1, span)?;
    let path = str_arg("read_file", &args[0], span)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| RuntimeError::new(format!("read_file(): '{path}': {e}"), span))?;
    Ok(VmValue::Str(content))
}

/// 53: write_file(path, content: Str|Bytes)
pub(crate) fn b_write_file(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("write_file", &args, 2, span)?;
    let path = str_arg("write_file", &args[0], span)?;
    let content = data_arg("write_file", &args[1], span)?;
    std::fs::write(&path, content)
        .map_err(|e| RuntimeError::new(format!("write_file(): '{path}': {e}"), span))?;
    Ok(VmValue::Nil)
}

/// 69: list_dir(path) → List of entry names
pub(crate) fn b_list_dir(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("list_dir", &args, 1, span)?;
    let path = str_arg("list_dir", &args[0], span)?;
    let mut names = Vec::new();
    let rd = std::fs::read_dir(&path)
        .map_err(|e| RuntimeError::new(format!("list_dir('{path}'): {e}"), span))?;
    for entry in rd.flatten() {
        names.push(VmValue::Str(
            entry.file_name().to_string_lossy().into_owned(),
        ));
    }
    Ok(VmValue::List(Arc::new(names)))
}

/// 70: file_exists(path) → Bool
pub(crate) fn b_file_exists(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("file_exists", &args, 1, span)?;
    let path = str_arg("file_exists", &args[0], span)?;
    Ok(VmValue::Bool(std::path::Path::new(&path).exists()))
}

/// 71: is_dir(path) → Bool
pub(crate) fn b_is_dir(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("is_dir", &args, 1, span)?;
    let path = str_arg("is_dir", &args[0], span)?;
    Ok(VmValue::Bool(std::path::Path::new(&path).is_dir()))
}

/// 104: file_size(path) → Int (바이트 수, Content-Length 용)
pub(crate) fn b_file_size(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("file_size", &args, 1, span)?;
    let path = str_arg("file_size", &args[0], span)?;
    let meta = std::fs::metadata(&path)
        .map_err(|e| RuntimeError::new(format!("file_size('{path}'): {e}"), span))?;
    Ok(VmValue::Int(meta.len() as i64))
}

/// 113: file_remove(path) → nil  (파일만; 디렉터리는 에러)
pub(crate) fn b_file_remove(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("file_remove", &args, 1, span)?;
    let path = str_arg("file_remove", &args[0], span)?;
    if std::path::Path::new(&path).is_dir() {
        return Err(RuntimeError::new(
            format!("file_remove('{path}'): 디렉터리는 삭제할 수 없음 (파일만 지원)"),
            span,
        ));
    }
    std::fs::remove_file(&path)
        .map_err(|e| RuntimeError::new(format!("file_remove('{path}'): {e}"), span))?;
    Ok(VmValue::Nil)
}

/// 114: file_move(src, dst) → nil  (같은 볼륨 내 이동/이름 변경)
pub(crate) fn b_file_move(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("file_move", &args, 2, span)?;
    let src = str_arg("file_move", &args[0], span)?;
    let dst = str_arg("file_move", &args[1], span)?;
    std::fs::rename(&src, &dst)
        .map_err(|e| RuntimeError::new(format!("file_move('{src}', '{dst}'): {e}"), span))?;
    Ok(VmValue::Nil)
}

/// 115: dir_create(path) → nil  (중간 경로 포함, 이미 있으면 통과)
pub(crate) fn b_dir_create(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("dir_create", &args, 1, span)?;
    let path = str_arg("dir_create", &args[0], span)?;
    std::fs::create_dir_all(&path)
        .map_err(|e| RuntimeError::new(format!("dir_create('{path}'): {e}"), span))?;
    Ok(VmValue::Nil)
}

/// 116: file_append(path, content: Str|Bytes) → nil  (없으면 생성)
pub(crate) fn b_file_append(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("file_append", &args, 2, span)?;
    let path = str_arg("file_append", &args[0], span)?;
    let content = data_arg("file_append", &args[1], span)?;
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(content))
        .map_err(|e| RuntimeError::new(format!("file_append('{path}'): {e}"), span))?;
    Ok(VmValue::Nil)
}

/// 121: file_read_bytes(path) → Bytes (무손실, UTF-8 무관)
pub(crate) fn b_file_read_bytes(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("file_read_bytes", &args, 1, span)?;
    let path = str_arg("file_read_bytes", &args[0], span)?;
    let data = std::fs::read(&path)
        .map_err(|e| RuntimeError::new(format!("file_read_bytes('{path}'): {e}"), span))?;
    Ok(VmValue::Bytes(Arc::new(data)))
}

// ── 경로 (Phase 52) — 문자열만 다루며 파일시스템에 접근하지 않는다 (path_abs 제외) ──

fn path_str(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

/// 125: path_join(a, b, ...) → OS 구분자로 이은 경로 (뒤 조각이 절대 경로면 거기서 다시 시작 — Python과 같음)
pub(crate) fn b_path_join(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::new("path_join(): 경로 조각이 하나 이상 필요", span)
            .with_kind(crate::runtime::kind::ARGUMENT));
    }
    let mut p = std::path::PathBuf::new();
    for a in &args {
        p.push(str_arg("path_join", a, span)?);
    }
    Ok(VmValue::Str(path_str(&p)))
}

/// 126: path_basename(p) → 마지막 이름 ("a/b.txt" → "b.txt", 없으면 "")
pub(crate) fn b_path_basename(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("path_basename", &args, 1, span)?;
    let s = str_arg("path_basename", &args[0], span)?;
    let name = std::path::Path::new(&s).file_name().map(|n| n.to_string_lossy().into_owned());
    Ok(VmValue::Str(name.unwrap_or_default()))
}

/// 127: path_dirname(p) → 상위 경로 ("a/b.txt" → "a", "b.txt" → "", "/" → "")
pub(crate) fn b_path_dirname(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("path_dirname", &args, 1, span)?;
    let s = str_arg("path_dirname", &args[0], span)?;
    let parent = std::path::Path::new(&s).parent().map(path_str);
    Ok(VmValue::Str(parent.unwrap_or_default()))
}

/// 128: path_ext(p) → 확장자, 점 없이 ("a.tar.gz" → "gz", 없으면 "")
pub(crate) fn b_path_ext(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("path_ext", &args, 1, span)?;
    let s = str_arg("path_ext", &args[0], span)?;
    let ext = std::path::Path::new(&s).extension().map(|e| e.to_string_lossy().into_owned());
    Ok(VmValue::Str(ext.unwrap_or_default()))
}

/// 129: path_abs(p) → 현재 디렉터리 기준 절대 경로 (파일이 없어도 됨, 심볼릭 링크는 풀지 않음)
pub(crate) fn b_path_abs(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("path_abs", &args, 1, span)?;
    let s = str_arg("path_abs", &args[0], span)?;
    std::path::absolute(&s)
        .map(|p| VmValue::Str(path_str(&p)))
        .map_err(|e| RuntimeError::new(format!("path_abs('{s}'): {e}"), span))
}
