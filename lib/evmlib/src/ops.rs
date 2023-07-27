// This is free and unencumbered software released into the public domain.

use ethnum::{AsU256, I256};
use std::{
    convert::TryInto,
    ops::{Not, Shl, Shr},
};
use ux::*;

use crate::{
    env::{Address, Env, EvmLog},
    hash_provider::HashProvider,
    state::{Machine, Memory, Stack, Word, MAX_STACK_DEPTH, ONE, ZERO},
};

const KECCAK_EMPTY: Word = Word::from_words(
    0xc5d2460186f7233c927e7db2dcc703c0,
    0xe500b653ca82273b7bfad8045d85a470,
);

pub(crate) static mut EVM: Machine = Machine {
    trace_level: 0,
    #[cfg(feature = "pc")]
    program_counter: 0,
    #[cfg(feature = "gas")]
    gas_used: 0,
    gas_limit: 10_000_000,
    gas_price: 0, // gas is ultimately paid in $cspr
    stack: Stack {
        depth: 0,
        slots: [ZERO; MAX_STACK_DEPTH],
    },
    memory: Memory { bytes: Vec::new() },
    call_value: Word::ZERO,
    code: Vec::new(),
    chain_id: ZERO,
    self_balance: ZERO,
};

#[cfg(all(feature = "cspr", not(test)))]
pub(crate) static mut ENV: crate::cspr_runtime::csprRuntime = crate::cspr_runtime::csprRuntime {
    call_data: None,
    storage_cache: None,
    address_cache: None,
    origin_cache: None,
    caller_cache: None,
    exit_status: None,
    return_data: Vec::new(),
};

#[cfg(any(not(feature = "cspr"), test))]
pub(crate) static mut ENV: crate::env::mock::MockEnv = crate::env::mock::MockEnv {
    call_data: Vec::new(),
    address: [0u8; 20],
    origin: [0u8; 20],
    caller: [0u8; 20],
    block_height: 0,
    timestamp: 0,
    storage: None,
    logs: Vec::new(),
    return_data: Vec::new(),
    exit_status: None,
};

#[cfg(all(feature = "cspr", not(test)))]
pub(crate) type Hasher = crate::cspr_runtime::csprRuntime;

#[cfg(any(not(feature = "cspr"), test))]
pub(crate) type Hasher = crate::hash_provider::Native;

macro_rules! trace {
    ($($t:tt)*) => {{
        #[cfg(target_os = "wasi")]
        if EVM.trace_level >= 1 {
            #[cfg(feature = "pc")]
            eprint!("@{:04x}\t", EVM.program_counter);
            eprintln!($($t)*);
            if EVM.trace_level >= 2 {
                eprint!("\tstack: ");
                EVM.stack.dump();
                if EVM.trace_level >= 3 {
                    eprintln!("\tmemory:");
                    EVM.memory.dump();
                }
            }
        }
    }};
}

#[no_mangle]
pub unsafe fn stop() {
    EVM.burn_gas(0);
    EVM.stack.clear();
    ENV.value_return(&[]);
    trace!("STOP");
}

#[no_mangle]
pub unsafe fn add() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a + b);
    trace!("ADD a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn mul() {
    EVM.burn_gas(5);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a * b);
    trace!("MUL a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn sub() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a - b);
    trace!("SUB a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn div() {
    EVM.burn_gas(5);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(if b == ZERO { ZERO } else { a / b });
    trace!("DIV a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn sdiv() {
    EVM.burn_gas(5);
    let a = EVM.stack.pop().as_i256();
    let b = EVM.stack.pop().as_i256();
    EVM.stack.push(if b == I256::ZERO {
        ZERO
    } else {
        (a / b).as_u256()
    });
    trace!("SDIV a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn r#mod() {
    EVM.burn_gas(5);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(if b == ZERO { ZERO } else { a % b });
    trace!("MOD a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn smod() {
    EVM.burn_gas(5);
    let a = EVM.stack.pop().as_i256();
    let b = EVM.stack.pop().as_i256();
    EVM.stack.push(if b == I256::ZERO {
        ZERO
    } else {
        (a % b).as_u256()
    });
    trace!("SMOD a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn addmod() {
    EVM.burn_gas(8);
    // TODO: need to use 512-bit arithmetic here to prevent overflow before taking the modulus
    let (a, b, n) = EVM.stack.pop3();
    let result = if n == ZERO { ZERO } else { (a + b) % n };
    EVM.stack.push(result);
    trace!("ADDMOD a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn mulmod() {
    EVM.burn_gas(8);
    // TODO: need to use 512-bit arithmetic here to prevent overflow before taking the modulus
    let (a, b, n) = EVM.stack.pop3();
    let result = if n == ZERO { ZERO } else { (a * b) % n };
    EVM.stack.push(result);
    trace!("MULMOD a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn exp() {
    EVM.burn_gas(10);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a.pow(b.try_into().unwrap()));
    trace!("EXP a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn signextend() {
    EVM.burn_gas(5);
    let (op1, op2) = EVM.stack.pop2();
    let result = if op1 < ethnum::U256::new(32) {
        // `as_u32` works since op1 < 32
        let bit_index = (8 * op1.as_u32() + 7) as usize;
        let word = if bit_index < 128 {
            op2.low()
        } else {
            op2.high()
        };
        let bit = word & (1 << (bit_index % 128)) != 0;
        let mask = (ONE << bit_index) - ONE;
        if bit {
            op2 | !mask
        } else {
            op2 & mask
        }
    } else {
        op2
    };
    EVM.stack.push(result);
    trace!("SIGNEXTEND op1={} op2={}", op1, op2);
}

#[no_mangle]
pub unsafe fn lt() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(if a < b { ONE } else { ZERO });
    trace!("LT a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn gt() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(if a > b { ONE } else { ZERO });
    trace!("GT a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn slt() {
    EVM.burn_gas(3);
    let a = EVM.stack.pop().as_i256();
    let b = EVM.stack.pop().as_i256();
    EVM.stack.push(if a < b { ONE } else { ZERO });
    trace!("SLT a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn sgt() {
    EVM.burn_gas(3);
    let a = EVM.stack.pop().as_i256();
    let b = EVM.stack.pop().as_i256();
    EVM.stack.push(if a > b { ONE } else { ZERO });
    trace!("SGT a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn eq() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(if a == b { ONE } else { ZERO });
    trace!("EQ a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn iszero() {
    EVM.burn_gas(3);
    let x = EVM.stack.pop();
    EVM.stack.push(if x == ZERO { ONE } else { ZERO });
    trace!("ISZERO x={}", x);
}

#[no_mangle]
pub unsafe fn and() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a & b);
    trace!("AND a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn or() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a | b);
    trace!("OR a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn xor() {
    EVM.burn_gas(3);
    let (a, b) = EVM.stack.pop2();
    EVM.stack.push(a ^ b);
    trace!("XOR a={} b={}", a, b);
}

#[no_mangle]
pub unsafe fn not() {
    EVM.burn_gas(3);
    let x = EVM.stack.pop();
    EVM.stack.push(x.not());
    trace!("NOT x={}", x);
}

#[no_mangle]
pub unsafe fn byte() {
    EVM.burn_gas(3);
    let (index, word) = EVM.stack.pop2();
    let result = if index > 31 {
        ZERO
    } else {
        let bytes = word.to_be_bytes();
        Word::from(bytes[index.as_usize()])
    };
    EVM.stack.push(result);
    trace!("BYTE index={} word={}", index, word);
}

#[no_mangle]
pub unsafe fn shl() {
    EVM.burn_gas(3);
    let (shift, value) = EVM.stack.pop2();
    let result = if value == ZERO || shift > Word::from(255u8) {
        ZERO
    } else {
        value.shl(shift)
    };
    EVM.stack.push(result);
    trace!("SHL shift={} value={}", shift, value);
}

#[no_mangle]
pub unsafe fn shr() {
    EVM.burn_gas(3);
    let (shift, value) = EVM.stack.pop2();
    let result = if value == ZERO || shift > Word::from(255u8) {
        ZERO
    } else {
        value.shr(shift)
    };
    EVM.stack.push(result);
    trace!("SHR shift={} value={}", shift, value);
}

#[no_mangle]
pub unsafe fn sar() {
    EVM.burn_gas(3);
    let (shift, value) = EVM.stack.pop2();
    let signed_value = value.as_i256();
    let result = if signed_value == I256::ZERO || shift > Word::from(255u8) {
        if signed_value.is_positive() {
            ZERO
        } else {
            I256::from(-1).as_u256()
        }
    } else {
        // Cast is safe since we checked if shift is less than 255
        let shift = shift.as_u32();
        if signed_value.is_positive() {
            value.shr(shift).as_u256()
        } else {
            signed_value
                .overflowing_sub(I256::ONE)
                .0
                .shr(shift)
                .overflowing_add(I256::ONE)
                .0
                .as_u256()
        }
    };
    EVM.stack.push(result);
    trace!("SAR shift={} value={}", shift, value);
}

#[no_mangle]
pub unsafe fn sha3() {
    EVM.burn_gas(30);
    let (offset, size) = EVM.stack.pop2();
    let size = as_usize_or_oog(size);
    let result = if size == 0 {
        KECCAK_EMPTY
    } else {
        let offset = as_usize_or_oog(offset);
        EVM.memory.resize(offset + size);
        let slice = EVM.memory.slice(offset, size);
        let hash = Hasher::keccak256(slice);

        Word::from_be_bytes(hash)
    };
    EVM.stack.push(result);
    trace!("SHA3 offset={} size={}", offset, size);
}

#[no_mangle]
pub unsafe fn address() {
    EVM.burn_gas(2);
    let address = ENV.address();
    EVM.stack.push(address_to_u256(&address));
    trace!("ADDRESS");
}

#[no_mangle]
pub unsafe fn balance() {
    EVM.burn_gas(100);
    let address_u256 = EVM.stack.pop();
    let address = u256_to_address(address_u256);
    let result = if address == ENV.address() {
        EVM.self_balance
    } else {
        ZERO
    };
    EVM.stack.push(result);
    trace!("BALANCE address={}", address_u256);
}

#[no_mangle]
pub unsafe fn origin() {
    EVM.burn_gas(2);
    let address = ENV.origin();
    EVM.stack.push(address_to_u256(&address));
    trace!("ORIGIN");
}

#[no_mangle]
pub unsafe fn caller() {
    EVM.burn_gas(2);
    let address = ENV.caller();
    EVM.stack.push(address_to_u256(&address));
    trace!("CALLER");
}

#[no_mangle]
pub unsafe fn callvalue() {
    EVM.burn_gas(2);
    EVM.stack.push(EVM.call_value);
    trace!("CALLVALUE");
}

#[no_mangle]
pub unsafe fn calldataload() {
    EVM.burn_gas(3);
    // Note: if the value on the stack is larger than usize::MAX then
    // `as_usize` will return `usize::MAX`, and this is ok because that
    // is the largest possible calldata size.
    let index = EVM.stack.pop().as_usize();
    let call_data = ENV.call_data();
    let call_data_len = call_data.len();
    let result = if index < call_data_len {
        // Result is at most 32 bytes
        let slice_size = (call_data_len - index).min(32);
        let mut slice_bytes = [0u8; 32];
        slice_bytes[0..slice_size].copy_from_slice(&call_data[index..(index + slice_size)]);
        Word::from_be_bytes(slice_bytes)
    } else {
        ZERO
    };

    EVM.stack.push(result);
    trace!("CALLDATALOAD index={}", index);
}

#[no_mangle]
pub unsafe fn calldatasize() {
    EVM.burn_gas(2);
    EVM.stack.push(Word::from(ENV.call_data_len() as u32));
    trace!("CALLDATASIZE");
}

#[no_mangle]
pub unsafe fn calldatacopy() {
    EVM.burn_gas(3);
    let (dest_offset, offset, size) = EVM.stack.pop3();
    data_copy(dest_offset, offset, size, ENV.call_data());
    trace!(
        "CALLDATACOPY dest_offset={} offset={} size={}",
        dest_offset,
        offset,
        size
    );
}

#[no_mangle]
pub unsafe fn codesize() {
    EVM.burn_gas(2);
    EVM.stack.push(Word::from(EVM.code.len() as u32));
    trace!("CODESIZE");
}

#[no_mangle]
pub unsafe fn codecopy() {
    EVM.burn_gas(3);
    let (dest_offset, offset, size) = EVM.stack.pop3();
    data_copy(dest_offset, offset, size, &EVM.code);
    trace!(
        "CODECOPY dest_offset={} offset={} size={}",
        dest_offset,
        offset,
        size
    );
}

#[no_mangle]
pub unsafe fn gasprice() {
    EVM.burn_gas(2);
    EVM.stack.push(Word::from(EVM.gas_price));
    trace!("GASPRICE");
}

#[no_mangle]
pub unsafe fn extcodesize() {
    EVM.burn_gas(100);
    let address_u256 = EVM.stack.pop();
    let address = u256_to_address(address_u256);
    // The only code we know about is our own.
    // TODO: in a world with `CALL`, how would this opcode work?
    let result = if address == ENV.address() {
        Word::from(EVM.code.len() as u64)
    } else {
        ZERO
    };
    EVM.stack.push(result);
    trace!("EXTCODESIZE address={}", address_u256);
}

#[no_mangle]
pub unsafe fn extcodecopy() {
    EVM.burn_gas(100);
    let (address_u256, dest_offset, offset, size) = EVM.stack.pop4();
    let address = u256_to_address(address_u256);
    // See note in `extcodesize` about why we only act on our own address
    if address == ENV.address() {
        data_copy(dest_offset, offset, size, &EVM.code);
    } else {
        data_copy(dest_offset, offset, size, &[]);
    }
    trace!(
        "EXTCODECOPY address={} dest_offset={} offset={} size={}",
        address_u256,
        dest_offset,
        offset,
        size
    );
}

#[no_mangle]
pub unsafe fn returndatasize() {
    EVM.burn_gas(2);
    // Without any implementation of `CALL` there can be no sub-context
    // to have produced return data used in a larger execution.
    // We could consider using cspr's promise API as the previous return data,
    // but that may not be what we want depending on the design of `CALL`.
    // For now we will simply always return `ZERO`.
    EVM.stack.push(ZERO);
    trace!("RETURNDATASIZE");
}

#[no_mangle]
pub unsafe fn returndatacopy() {
    EVM.burn_gas(3);
    let (dest_offset, offset, size) = EVM.stack.pop3();
    // See note in `returndatasize` about why we assume the return data is always empty.
    data_copy(dest_offset, offset, size, &[]);
    trace!(
        "RETURNDATACOPY dest_offset={} offset={} size={}",
        dest_offset,
        offset,
        size
    );
}

#[no_mangle]
pub unsafe fn extcodehash() {
    EVM.burn_gas(100);
    let address_u256 = EVM.stack.pop();
    let address = u256_to_address(address_u256);
    // See note in `extcodesize` about why we only act on our own address
    let result = if address == ENV.address() {
        let hash = Hasher::keccak256(&EVM.code);
        Word::from_be_bytes(hash)
    } else {
        ZERO
    };
    EVM.stack.push(result);
    trace!("EXTCODEHASH address={}", address_u256);
}

#[no_mangle]
pub unsafe fn blockhash() {
    EVM.burn_gas(20);
    EVM.stack.push(ZERO); // TODO: cspr SDK
    trace!("BLOCKHASH");
}

#[no_mangle]
pub unsafe fn coinbase() {
    EVM.burn_gas(2);
    EVM.stack.push(ZERO); // TODO: cspr SDK
    trace!("COINBASE");
}

#[no_mangle]
pub unsafe fn timestamp() {
    EVM.burn_gas(2);
    let number = ENV.timestamp();
    EVM.stack.push(Word::from(number));
    trace!("TIMESTAMP");
}

#[no_mangle]
pub unsafe fn number() {
    EVM.burn_gas(2);
    let number = ENV.block_height();
    EVM.stack.push(Word::from(number));
    trace!("NUMBER");
}

#[no_mangle]
pub unsafe fn difficulty() {
    EVM.burn_gas(2);
    EVM.stack.push(ZERO);
    trace!("DIFFICULTY");
}

#[no_mangle]
pub unsafe fn gaslimit() {
    EVM.burn_gas(2);
    EVM.stack.push(Word::from(EVM.gas_limit));
    trace!("GASLIMIT");
}

#[no_mangle]
pub unsafe fn chainid() {
    EVM.burn_gas(2);
    EVM.stack.push(EVM.chain_id);
    trace!("CHAINID");
}

#[no_mangle]
pub unsafe fn selfbalance() {
    EVM.burn_gas(5);
    EVM.stack.push(EVM.self_balance);
    trace!("SELFBALANCE");
}

#[no_mangle]
pub unsafe fn basefee() {
    EVM.burn_gas(2);
    EVM.stack.push(ZERO);
    trace!("BASEFEE");
}

#[no_mangle]
pub unsafe fn pop() {
    EVM.burn_gas(2);
    let _tos = EVM.stack.pop();
    trace!("POP tos={}", _tos);
}

#[no_mangle]
pub unsafe fn mload() {
    EVM.burn_gas(3);
    let offset = EVM.stack.pop();
    // TODO: gas cost for memory resize (reads resize the memory too)
    let value = EVM.memory.load_word(offset.try_into().unwrap());
    EVM.stack.push(value);
    trace!("MLOAD offset={}", offset);
}

#[no_mangle]
pub unsafe fn mstore() {
    EVM.burn_gas(3);
    // TODO: gas cost for memory resize
    let (offset, value) = EVM.stack.pop2();
    EVM.memory.store_word(offset.try_into().unwrap(), value);
    trace!("MSTORE offset={} value={}", offset, value);
}

#[no_mangle]
pub unsafe fn mstore8() {
    EVM.burn_gas(3);
    let (offset, value) = (EVM.stack.pop(), EVM.stack.pop() & 0xFF);
    // TODO: gas cost for memory resize
    EVM.memory
        .store_byte(offset.try_into().unwrap(), value.try_into().unwrap());
    trace!("MSTORE8 offset={} value={}", offset, value);
}

#[no_mangle]
pub unsafe fn sload() {
    EVM.burn_gas(100);
    // TODO: dynamic hot/cold gas cost
    let key = EVM.stack.pop();
    let value = ENV.storage_read(key);
    EVM.stack.push(value);
    trace!("SLOAD key={}", key);
}

#[no_mangle]
pub unsafe fn sstore() {
    EVM.burn_gas(100);
    // TODO: dynamic hot/cold gas cost
    let (key, value) = EVM.stack.pop2();
    ENV.storage_write(key, value);
    trace!("SSTORE key={} value={}", key, value);
}

#[no_mangle]
pub unsafe fn jump() {
    // We only do JUMP gas cost accounting here, the actual branch is
    // synthesized by the compiler.
    EVM.burn_gas(8);
    trace!("JUMP");
}

#[no_mangle]
pub unsafe fn pc() {
    EVM.burn_gas(2);
    #[cfg(feature = "pc")]
    EVM.stack.push(Word::from(EVM.program_counter));
    #[cfg(not(feature = "pc"))]
    EVM.stack.push(ZERO); // --fno-program-counter
    trace!("PC");
}

#[no_mangle]
pub unsafe fn msize() {
    EVM.burn_gas(2);
    EVM.stack.push(Word::from(EVM.memory.size() as u64));
    trace!("MSIZE");
}

#[no_mangle]
pub unsafe fn gas() {
    EVM.burn_gas(2);
    EVM.stack.push(Word::from(EVM.gas_limit - EVM.gas_used)); // TODO: --fno-gas-accounting
    trace!("GAS");
}

#[no_mangle]
pub unsafe fn jumpdest() {
    trace!("JUMPDEST");
}

#[no_mangle]
pub unsafe fn push1(word: u8) {
    EVM.burn_gas(3);
    EVM.stack.push(Word::from(word));
    trace!("PUSH1 0x{:02x}", word);
}

#[no_mangle]
pub unsafe fn push2(word: u16) {
    push4(word.into())
}

#[no_mangle]
pub unsafe fn push3(word: u24) {
    push4(word.into())
}

#[no_mangle]
pub unsafe fn push4(word: u32) {
    EVM.burn_gas(3);
    EVM.stack.push(Word::from(word));
    trace!("PUSH4 0x{:04x}", word);
}

#[no_mangle]
pub unsafe fn push5(word: u40) {
    push8(word.into())
}

#[no_mangle]
pub unsafe fn push6(word: u48) {
    push8(word.into())
}

#[no_mangle]
pub unsafe fn push7(word: u56) {
    push8(word.into())
}

#[no_mangle]
pub unsafe fn push8(word: u64) {
    EVM.burn_gas(3);
    EVM.stack.push(Word::from(word));
    trace!("PUSH8 0x{:08x}", word);
}

#[no_mangle]
pub unsafe fn push9(word: /*u72*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push10(word: /*u80*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push11(word: /*u88*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push12(word: /*u96*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push13(word: /*u104*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push14(word: /*u112*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push15(word: /*u120*/ u128) {
    push16(word)
}

#[no_mangle]
pub unsafe fn push16(word: u128) {
    EVM.burn_gas(3);
    EVM.stack.push(Word::from_words(0, word));
    trace!("PUSH16 0x{:16x}", word);
}

#[no_mangle]
pub unsafe fn push17(word_0: u64, word_1: u64, word_2: u8) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push18(word_0: u64, word_1: u64, word_2: u16) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push19(word_0: u64, word_1: u64, word_2: u24) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push20(word_0: u64, word_1: u64, word_2: u32) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push21(word_0: u64, word_1: u64, word_2: u40) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push22(word_0: u64, word_1: u64, word_2: u48) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push23(word_0: u64, word_1: u64, word_2: u56) {
    push24(word_0, word_1, word_2.into())
}

#[no_mangle]
pub unsafe fn push24(word_0: u64, word_1: u64, word_2: u64) {
    push32(word_0, word_1, word_2, 0);
}

#[no_mangle]
pub unsafe fn push25(word_0: u64, word_1: u64, word_2: u64, word_3: u8) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push26(word_0: u64, word_1: u64, word_2: u64, word_3: u16) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push27(word_0: u64, word_1: u64, word_2: u64, word_3: u24) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push28(word_0: u64, word_1: u64, word_2: u64, word_3: u32) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push29(word_0: u64, word_1: u64, word_2: u64, word_3: u40) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push30(word_0: u64, word_1: u64, word_2: u64, word_3: u48) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push31(word_0: u64, word_1: u64, word_2: u64, word_3: u56) {
    push32(word_0, word_1, word_2, word_3.into())
}

#[no_mangle]
pub unsafe fn push32(word_0: u64, word_1: u64, word_2: u64, word_3: u64) {
    EVM.burn_gas(3);
    let mut bytes: [u8; 32] = [0; 32];
    bytes[0..8].copy_from_slice(&word_0.to_le_bytes());
    bytes[8..16].copy_from_slice(&word_1.to_le_bytes());
    bytes[16..24].copy_from_slice(&word_2.to_le_bytes());
    bytes[24..32].copy_from_slice(&word_3.to_le_bytes());
    EVM.stack.push(Word::from_le_bytes(bytes));
    trace!("PUSH32"); // TODO: trace!("PUSH32 0x{:32x}", word);
}

#[no_mangle]
pub unsafe fn dup1() {
    EVM.burn_gas(3);
    EVM.stack.push(EVM.stack.peek());
    trace!("DUP1");
}

#[no_mangle]
pub unsafe fn dup2() {
    dup(2)
}

#[no_mangle]
pub unsafe fn dup3() {
    dup(3)
}

#[no_mangle]
pub unsafe fn dup4() {
    dup(4)
}

#[no_mangle]
pub unsafe fn dup5() {
    dup(5)
}

#[no_mangle]
pub unsafe fn dup6() {
    dup(6)
}

#[no_mangle]
pub unsafe fn dup7() {
    dup(7)
}

#[no_mangle]
pub unsafe fn dup8() {
    dup(8)
}

#[no_mangle]
pub unsafe fn dup9() {
    dup(9)
}

#[no_mangle]
pub unsafe fn dup10() {
    dup(10)
}

#[no_mangle]
pub unsafe fn dup11() {
    dup(11)
}

#[no_mangle]
pub unsafe fn dup12() {
    dup(12)
}

#[no_mangle]
pub unsafe fn dup13() {
    dup(13)
}

#[no_mangle]
pub unsafe fn dup14() {
    dup(14)
}

#[no_mangle]
pub unsafe fn dup15() {
    dup(15)
}

#[no_mangle]
pub unsafe fn dup16() {
    dup(16)
}

unsafe fn dup(n: u8) {
    assert!((1..=16).contains(&n));
    EVM.burn_gas(3);
    EVM.stack.push(EVM.stack.peek_n(n as usize - 1));
    trace!("DUP{}", n);
}

#[no_mangle]
pub unsafe fn swap1() {
    swap(1)
}

#[no_mangle]
pub unsafe fn swap2() {
    swap(2)
}

#[no_mangle]
pub unsafe fn swap3() {
    swap(3)
}

#[no_mangle]
pub unsafe fn swap4() {
    swap(4)
}

#[no_mangle]
pub unsafe fn swap5() {
    swap(5)
}

#[no_mangle]
pub unsafe fn swap6() {
    swap(6)
}

#[no_mangle]
pub unsafe fn swap7() {
    swap(7)
}

#[no_mangle]
pub unsafe fn swap8() {
    swap(8)
}

#[no_mangle]
pub unsafe fn swap9() {
    swap(9)
}

#[no_mangle]
pub unsafe fn swap10() {
    swap(10)
}

#[no_mangle]
pub unsafe fn swap11() {
    swap(11)
}

#[no_mangle]
pub unsafe fn swap12() {
    swap(12)
}

#[no_mangle]
pub unsafe fn swap13() {
    swap(13)
}

#[no_mangle]
pub unsafe fn swap14() {
    swap(14)
}

#[no_mangle]
pub unsafe fn swap15() {
    swap(15)
}

#[no_mangle]
pub unsafe fn swap16() {
    swap(16)
}

unsafe fn swap(n: u8) {
    assert!((1..=16).contains(&n));
    EVM.burn_gas(3);
    EVM.stack.swap(n.into());
    trace!("SWAP{}", n);
}

#[no_mangle]
pub unsafe fn log0() {
    EVM.burn_gas(375);
    let (offset, size) = EVM.stack.pop2();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    let log = EvmLog {
        address: ENV.address(),
        topics: &[],
        data,
    };
    ENV.log(log);
    trace!("LOG0 offset={} size={}", offset, size);
}

#[no_mangle]
pub unsafe fn log1() {
    EVM.burn_gas(750);
    let (offset, size) = EVM.stack.pop2();
    let topic = EVM.stack.pop();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    let log = EvmLog {
        address: ENV.address(),
        topics: &[topic],
        data,
    };
    ENV.log(log);
    trace!("LOG1 offset={} size={} topic={}", offset, size, topic);
}

#[no_mangle]
pub unsafe fn log2() {
    EVM.burn_gas(1125);
    let (offset, size) = EVM.stack.pop2();
    let (topic1, topic2) = EVM.stack.pop2();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    let log = EvmLog {
        address: ENV.address(),
        topics: &[topic1, topic2],
        data,
    };
    ENV.log(log);
    trace!(
        "LOG2 offset={} size={} topics={{{}, {}}}",
        offset,
        size,
        topic1,
        topic2
    );
}

#[no_mangle]
pub unsafe fn log3() {
    EVM.burn_gas(1500);
    let (offset, size) = EVM.stack.pop2();
    let (topic1, topic2, topic3) = EVM.stack.pop3();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    let log = EvmLog {
        address: ENV.address(),
        topics: &[topic1, topic2, topic3],
        data,
    };
    ENV.log(log);
    trace!(
        "LOG3 offset={} size={} topics={{{}, {}, {}}}",
        offset,
        size,
        topic1,
        topic2,
        topic3
    );
}

#[no_mangle]
pub unsafe fn log4() {
    EVM.burn_gas(1875);
    let (offset, size) = EVM.stack.pop2();
    let (topic1, topic2, topic3, topic4) = EVM.stack.pop4();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    let log = EvmLog {
        address: ENV.address(),
        topics: &[topic1, topic2, topic3, topic4],
        data,
    };
    ENV.log(log);
    trace!(
        "LOG4 offset={} size={} topics={{{}, {}, {}, {}}}",
        offset,
        size,
        topic1,
        topic2,
        topic3,
        topic4
    );
}

#[no_mangle]
pub unsafe fn create() {
    EVM.burn_gas(32000);
    trace!("CREATE");
    todo!("CREATE") // TODO
}

#[no_mangle]
pub unsafe fn call() {
    EVM.burn_gas(100);
    trace!("CALL");
    todo!("CALL") // TODO
}

#[no_mangle]
pub unsafe fn callcode() {
    EVM.burn_gas(100);
    trace!("CALLCODE");
    todo!("CALLCODE") // TODO
}

#[no_mangle]
pub unsafe fn r#return() {
    EVM.burn_gas(0);
    let (offset, size) = EVM.stack.pop2();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    ENV.value_return(data);
    // There is no host function to successfully terminate execution, so
    // the compiler will insert a WebAssembly RETURN instruction here.
    trace!("RETURN offset={} size={}", offset, size);
}

#[no_mangle]
pub unsafe fn delegatecall() {
    EVM.burn_gas(100);
    trace!("DELEGATECALL");
    todo!("DELEGATECALL") // TODO
}

#[no_mangle]
pub unsafe fn create2() {
    EVM.burn_gas(32000);
    trace!("CREATE2");
    todo!("CREATE2") // TODO
}

#[no_mangle]
pub unsafe fn staticcall() {
    EVM.burn_gas(100);
    trace!("STATICCALL");
    todo!("STATICCALL") // TODO
}

#[no_mangle]
pub unsafe fn revert() {
    EVM.burn_gas(0);
    let (offset, size) = EVM.stack.pop2();
    let data = EVM.memory.slice(offset.as_usize(), size.as_usize());
    ENV.revert(data);
    trace!("REVERT offset={} size={}", offset, size);
}

#[no_mangle]
pub unsafe fn invalid() {
    // `INVALID` is "Equivalent to REVERT (since Byzantium fork) with 0,0 as stack
    // parameters, except that all the gas given to the current context is consumed."
    EVM.burn_gas(EVM.gas_limit);
    ENV.revert(&[]);
    trace!("INVALID");
}

#[no_mangle]
pub unsafe fn selfdestruct() {
    EVM.burn_gas(5000);
    trace!("SELFDESTRUCT");
    todo!("SELFDESTRUCT") // TODO: state reset
}

fn as_usize_or_oog(word: Word) -> usize {
    if word > Word::new(usize::MAX as u128) {
        unsafe {
            ENV.exit_oog();
            unreachable!("OOG");
        }
    } else {
        word.as_usize()
    }
}

fn address_to_u256(address: &Address) -> Word {
    let mut buf = [0u8; 32];
    buf[12..32].copy_from_slice(address);
    Word::from_be_bytes(buf)
}

fn u256_to_address(word: Word) -> Address {
    let mut buf = [0u8; 20];
    buf[4..20].copy_from_slice(&word.low().to_be_bytes());
    buf[0..4].copy_from_slice(&word.high().to_be_bytes()[12..16]);
    buf
}

unsafe fn data_copy(dest_offset: Word, offset: Word, size: Word, source: &[u8]) {
    // Cannot copy more than `usize::MAX` within any gas limit
    let size = as_usize_or_oog(size);

    // Nothing to copy; we're done
    if size == 0 {
        return;
    }

    // Cannot allocate more than `usize::MAX` bytes of memory within any gas limit
    let dest_offset = as_usize_or_oog(dest_offset);

    // See note in calldataload about usize cast of calldata offset.
    let offset = offset.as_usize();

    // TODO: gas cost for memory resize

    let data_len = source.len();
    // Bytes that are within the call_data range
    let on_data_bytes = if offset > data_len {
        &[]
    } else if size > data_len - offset {
        &source[offset..]
    } else {
        &source[offset..(offset + size)]
    };
    if !on_data_bytes.is_empty() {
        EVM.memory.store_slice(dest_offset, on_data_bytes);
    }

    // Bytes outside the calldata are implicitly 0
    let on_data_size = on_data_bytes.len();
    let remaining_size = size - on_data_size;
    let dest_offset = dest_offset + on_data_size;
    if remaining_size > 0 {
        EVM.memory.store_zeros(dest_offset, remaining_size);
    }
}
