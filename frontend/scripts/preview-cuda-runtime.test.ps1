# Dependency-graph fixtures require neither Windows nor a CUDA installation.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'preview-cuda-runtime.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) "meetily-cuda-test-$([Guid]::NewGuid())"
$toolkit = Join-Path $root 'toolkit'
$stage = Join-Path $root 'stage'
$system = Join-Path $root 'system'
$script:imports = @{}
$script:dumpbinFails = $false
$previousExitCode = $global:LASTEXITCODE
function dumpbin.exe($NoLogo, $Dependents, $Path) {
  $global:LASTEXITCODE = if ($script:dumpbinFails) { 1 } else { 0 }
  'Dump of file fixture'
  '  Image has the following dependencies:'
  foreach ($name in $script:imports[(Split-Path $Path -Leaf).ToLowerInvariant()]) { "    $name" }
  '  Summary'
}
function Assert-Throws([scriptblock]$Action, [string]$Message) {
  $caught = $null
  try { & $Action | Out-Null } catch { $caught = $_.Exception.Message }
  if (!$caught -or !$caught.Contains($Message)) { throw "Expected '$Message', received '$caught'" }
}
function Reset-Fixture {
  if (Test-Path $root) { Remove-Item $root -Recurse -Force }
  New-Item -ItemType Directory -Force "$toolkit/bin/x64", $stage, $system | Out-Null
  foreach ($name in @('cudart64_13.dll', 'cublas64_13.dll', 'cublasLt64_13.dll', 'nvJitLink_130_0.dll')) {
    Set-Content "$toolkit/bin/x64/$name" "fixture:$name"
  }
  Set-Content "$stage/vcruntime140.dll" 'redistributable'
  Set-Content "$system/KERNEL32.dll" 'system'
  Set-Content "$toolkit/LICENSE" 'CUDA license fixture'
  $script:imports = @{
    'cudart64_13.dll' = @('KERNEL32.dll', 'api-ms-win-core-libraryloader-l1-2-0.dll', 'nvcuda.dll')
    'cublas64_13.dll' = @('cublasLt64_13.dll', 'vcruntime140.dll')
    'cublaslt64_13.dll' = @('nvJitLink_130_0.dll')
    'nvjitlink_130_0.dll' = @('cudart64_13.dll')
    'vcruntime140.dll' = @('KERNEL32.dll')
  }
}
try {
  Reset-Fixture
  $graph = Copy-PreviewCudaRuntime $toolkit $stage $system
  foreach ($name in @('cudart64_13.dll', 'cublas64_13.dll', 'cublasLt64_13.dll', 'nvJitLink_130_0.dll', 'vcruntime140.dll', 'CUDA-EULA.txt')) {
    if (!(Test-Path "$stage/$name")) { throw "Required fixture not staged: $name" }
  }
  if ($graph.Count -ne 5) { throw 'Dependency cycle was not traversed exactly once per DLL' }
  foreach ($name in @('nvcuda.dll', 'KERNEL32.dll')) {
    if (Test-Path "$stage/$name") { throw "Driver/system DLL should not be redistributed: $name" }
  }
  if ((Get-Content "$stage/CUDA-EULA.txt") -ne 'CUDA license fixture') { throw 'License was not preserved' }
  Write-Host 'PASS: bin/x64 runtime, transitive dependency, cycle, CRT, license and driver/system exclusions'

  Reset-Fixture
  Move-Item "$toolkit/bin/x64/*.dll" "$toolkit/bin"
  Move-Item "$toolkit/LICENSE" "$toolkit/EULA.txt"
  $null = Copy-PreviewCudaRuntime $toolkit $stage $system
  if (!(Test-Path "$stage/nvJitLink_130_0.dll")) { throw 'Legacy bin fallback failed' }
  Write-Host 'PASS: legacy bin fallback'

  Reset-Fixture
  Remove-Item "$toolkit/bin/x64/cublasLt64_13.dll"
  Assert-Throws { Copy-PreviewCudaRuntime $toolkit $stage $system } 'Missing CUDA 13 runtime: cublasLt64_13.dll'
  Write-Host 'PASS: missing required runtime fails'

  Reset-Fixture
  Remove-Item "$toolkit/bin/x64/nvJitLink_130_0.dll"
  Assert-Throws { Copy-PreviewCudaRuntime $toolkit $stage $system } 'Unresolved CUDA runtime dependency:'
  Write-Host 'PASS: missing transitive dependency fails'

  Reset-Fixture
  Remove-Item "$toolkit/LICENSE"
  Assert-Throws { Copy-PreviewCudaRuntime $toolkit $stage $system } 'CUDA toolkit license missing'
  Write-Host 'PASS: missing license fails'

  $script:imports['fixture.exe'] = @('CUBLAS64_13.dll', 'cublas64_13.dll', 'cudart64_13.dll')
  $parsed = @(Get-PreviewPeDependencies 'fixture.exe')
  if ($parsed.Count -ne 2 -or $parsed -notcontains 'cublas64_13.dll') { throw 'Import normalization failed' }
  $script:dumpbinFails = $true
  Assert-Throws { Get-PreviewPeDependencies 'fixture.exe' } 'Cannot inspect PE dependencies:'
  Write-Host 'PASS: import normalization and dumpbin failure'
} finally {
  Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
  Remove-Item Function:dumpbin.exe
  $global:LASTEXITCODE = $previousExitCode
}
