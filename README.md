# ADI Stack OS

ADI Stack OS is a new state transition function implementation that enables multiple execution environments (EVM, EraVM, Wasm, etc.) to operate within a unified ecosystem. It is implemented in Rust and compiled into a RISC-V binary, which can later be proven using the `adi-stack-airbender-system`.

ADI Stack utilizing the [MatterLabs zkOS stack](https://github.com/matter-labs/zksync-os)

## Documentation

The most recent documentation can be found here:

- [In-repo documentation](./docs/README.md)
- [Repository structure](./docs/repository_structure.md)

## How to build

### One-time setup
Run the following commands to prepare your environment (only needed once):

```bash
rustup target add riscv32i-unknown-none-elf
cargo install cargo-binutils && rustup component add llvm-tools-preview
```

ZKsync OS is built for two targets:
- Your host platform, used by the sequencer to execute blocks/batches.
- RISC-V, used to produce a binary that is later proved by a RISC-V prover (Airbender).

### Build for host platform
```bash
cargo build --workspace
```

### Build for RISC-V

#### Reproducible build

To build RISC-V binaries in a reproducible way, use the following command (requires Docker):

```bash
./zksync_os/reproduce/reproduce.sh
```

#### Manual build

Navigate to the `zksync_os` directory and run:
```bash
./dump_bin.sh --type for-tests
```

For other build modes, check `zksync_os/dump_bin.sh`.

## Testing

### Integration and unit tests

Build `zksync_os` first for tests that execute the proof-running path:
```bash
cd zksync_os && ./dump_bin.sh --type for-tests
```

Run workspace tests:
```bash
cargo test --workspace
```

Note: `cargo test --workspace` does **not** include directories excluded in root `Cargo.toml` (for example `zksync_os`, `tests/fuzzer`, `tests/evm_tester`, `tests/instances/eth_runner`).

Integration tests are mainly organized in `tests/instances/` using the rig in `tests/rig/`.

Examples:
```bash
cargo test -p transactions -- --nocapture
cargo test -p precompiles -- --nocapture
```

Unit tests are organized in corresponding modules.

#### Proving-enabled test execution

By default, many tests execute the RISC-V simulator to validate the behavior of the RISC-V-compiled ZKsync OS binary, but they do not generate full proofs. You can run proving by enabling the `e2e_proving` feature while running tests, for example:
```bash
cargo test --features e2e_proving -p transactions -- --nocapture
```

Alternatively, you can prove tests manually using this guide: [Proving tests with CLI](./docs/proving_tests_with_cli.md).

### EVM tester

The repository also contains the EVM tester setup in `tests/evm_tester`.

Prepare fixtures once:
```bash
cd tests/evm_tester && ./download_ethereum_fixtures.sh
```

Run the tester:
```bash
cd tests/evm_tester && cargo run --bin evm-tester --release --features zksync_os_forward_system/no_print
```

## Policies

- [Contribution policy](CONTRIBUTING.md)

## License

ZKsync OS is distributed under the terms of either

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/blog/license/mit/>)

at your option.

## Official Links

- [Website](https://adi.foundation)
- [Docs](https://docs.adi.foundation/)
- [Github](https://github.com/ADI-Foundation-Labs/)
- [X](https://x.com/adi_foundation)
- [X for ADI Chain announcements](https://x.com/ADIChain_)
- [LinkedIn](https://www.linkedin.com/company/adifoundation/)
- [Discord](http://discord.gg/adi-foundation)
