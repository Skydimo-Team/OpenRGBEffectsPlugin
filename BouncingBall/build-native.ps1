$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
cargo build --release --manifest-path (Join-Path $root "Cargo.toml")

$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$platformDir = Join-Path $root "native\windows-x86_64"
New-Item -ItemType Directory -Force -Path $platformDir | Out-Null
Copy-Item -Force -LiteralPath (Join-Path $targetRoot "release\bouncing_ball.dll") -Destination (Join-Path $platformDir "bouncing_ball.dll")
