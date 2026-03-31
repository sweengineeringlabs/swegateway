#!/usr/bin/env bash
set -euo pipefail
cargo build -p swe-gateway
cargo test -p swe-gateway
