#!/bin/bash
set -euo pipefail

# prep
pip3 install matplotlib || pip3 install matplotlib --break-system-packages
rustup target add riscv32i-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools-preview
rustup component add rust-src
echo 'make sure you're on your feature branch or this won't work!'
base=$(git merge-base origin/main HEAD)
head=$(git branch --show-current)

# switch to base branch before divergence
git checkout $base --recurse-submodules --force

# build zkos binaries on base branch
# also fix problematic import
# sed -i '' -e 's|^prover = { git = "https://github.com/matter-labs/zksync-airbender", features = \["prover"\] }$|prover = { git = "https://github.com/matter-labs/zksync-airbender", features = ["prover"], tag = "v0.3.1" }|' tests/binary_checker/Cargo.toml
(cd zksync_os && ./dump_bin.sh --type benchmarking)
(cd zksync_os && ./dump_bin.sh --type evm-replay-benchmarking)

# benchmark the base branch
for dir in tests/instances/eth_runner/blocks/*; do
    blk=$(basename "$dir")
    MARKER_PATH=$(pwd)/base_block_${blk}.bench cargo run -p eth_runner --release -j 3 --features rig/no_print,rig/cycle_marker,rig/unlimited_native -- single-run --block-dir "$dir" > base_block_${blk}.out
done
# MARKER_PATH=$(pwd)/base_precompiles.bench cargo test --release -j 3 --features rig/no_print,precompiles/cycle_marker,rig/unlimited_native -p precompiles -- test_precompiles

# switch to current branch upgrades
git checkout --force $head

# build zkos binaries on new branch
sed -i '' 's/default = \["forward", "secp256k1-static-context"\]/default = ["forward", "secp256k1-static-context", "sha3_tests"]/' crypto/Cargo.toml
(cd zksync_os && ./dump_bin.sh --type benchmarking)
(cd zksync_os && ./dump_bin.sh --type evm-replay-benchmarking)

# benchmark the new branch
# mkdir -p bench_results
# pairs=""
for dir in tests/instances/eth_runner/blocks/*; do
    blk=$(basename "$dir")
    MARKER_PATH=$(pwd)/head_block_${blk}.bench cargo run -p eth_runner --release -j 3 --features rig/no_print,rig/cycle_marker,rig/unlimited_native -- single-run --block-dir "$dir" > head_block_${blk}.out
    # python3 bench_scripts/parse_opcodes.py base_block_${blk}.out bench_results/base_block_${blk}.csv bench_results/base_block_${blk}.png
    # python3 bench_scripts/parse_opcodes.py head_block_${blk}.out bench_results/head_block_${blk}.csv bench_results/head_block_${blk}.png
    # if [ -z "$pairs" ]; then
    #     pairs="(\"block_${blk}\", \"base_block_${blk}.bench\", \"head_block_${blk}.bench\", \"run_prepared\")"
    # else
    #     pairs="${pairs},(\"block_${blk}\", \"base_block_${blk}.bench\", \"head_block_${blk}.bench\", \"run_prepared\")"
    # fi
done
# MARKER_PATH=$(pwd)/head_precompiles.bench cargo test --release -j 3 --features rig/no_print,precompiles/cycle_marker,rig/unlimited_native -p precompiles -- test_precompiles

# Output all lines from the benchmark result starting from the "## ..." comparison header.
# Since the output spans multiple lines, we use a heredoc declaration.
# python3 bench_scripts/compare_bench.py "[${pairs}]"
echo "the benchmark report doesn't show cycle count so go look in the .out files that were generated"
