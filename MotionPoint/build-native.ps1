$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
cargo build --release --manifest-path (Join-Path $root "Cargo.toml")

$platformDir = Join-Path $root "native\windows-x86_64"
New-Item -ItemType Directory -Force -Path $platformDir | Out-Null
Copy-Item -Force -LiteralPath (Join-Path $root "target\release\motion_point.dll") -Destination (Join-Path $platformDir "motion_point.dll")
