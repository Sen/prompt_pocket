#!/usr/bin/env bash
set -euo pipefail

APP_NAME="prompt-pocket"
BIN_DIR="bin"

cargo build --release
mkdir -p "$BIN_DIR"
cp "target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
cargo clean

echo "Saved release binary to $BIN_DIR/$APP_NAME"
