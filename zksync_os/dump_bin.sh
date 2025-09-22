#!/bin/sh
set -e

# Default mode
TYPE="singleblock-batch"

# Parse --type argument
while [ "$#" -gt 0 ]; do
  case "$1" in
    --type)
      TYPE="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--type singleblock-batch|singleblock-batch-logging-enabled|multiblock-batch|multiblock-batch-logging-enabled|evm-replay|benchmarking|evm-replay-benchmarking|evm-replay-with-logs|debug-in-simulator|pectra]"
      exit 1
      ;;
  esac
done

# Base features and output names
FEATURES="proving"

# TODO: we can remove no_print everywhere, except proving binary
# Adjust for server modes
case "$TYPE" in
  singleblock-batch)
    BIN_NAME="app.bin" # TODO: rename to singleblock-batch?
    ELF_NAME="app.elf"
    TEXT_NAME="app.text"
    ;;
  singleblock-batch-logging-enabled)
    FEATURES="$FEATURES,print_debug_info"
    BIN_NAME="app_logging_enabled.bin"
    ELF_NAME="app_logging_enabled.elf"
    TEXT_NAME="app_logging_enabled.text"
    ;;
  multiblock-batch)
    FEATURES="$FEATURES,multiblock-batch"
    BIN_NAME="multiblock_batch.bin"
    ELF_NAME="multiblock_batch.elf"
    TEXT_NAME="multiblock_batch.text"
    ;;
  multiblock-batch-logging-enabled)
    FEATURES="$FEATURES,multiblock-batch,print_debug_info"
    BIN_NAME="multiblock_batch.bin"
    ELF_NAME="multiblock_batch.elf"
    TEXT_NAME="multiblock_batch.text"
    ;;
  benchmarking)
    FEATURES="$FEATURES,proof_running_system/cycle_marker,proof_running_system/unlimited_native,proof_running_system/p256_precompile"
    BIN_NAME="app.bin"
    ELF_NAME="app.elf"
    TEXT_NAME="app.text"
    ;;
  debug-in-simulator)
    FEATURES="$FEATURES,print_debug_info,proof_running_system/cycle_marker,proof_running_system/p256_precompile"
    BIN_NAME="app_debug.bin"
    ELF_NAME="app_debug.elf"
    TEXT_NAME="app_debug.text"
    ;;
  evm-replay)
    FEATURES="$FEATURES,proof_running_system/disable_system_contracts,proof_running_system/prevrandao,proof_running_system/evm_refunds"
    BIN_NAME="evm_replay.bin"
    ELF_NAME="evm_replay.elf"
    TEXT_NAME="evm_replay.text"
    ;;
  evm-replay-with-logs)
    FEATURES="$FEATURES,proof_running_system/disable_system_contracts,proof_running_system/prevrandao,print_debug_info"
    BIN_NAME="evm_replay_with_logs.bin"
    ELF_NAME="evm_replay_with_logs.elf"
    TEXT_NAME="evm_replay_with_logs.text"
    ;;
  evm-replay-benchmarking)
    FEATURES="$FEATURES,proof_running_system/unlimited_native,proof_running_system/disable_system_contracts,proof_running_system/cycle_marker,proof_running_system/prevrandao,proof_running_system/evm_refunds"
    BIN_NAME="evm_replay.bin"
    ELF_NAME="evm_replay.elf"
    TEXT_NAME="evm_replay.text"
    ;;
  pectra)
    FEATURES="$FEATURES,pectra,evm-compatibility,prevrandao,evm_refunds,disable_system_contracts"
    BIN_NAME="app.bin"
    ELF_NAME="app.elf"
    TEXT_NAME="app.text"
    ;;
  pectra-debug)
    FEATURES="$FEATURES,pectra,evm-compatibility,prevrandao,evm_refunds,disable_system_contracts,print_debug_info"
    BIN_NAME="app.bin"
    ELF_NAME="app.elf"
    TEXT_NAME="app.text"
    ;;
  *)
    echo "Invalid --type: $TYPE"
    echo "Valid types are: singleblock-batch, singleblock-batch-logging-enabled, multiblock-batch, multiblock-batch-logging-enabled, evm-replay, benchmarking, evm-replay-benchmarking, debug-in-simulator"
    exit 1
    ;;
esac

# Clean up only the artifacts for this mode
rm -f "$BIN_NAME" "$ELF_NAME" "$TEXT_NAME"

# Build
cargo build --features "$FEATURES" --release

# cargo objdump --features "$FEATURES" --release -v -- -d

# Produce and rename outputs
cargo objcopy --features "$FEATURES" --release -- -O binary "$BIN_NAME"
cargo objcopy --features "$FEATURES" --release -- -R .text "$ELF_NAME"
cargo objcopy --features "$FEATURES" --release -- -O binary --only-section=.text "$TEXT_NAME"

# Summary
echo "Built [$TYPE] with features: $FEATURES"
echo "→ $BIN_NAME"
echo "→ $ELF_NAME"
echo "→ $TEXT_NAME"
