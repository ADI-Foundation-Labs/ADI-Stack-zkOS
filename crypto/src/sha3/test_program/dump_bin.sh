#!/bin/bash

set -e

cargo objcopy --release --features keccak_f1600_test -- -O binary app_keccak_simple.bin
cargo objcopy --release --features bad_keccak_f1600_test -- -O binary app_keccak_bad.bin
cargo objcopy --release --features hash_chain_test -- -O binary app_keccak_bench.bin
cargo objcopy --release --features mini_digest_test -- -O binary app_keccak_complex.bin
