// Bang — 네이티브 코드 생성 (실험적): AOT C 트랜스파일러 (Phase 10)
//
// 정본 실행 엔진은 바이트코드 VM이다. AOT는 정수·문자열·함수 부분집합만 지원하며,
// 미지원 기능은 명확한 에러(TranspileError)로 거절한다.
// (Cranelift JIT는 Phase 44에서 제거 — 바이트코드를 직접 해석해 VM 변경마다 깨졌고 빌드 불가였다.)

pub mod transpile;
pub use transpile::{transpile, transpile_with_source, TranspileError};
