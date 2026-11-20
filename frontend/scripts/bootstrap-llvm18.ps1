param(
  [string]$ToolRoot,
  [ValidateSet('All', 'Download', 'Verify', 'Extract')]
  [string]$Phase = 'All'
)

$ErrorActionPreference = "Stop"
$frontend = Split-Path $PSScriptRoot -Parent
$repo = Split-Path $frontend -Parent
if (-not $ToolRoot) {
  $ToolRoot = Join-Path $repo ".build-tools"
}

$llvm = Join-Path $ToolRoot "clang+llvm-18.1.8-x86_64-pc-windows-msvc"
$libclang = Join-Path $llvm "bin\libclang.dll"
if (Test-Path $libclang) {
  Write-Host "LLVM 18 already ready: $llvm"
  $llvm
  exit 0
}

New-Item -ItemType Directory -Force -Path $ToolRoot | Out-Null
$archive = Join-Path $ToolRoot "clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz"
$expectedSha256 = "22C5907DB053026CC2A8FF96D21C0F642A90D24D66C23C6D28EE7B1D572B82E8"
$timer = [Diagnostics.Stopwatch]::StartNew()
Write-Host "[$([DateTime]::UtcNow.ToString('o'))] LLVM phase: $Phase"
if ($Phase -in @('All', 'Download') -and -not (Test-Path $archive)) {
  Write-Host "Downloading portable LLVM 18 (required for whisper-rs Windows bindings)…"
  gh release download llvmorg-18.1.8 `
    --repo llvm/llvm-project `
    --pattern "clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz" `
    --dir $ToolRoot
  if ($LASTEXITCODE -ne 0) { throw "Failed to download LLVM 18" }
}
if (!(Test-Path $archive)) { throw "LLVM archive missing: run the Download phase first" }
Write-Host "Archive size: $((Get-Item $archive).Length) bytes"
if ($Phase -eq 'Download') {
  Write-Host "Download complete in $($timer.Elapsed.TotalSeconds.ToString('F1')) seconds"
  exit 0
}
Write-Host "[$([DateTime]::UtcNow.ToString('o'))] Checking LLVM archive SHA-256"
$actualSha256 = (Get-FileHash $archive -Algorithm SHA256).Hash
if ($actualSha256 -ne $expectedSha256) {
  throw "LLVM 18 archive checksum mismatch: $actualSha256"
}
Write-Host "SHA-256 verified: $actualSha256"
if ($Phase -eq 'Verify') {
  Write-Host "Verification complete in $($timer.Elapsed.TotalSeconds.ToString('F1')) seconds"
  exit 0
}

$sevenZip = Get-Command 7z -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1
if (!$sevenZip -and $env:ProgramFiles) {
  $candidate = Join-Path $env:ProgramFiles '7-Zip/7z.exe'
  if (Test-Path $candidate) { $sevenZip = $candidate }
}
if (!$sevenZip) { throw 'LLVM extraction requires 7-Zip (7z on PATH or Program Files/7-Zip/7z.exe)' }

# Windows' bundled tar stalled on this verified XZ archive in CI. Use 7-Zip
# for both layers; do not pipe binary TAR data through PowerShell's text pipeline.
$scratch = Join-Path $ToolRoot ('.llvm-extract-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
  Write-Host "[$([DateTime]::UtcNow.ToString('o'))] Decompressing XZ with $sevenZip"
  & $sevenZip x $archive "-o$scratch" -y -bsp0 | Out-Host
  if ($LASTEXITCODE -ne 0) { throw 'Failed to decompress LLVM XZ archive' }
  $tarFiles = @(Get-ChildItem $scratch -Filter '*.tar' -File)
  if ($tarFiles.Count -ne 1) { throw 'Expected exactly one TAR inside LLVM XZ archive' }
  Write-Host "[$([DateTime]::UtcNow.ToString('o'))] XZ complete; extracting TAR ($($tarFiles[0].Length) bytes)"
  & $sevenZip x $tarFiles[0].FullName "-o$ToolRoot" -y -bsp0 | Out-Host
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $libclang)) {
    throw 'Failed to extract LLVM TAR payload'
  }
} finally {
  # Only this invocation's intermediate TAR; keep the verified source archive.
  Remove-Item $scratch -Recurse -Force
}

Write-Host "LLVM 18 ready in $($timer.Elapsed.TotalSeconds.ToString('F1')) seconds: $llvm"
$llvm
