#!/usr/bin/env bash
set -euo pipefail
echo "Setting up development environment..."
rustup update
cargo fetch
