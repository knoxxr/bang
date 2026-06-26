// Bang — Phase 9 Part B: Cranelift JIT (cranelift 0.109)
//
// 지원 범위: Int 값만 사용하는 내부 함수
//   - 전역·업값·클로저·호출·spawn·채널 없음
//   - 지원 opcode: 산술, 비교, 로컬 변수, 점프, 반환
// JIT 불가능하면 None 반환 → VM 자동 폴백.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, AbiParam, Block, InstBuilder, Value};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use crate::compiler::{
    OP_ADD, OP_CONST, OP_DIV, OP_DUP, OP_EQ, OP_FALSE, OP_GE, OP_GT, OP_JUMP, OP_JUMP_FALSE,
    OP_LE, OP_LOAD_LOCAL, OP_LT, OP_MOD, OP_MUL, OP_NE, OP_NEG, OP_NIL, OP_NOT, OP_POP,
    OP_RETURN, OP_STORE_LOCAL, OP_SUB, OP_TRUE,
};
use crate::lexer::token::Span;
use crate::runtime::RuntimeError;
use crate::vm::{CompiledFn, VmValue};

// fn(args_ptr: *const i64, args_len: usize) -> i64
type JitFnPtr = unsafe extern "C" fn(*const i64, usize) -> i64;

/// JIT 통계 (진단·테스트용)
pub struct JitStats {
    pub compiled:   usize,
    pub cache_hits: usize,
    pub bailouts:   usize,
}

// ============================================================================
// 스레드-로컬 JIT 상태
// ============================================================================

struct JitState {
    module: JITModule,
    cache:  HashMap<usize, JitFnPtr>, // Arc<CompiledFn> 포인터 주소 → JIT fn ptr
    pub stats: JitStats,
}

thread_local! {
    static JIT: RefCell<Option<JitState>> = const { RefCell::new(None) };
}

fn with_jit<R>(f: impl FnOnce(&mut JitState) -> R) -> R {
    JIT.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow = Some(create_state());
        }
        f(borrow.as_mut().unwrap())
    })
}

fn create_state() -> JitState {
    let mut flag_builder = settings::builder();
    // PIC 비활성화 (일반 실행 파일용)
    flag_builder.set("use_colocated_libcalls", "false").unwrap();
    flag_builder.set("is_pic", "false").unwrap();
    let flags = settings::Flags::new(flag_builder);

    let isa_builder = cranelift_native::builder()
        .unwrap_or_else(|msg| panic!("JIT: 호스트 ISA 초기화 실패: {msg}"));
    let isa = isa_builder.finish(flags).expect("JIT: ISA 완성 실패");
    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

    JitState {
        module: JITModule::new(builder),
        cache:  HashMap::new(),
        stats:  JitStats { compiled: 0, cache_hits: 0, bailouts: 0 },
    }
}

// ============================================================================
// JIT 컴파일 가능 여부 검사
// ============================================================================

/// 함수가 Int-only JIT 대상인지 확인한다.
/// 전역·업값·클로저·채널·외부 호출을 쓰지 않아야 한다.
pub fn can_jit(func: &CompiledFn) -> bool {
    // 상수 풀이 모두 Int 여야 한다
    for c in &func.chunk.constants {
        if !matches!(c, VmValue::Int(_)) {
            return false;
        }
    }

    let code = &func.chunk.code;
    let mut i = 0;
    while i < code.len() {
        let op = code[i];
        let skip: usize = match op {
            // 무-피연산자 단순 명령어
            OP_POP | OP_NIL | OP_TRUE | OP_FALSE | OP_DUP
            | OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MOD | OP_NEG
            | OP_EQ | OP_NE | OP_LT | OP_LE | OP_GT | OP_GE | OP_NOT
            | OP_RETURN => 1,

            // 1-byte 피연산자
            OP_CONST | OP_LOAD_LOCAL | OP_STORE_LOCAL => 2,

            // 2-byte (i16) 점프
            OP_JUMP | OP_JUMP_FALSE => 3,

            // 비지원 opcode → 즉시 false
            _ => return false,
        };
        i += skip;
    }
    true
}

// ============================================================================
// 기본 블록 경계 탐색
// ============================================================================

fn find_block_entries(code: &[u8]) -> BTreeSet<usize> {
    let mut entries = BTreeSet::new();
    entries.insert(0usize);

    let mut i = 0usize;
    while i < code.len() {
        let op = code[i];
        match op {
            OP_JUMP | OP_JUMP_FALSE if i + 2 < code.len() => {
                let offset = i16::from_le_bytes([code[i + 1], code[i + 2]]);
                let target_ip = (i as isize + 3 + offset as isize) as usize;
                entries.insert(target_ip);
                entries.insert(i + 3); // fall-through 도 새 블록
                i += 3;
            }
            OP_CONST | OP_LOAD_LOCAL | OP_STORE_LOCAL => i += 2,
            _ => i += 1,
        }
    }
    entries
}

// ============================================================================
// 스택 헬퍼 (Cranelift Variable 기반 가상 스택)
// ============================================================================

#[inline]
fn stack_push(builder: &mut FunctionBuilder, depth: &mut usize, base: usize, v: Value) {
    builder.def_var(Variable::new(base + *depth), v);
    *depth += 1;
}

#[inline]
fn stack_pop(builder: &mut FunctionBuilder, depth: &mut usize, base: usize) -> Value {
    *depth -= 1;
    builder.use_var(Variable::new(base + *depth))
}

#[inline]
fn stack_peek(builder: &mut FunctionBuilder, depth: usize, base: usize) -> Value {
    builder.use_var(Variable::new(base + depth - 1))
}

// ============================================================================
// 함수 단위 JIT 컴파일
// ============================================================================

fn compile_one(state: &mut JitState, func: &Arc<CompiledFn>) -> Result<JitFnPtr, String> {
    let code       = &func.chunk.code;
    let local_count = func.local_count;
    let arity       = func.arity;

    // 시그니처: (i64 x arity) → i64
    let mut sig = state.module.make_signature();
    for _ in 0..arity {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));

    // 충돌 없는 고유 이름
    let base_name = func
        .name
        .as_deref()
        .unwrap_or("__anon__")
        .replace(['-', ' ', '.'], "_");
    let unique_name = format!("{base_name}_{:x}", Arc::as_ptr(func) as usize);

    let func_id = state
        .module
        .declare_function(&unique_name, Linkage::Local, &sig)
        .map_err(|e| e.to_string())?;

    let mut ctx = state.module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // 기본 블록 생성
        let entries = find_block_entries(code);
        let ip_to_block: HashMap<usize, Block> =
            entries.iter().map(|&ip| (ip, builder.create_block())).collect();

        let entry_block = ip_to_block[&0];
        builder.append_block_params_for_function_params(entry_block);

        // 변수 선언: 로컬(0..local_count) + 가상 스택(local_count..+64)
        const MAX_STACK: usize = 64;
        for idx in 0..local_count + MAX_STACK {
            builder.declare_var(Variable::new(idx), types::I64);
        }

        // 진입 블록 초기화
        builder.switch_to_block(entry_block);
        let zero = builder.ins().iconst(types::I64, 0);
        for idx in 0..local_count {
            if idx < arity {
                let v = builder.block_params(entry_block)[idx];
                builder.def_var(Variable::new(idx), v);
            } else {
                builder.def_var(Variable::new(idx), zero);
            }
        }
        for idx in local_count..local_count + MAX_STACK {
            builder.def_var(Variable::new(idx), zero);
        }

        // 바이트코드 → Cranelift IR 변환 (선형 패스)
        let mut stack_depth: usize = 0;
        let mut terminated = false; // 현 블록이 이미 종결됐는가
        let mut i = 0usize;

        while i < code.len() {
            // 새 기본 블록 진입 처리
            if i > 0 && entries.contains(&i) {
                let new_block = ip_to_block[&i];
                if !terminated {
                    builder.ins().jump(new_block, &[]);
                }
                builder.switch_to_block(new_block);
                terminated = false;
            }

            if terminated {
                // 종결된 블록의 dead code — 다음 블록 경계까지 skip
                let op = code[i];
                i += match op {
                    OP_CONST | OP_LOAD_LOCAL | OP_STORE_LOCAL => 2,
                    OP_JUMP | OP_JUMP_FALSE                   => 3,
                    _                                         => 1,
                };
                continue;
            }

            let op = code[i]; i += 1;

            match op {
                OP_NIL | OP_FALSE => {
                    let v = builder.ins().iconst(types::I64, 0);
                    stack_push(&mut builder, &mut stack_depth, local_count, v);
                }
                OP_TRUE => {
                    let v = builder.ins().iconst(types::I64, 1);
                    stack_push(&mut builder, &mut stack_depth, local_count, v);
                }
                OP_CONST => {
                    let idx = code[i] as usize; i += 1;
                    let n = match &func.chunk.constants[idx] {
                        VmValue::Int(n) => *n,
                        _ => return Err("JIT: 비-Int 상수".into()),
                    };
                    let v = builder.ins().iconst(types::I64, n);
                    stack_push(&mut builder, &mut stack_depth, local_count, v);
                }
                OP_DUP => {
                    let v = stack_peek(&mut builder, stack_depth, local_count);
                    stack_push(&mut builder, &mut stack_depth, local_count, v);
                }
                OP_POP => {
                    stack_pop(&mut builder, &mut stack_depth, local_count);
                }

                OP_LOAD_LOCAL => {
                    let slot = code[i] as usize; i += 1;
                    let v = builder.use_var(Variable::new(slot));
                    stack_push(&mut builder, &mut stack_depth, local_count, v);
                }
                OP_STORE_LOCAL => {
                    let slot = code[i] as usize; i += 1;
                    let v = stack_pop(&mut builder, &mut stack_depth, local_count);
                    builder.def_var(Variable::new(slot), v);
                }

                OP_ADD => binary_int_op(&mut builder, &mut stack_depth, local_count,
                    |b, l, r| b.ins().iadd(l, r)),
                OP_SUB => binary_int_op(&mut builder, &mut stack_depth, local_count,
                    |b, l, r| b.ins().isub(l, r)),
                OP_MUL => binary_int_op(&mut builder, &mut stack_depth, local_count,
                    |b, l, r| b.ins().imul(l, r)),
                OP_DIV => binary_int_op(&mut builder, &mut stack_depth, local_count,
                    |b, l, r| b.ins().sdiv(l, r)),
                OP_MOD => binary_int_op(&mut builder, &mut stack_depth, local_count,
                    |b, l, r| b.ins().srem(l, r)),

                OP_NEG => {
                    let v = stack_pop(&mut builder, &mut stack_depth, local_count);
                    let neg = builder.ins().ineg(v);
                    stack_push(&mut builder, &mut stack_depth, local_count, neg);
                }
                OP_NOT => {
                    let v    = stack_pop(&mut builder, &mut stack_depth, local_count);
                    let zero = builder.ins().iconst(types::I64, 0);
                    let b    = builder.ins().icmp(IntCC::Equal, v, zero);
                    let r    = builder.ins().uextend(types::I64, b);
                    stack_push(&mut builder, &mut stack_depth, local_count, r);
                }

                OP_EQ | OP_NE | OP_LT | OP_LE | OP_GT | OP_GE => {
                    let cc = match op {
                        OP_EQ => IntCC::Equal,
                        OP_NE => IntCC::NotEqual,
                        OP_LT => IntCC::SignedLessThan,
                        OP_LE => IntCC::SignedLessThanOrEqual,
                        OP_GT => IntCC::SignedGreaterThan,
                        OP_GE => IntCC::SignedGreaterThanOrEqual,
                        _     => unreachable!(),
                    };
                    let r   = stack_pop(&mut builder, &mut stack_depth, local_count);
                    let l   = stack_pop(&mut builder, &mut stack_depth, local_count);
                    let cmp = builder.ins().icmp(cc, l, r);
                    let ext = builder.ins().uextend(types::I64, cmp);
                    stack_push(&mut builder, &mut stack_depth, local_count, ext);
                }

                OP_JUMP => {
                    let offset    = i16::from_le_bytes([code[i], code[i + 1]]); i += 2;
                    let target_ip = (i as isize + offset as isize) as usize;
                    let target    = ip_to_block[&target_ip];
                    builder.ins().jump(target, &[]);
                    terminated = true;
                }
                OP_JUMP_FALSE => {
                    let offset    = i16::from_le_bytes([code[i], code[i + 1]]); i += 2;
                    let false_ip  = (i as isize + offset as isize) as usize;
                    let true_ip   = i;
                    let cond      = stack_pop(&mut builder, &mut stack_depth, local_count);
                    let zero      = builder.ins().iconst(types::I64, 0);
                    let is_false  = builder.ins().icmp(IntCC::Equal, cond, zero);
                    let fb = ip_to_block[&false_ip];
                    let tb = ip_to_block[&true_ip];
                    // brif: if is_false → jump to false_block, else → true_block
                    builder.ins().brif(is_false, fb, &[], tb, &[]);
                    terminated = true;
                }

                OP_RETURN => {
                    let v = if stack_depth > 0 {
                        stack_pop(&mut builder, &mut stack_depth, local_count)
                    } else {
                        builder.ins().iconst(types::I64, 0)
                    };
                    builder.ins().return_(&[v]);
                    terminated = true;
                }

                unknown => return Err(format!("JIT: 비지원 opcode {unknown:#x}")),
            }
        }

        // 함수 끝까지 도달했으면 nil(0) 반환
        if !terminated {
            let v = builder.ins().iconst(types::I64, 0);
            builder.ins().return_(&[v]);
        }

        builder.seal_all_blocks();
        builder.finalize();
    } // builder + builder_ctx drop

    state
        .module
        .define_function(func_id, &mut ctx)
        .map_err(|e| e.to_string())?;
    state.module.clear_context(&mut ctx);
    state
        .module
        .finalize_definitions()
        .map_err(|e| e.to_string())?;

    let raw = state.module.get_finalized_function(func_id);
    let fp: JitFnPtr = unsafe { std::mem::transmute(raw) };
    Ok(fp)
}

/// 이항 정수 연산 헬퍼
fn binary_int_op(
    builder: &mut FunctionBuilder,
    depth: &mut usize,
    base: usize,
    op: impl Fn(&mut FunctionBuilder, Value, Value) -> Value,
) {
    let r = stack_pop(builder, depth, base);
    let l = stack_pop(builder, depth, base);
    let v = op(builder, l, r);
    stack_push(builder, depth, base, v);
}

// ============================================================================
// 공개 API
// ============================================================================

/// 함수를 JIT로 호출한다.
/// JIT 불가능하거나 인자가 Int 가 아니면 `None` → VM 폴백.
pub fn try_jit_call(
    func: &Arc<CompiledFn>,
    args: &[VmValue],
    _span: Span,
) -> Option<Result<VmValue, RuntimeError>> {
    // 모든 인수가 Int 여야 한다
    let mut int_args: Vec<i64> = Vec::with_capacity(args.len());
    for a in args {
        match a {
            VmValue::Int(n) => int_args.push(*n),
            _ => return None,
        }
    }

    let key = Arc::as_ptr(func) as usize;

    with_jit(|state| -> Option<Result<VmValue, RuntimeError>> {
        // 캐시 조회
        if let Some(&fp) = state.cache.get(&key) {
            state.stats.cache_hits += 1;
            let result = unsafe { fp(int_args.as_ptr(), int_args.len()) };
            return Some(Ok(VmValue::Int(result)));
        }

        // can_jit 검사
        if !can_jit(func) {
            state.stats.bailouts += 1;
            return None;
        }

        // 컴파일
        match compile_one(state, func) {
            Ok(fp) => {
                state.cache.insert(key, fp);
                state.stats.compiled += 1;
                let result = unsafe { fp(int_args.as_ptr(), int_args.len()) };
                Some(Ok(VmValue::Int(result)))
            }
            Err(e) => {
                state.stats.bailouts += 1;
                eprintln!("[JIT] 컴파일 실패, VM으로 폴백: {e}");
                None
            }
        }
    })
}
