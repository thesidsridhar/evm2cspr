# EVM → CASPER

`evm2cspr` is a project for compiling EVM bytecode into wasm bytecode, with the particular goal of having that wasm artifact be executable on the [Casper blockchain](https://casper.network/en-us/).
For ease of testing locally, `evm2cspr` also currently supports [wasi](https://wasi.dev/) as a target platform.
The wasi output can be run locally using a wasm runtime, for example [wasmtime](https://wasmtime.dev/).
This can be useful for debugging contracts without deploying to cspr.

Even though `evm2cspr` is a general EVM bytecode to wasm bytecode transpiler, the CLI interface accepts a Solidity source file as input for convenience.
The source file is compiled to EVM bytecode using [solc](https://github.com/ethereum/solidity).
This way, evm2cspr is able to access the contract ABI, which allows for the output wasm artifact to feature functions that correspond to those provided in the contract. For instance, the contract in `test/calc.sol` comprises a function `multiply(int a, int b)`, and the resulting wasm artifact will also include a similarly named multiply function that accepts a JSON string as input. The JSON input should be an object with fields matching the names of function arguments (a and b in this case). Along with these ABI-derived functions, there's also a generic execute function that accepts binary input conforming to the typical Solidity ABI format (meaning the initial four bytes are the function signature-derived "selector", while the remaining bytes represent the input arguments encoded using Solidity's ABI format).

## Usage

### Compiling to wasi (for running locally)

```
./evm2cspr INPUT_SOLIDITY_CONTRACT -o OUTPUT_WASM_FILE -b wasi
```

Example:

```console
./evm2cspr test/calc.sol -o calc.wasm -b wasi
```

Running the output in wasmtime:

```console
wasmtime --allow-unknown-exports calc.wasm --invoke multiply -- '{"a":6, "b": 7}'
```

### Compiling to cspr

```
./evm2cspr INPUT_SOLIDITY_CONTRACT -o OUTPUT_WASM_FILE -b cspr
```

Example:

```console
./evm2cspr test/calc.sol -o calc.wasm -b cspr
```


### Help

```console
./evm2cspr --help
```

## Development

### Prerequisites

- Rust toolchain (nightly 2022-09-07)
- Solidity compiler `solc` (0.8.16+)
- `wasm-strip` from WABT

#### Prerequisites on macOS

```console
brew install rustup solidity wabt
```

#### Prerequisites on Ubuntu

```console
curl -sSf https://sh.rustup.rs | sh

sudo apt-add-repository ppa:ethereum/ethereum
sudo apt update
sudo apt install solc

sudo apt install wabt
```

### Development Builds

```console
rustup target add wasm32-wasi
rustup target add wasm32-unknown-unknown
make
./evm2cspr --help
```

## Release

### Prerequisites

- Rust toolchain (nightly 2022-09-07)
- MinGW-w64 (10.0.0+)
- `wasm-strip` from WABT

#### Prerequisites on macOS

```console
brew install rustup mingw-w64 wabt
```

#### Prerequisites on Ubuntu

```console
curl -sSf https://sh.rustup.rs | sh

apt install mingw-w64 wabt
```

### Release Builds

```console
rustup target add wasm32-wasi
rustup target add wasm32-unknown-unknown
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
rustup target add aarch64-pc-windows-msvc
rustup target add x86_64-pc-windows-gnu
rustup target add aarch64-unknown-linux-musl
rustup target add x86_64-unknown-linux-musl
make clean release
```


# evm2cspr
