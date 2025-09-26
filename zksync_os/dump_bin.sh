#!/bin/sh
set -e

USAGE="Usage: $0 --type {server|server-logging-enabled|evm-replay|testing|testing-benchmarking|evm-replay-benchmarking|debug-in-simulator|pectra|multiblock-batch|multiblock-batch-logging-enabled}"
TYPE=""

# Parse --type argument
while [ "$#" -gt 0 ]; do
  case "$1" in
    --type)
      [ "$#" -ge 2 ] || { echo "Missing value for --type"; echo "$USAGE"; exit 2; }
      TYPE="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "$USAGE"
      exit 2
      ;;
  esac
done

# Base features and output names
FEATURES="proving"

# Adjust for server modes
case "$TYPE" in
  server)
    FEATURES="$FEATURES"
    BIN_NAME="server_app.bin"
    ELF_NAME="server_app.elf"
    TEXT_NAME="server_app.text"
    ;;
  server-logging-enabled)
    FEATURES="$FEATURES,print_debug_info"
    BIN_NAME="server_app_logging_enabled.bin"
    ELF_NAME="server_app_logging_enabled.elf"
    TEXT_NAME="server_app_logging_enabled.text"
    ;;
  testing)
    FEATURES="$FEATURES"
    BIN_NAME="testing.bin"
    ELF_NAME="testing.elf"
    TEXT_NAME="testing.text"
    ;;
  testing-benchmarking)
    FEATURES="$FEATURES,proof_running_system/testing,proof_running_system/cycle_marker,proof_running_system/unlimited_native,proof_running_system/p256_precompile"
    BIN_NAME="testing.bin"
    ELF_NAME="testing.elf"
    TEXT_NAME="testing.text"
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
  evm-replay-benchmarking)
    FEATURES="$FEATURES,proof_running_system/unlimited_native,proof_running_system/disable_system_contracts,proof_running_system/cycle_marker,proof_running_system/prevrandao,proof_running_system/evm_refunds"
    BIN_NAME="evm_replay.bin"
    ELF_NAME="evm_replay.elf"
    TEXT_NAME="evm_replay.text"
    ;;
  pectra)
    FEATURES="$FEATURES,proof_running_system/pectra"
    BIN_NAME="testing.bin"
    ELF_NAME="testing.elf"
    TEXT_NAME="testing.text"
    ;;
  multiblock-batch)
    FEATURES="$FEATURES,proof_running_system/multiblock-batch"
    BIN_NAME="multiblock_batch.bin"
    ELF_NAME="multiblock_batch.elf"
    TEXT_NAME="multiblock_batch.text"
    ;;
  multiblock-batch-logging-enabled)
    FEATURES="$FEATURES,proof_running_system/multiblock-batch,print_debug_info"
    BIN_NAME="multiblock_batch_logging_enabled.bin"
    ELF_NAME="multiblock_batch_logging_enabled.elf"
    TEXT_NAME="multiblock_batch_logging_enabled.text"
    ;;
  *)
    echo "Invalid --type: $TYPE"
    echo "$USAGE"
    exit 1
    ;;
esac

# Clean up only the artifacts for this mode
rm -f "$BIN_NAME" "$ELF_NAME" "$TEXT_NAME"

# Build
cargo build --features "$FEATURES" --release

# Produce and rename outputs
cargo objcopy --features "$FEATURES" --release -- -O binary "$BIN_NAME"
cargo objcopy --features "$FEATURES" --release -- -R .text "$ELF_NAME"
cargo objcopy --features "$FEATURES" --release -- -O binary --only-section=.text "$TEXT_NAME"

# Summary
echo "Built [$TYPE] with features: $FEATURES"
echo "→ $BIN_NAME"
echo "→ $ELF_NAME"
echo "→ $TEXT_NAME"
