#!/bin/sh
./dump_bin.sh
./dump_bin.sh --type evm-replay
./dump_bin.sh --type server-logging-enabled
./dump_bin.sh --type server
./update_protocol_hash.sh
