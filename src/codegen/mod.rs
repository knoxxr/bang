// Bang — Phase 9: 네이티브 백엔드 / Phase 10: AOT C 트랜스파일러

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "jit")]
pub use jit::{try_jit_call, JitStats};

pub mod transpile;
pub use transpile::{transpile, TranspileError};

/// JIT 기능이 비활성화된 빌드에서 `--jit` 플래그를 감지했을 때 반환하는 메시지
pub const JIT_DISABLED_MSG: &str =
    "JIT 백엔드가 비활성화되어 있습니다. `cargo build --features jit`으로 빌드하세요.";
