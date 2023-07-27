CARGO = cargo
LIPO = lipo
WASM_STRIP = wasm-strip

all: evm2cspr

release:                   \
  evm2cspr-macos-arm       \
  evm2cspr-macos-x86       \
  evm2cspr-windows-arm.exe \
  evm2cspr-windows-x86.exe \
  evm2cspr-linux-arm       \
  evm2cspr-linux-x86

EVM2cspr_FILES = $(wildcard bin/evm2cspr/src/*.rs)
EVMLIB_FILES = $(shell find lib/evmlib/src -name "*.rs")
RELOOPER_FILES = $(shell find lib/relooper/src -name "*.rs")

evm2cspr: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) $(RELOOPER_FILES) Makefile evmlib.wasi evmlib.wasm
	echo $^
	$(CARGO) build --package=evm2cspr
	ln -sf target/debug/evm2cspr evm2cspr

evm2cspr-macos: evm2cspr-macos-arm evm2cspr-macos-x86
	$(LIPO) -create -output $@ $^

evm2cspr-macos-arm: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) Makefile evmlib.wasi evmlib.wasm
	$(CARGO) build --package=evm2cspr --release --target=aarch64-apple-darwin
	ln -sf target/aarch64-apple-darwin/release/evm2cspr $@

evm2cspr-macos-x86: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) Makefile evmlib.wasi evmlib.wasm
	$(CARGO) build --package=evm2cspr --release --target=x86_64-apple-darwin
	ln -sf target/x86_64-apple-darwin/release/evm2cspr $@

evm2cspr-windows-arm.exe: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) Makefile evmlib.wasi evmlib.wasm
	#$(CARGO) build --package=evm2cspr --release --target=aarch64-pc-windows-msvc
	#ln -sf target/aarch64-pc-windows-msvc/release/evm2cspr.exe $@

evm2cspr-windows-x86.exe: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) Makefile evmlib.wasi evmlib.wasm
	$(CARGO) build --package=evm2cspr --release --target=x86_64-pc-windows-gnu
	ln -sf target/x86_64-pc-windows-gnu/release/evm2cspr.exe $@

evm2cspr-linux-arm: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) Makefile evmlib.wasi evmlib.wasm
	$(CARGO) build --package=evm2cspr --release --target=aarch64-unknown-linux-musl
	ln -sf target/aarch64-unknown-linux-musl/release/evm2cspr $@

evm2cspr-linux-x86: bin/evm2cspr/Cargo.toml $(EVM2cspr_FILES) Makefile evmlib.wasi evmlib.wasm
	$(CARGO) build --package=evm2cspr --release --target=x86_64-unknown-linux-musl
	ln -sf target/x86_64-unknown-linux-musl/release/evm2cspr $@

evmlib.wasm: lib/evmlib/Cargo.toml $(EVMLIB_FILES) Makefile
	$(CARGO) build --package=evmlib --release --target=wasm32-unknown-unknown --no-default-features --features=gas,pc,cspr
	$(WASM_STRIP) target/wasm32-unknown-unknown/release/$@
	ln -sf target/wasm32-unknown-unknown/release/$@ $@

evmlib.wasi: lib/evmlib/Cargo.toml $(EVMLIB_FILES) Makefile
	$(CARGO) build --package=evmlib --release --target=wasm32-wasi --no-default-features --features=gas,pc
	$(WASM_STRIP) target/wasm32-wasi/release/evmlib.wasm
	ln -sf target/wasm32-wasi/release/evmlib.wasm $@

check:
	$(CARGO) test --workspace -- --nocapture --test-threads=1 --color=always

clean:
	$(CARGO) clean
	rm -f evm2cspr evm2cspr-macos evm2cspr-*-* evmlib.wasi evmlib.wasm

.PHONY: check clean
