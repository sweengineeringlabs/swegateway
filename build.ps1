$ErrorActionPreference = "Stop"
cargo build -p swe-gateway
cargo test -p swe-gateway
