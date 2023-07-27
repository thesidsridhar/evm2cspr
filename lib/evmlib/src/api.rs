// This is free and unencumbered software released into the public domain.

use crate::{
    env::Env,
    json_utils::{decode::transform_json_call_data, encode::encode_return_data_as_json},
    ops::{ENV, EVM},
    state::Word,
};

#[no_mangle]
pub static mut _abi_buffer: [u8; 0xFFFF] = [1; 0xFFFF]; // FIXME

#[no_mangle]
pub unsafe fn _evm_init(_table_offset: u32, chain_id: u64, balance: u64) {
    #[cfg(feature = "cspr")]
    {
        // TODO
    }

    #[cfg(not(feature = "cspr"))]
    {
        let mut args = std::env::args();

        let wasm_file = args.next().unwrap();
        assert!(wasm_file.ends_with(".wasm") || wasm_file.ends_with(".wasi"));

        let mut arg = args.next();
        loop {
            match &arg {
                None => break, // no more arguments
                Some(s) => {
                    match s.as_str() {
                        "--trace" => {
                            EVM.trace_level = args
                                .next()
                                .and_then(|level| level.parse::<u8>().ok())
                                .expect("trace level expected to be 'u8'");
                        }
                        _ => break, // start of actual arguments
                    }
                }
            }
            arg = args.next();
        }

        ENV.call_data = match arg {
            None => Vec::new(), // no call data given
            Some(input) => {
                if input.starts_with("0x") {
                    match hex::decode(&input[2..]) {
                        Err(err) => panic!("{}", err), // FIXME
                        Ok(bytes) => bytes,
                    }
                } else if input.starts_with("{") || input.starts_with("[") {
                    input.into_bytes() // JSON
                } else {
                    panic!("expected JSON or hexadecimal input, but got: {}", input);
                    // FIXME
                }
            }
        };

        EVM.call_value = match args.next() {
            None => crate::state::ZERO,
            Some(s) => Word::from(s.parse::<u64>().unwrap_or(0)), // TODO: support decimal point as well
        };
        //eprintln!("_evm_init: call_data={:?} call_value={:?}", ENV.call_data, EVM.call_value);
    }

    EVM.chain_id = Word::from(chain_id);
    EVM.self_balance = Word::from(balance);
}

#[no_mangle]
pub unsafe fn _evm_call(
    selector: u32,
    param_names_off: usize, // relative to _abi_buffer
    param_names_len: usize,
    param_types_off: usize, // relative to _abi_buffer
    param_types_len: usize,
) {
    let raw_call_data = ENV.call_data();

    let param_names_ptr: *mut u8 = _abi_buffer
        .as_mut_ptr()
        .offset(param_names_off.try_into().unwrap());
    let param_names = std::slice::from_raw_parts(param_names_ptr, param_names_len);

    let param_types_ptr: *mut u8 = _abi_buffer
        .as_mut_ptr()
        .offset(param_types_off.try_into().unwrap());
    let param_types = std::slice::from_raw_parts(param_types_ptr, param_types_len);

    let call_data = if param_names.is_empty() {
        let mut call_data: Vec<u8> = vec![0; 4 + raw_call_data.len()];
        call_data[0..4].copy_from_slice(&selector.to_be_bytes());
        call_data[4..].copy_from_slice(raw_call_data);
        call_data
    } else {
        // TODO: support raw call data as well
        // TODO: check that sufficient arguments were provided
        transform_json_call_data(selector, param_names, param_types, raw_call_data).unwrap()
    };

    #[cfg(all(feature = "cspr", not(test)))]
    {
        ENV.call_data = Some(call_data);
    }
    #[cfg(any(not(feature = "cspr"), test))]
    {
        ENV.call_data = call_data;
    }
}

/// Posts the return value from the execution, translating into JSON using the ABI
#[no_mangle]
pub unsafe fn _evm_post_exec(
    output_types_off: usize, // relative to _abi_buffer
    output_types_len: usize,
) {
    // If there is an ABI given then we will try to encode output into JSON
    if output_types_len > 0 {
        let output_types_ptr: *mut u8 = _abi_buffer
            .as_mut_ptr()
            .offset(output_types_off.try_into().unwrap());
        let output_types = std::slice::from_raw_parts(output_types_ptr, output_types_len);
        let json_return_data =
            encode_return_data_as_json(output_types, ENV.get_return_data(), ENV.get_exit_status())
                .unwrap();
        ENV.overwrite_return_data(json_return_data);
    }
    ENV.post_exec();
}

#[no_mangle]
pub unsafe fn _evm_pop_u32() -> u32 {
    EVM.stack.pop().as_u32()
}

#[no_mangle]
pub unsafe fn _evm_push_u32(x: u32) {
    EVM.stack.push(x.into())
}

#[no_mangle]
pub unsafe fn _evm_set_pc(pc: u32) {
    #[cfg(feature = "pc")]
    EVM.program_counter = pc;
}

#[no_mangle]
pub unsafe fn _evm_burn_gas(gas: u32) {
    // TODO gas value should be u64
    EVM.burn_gas(gas as u64)
}
