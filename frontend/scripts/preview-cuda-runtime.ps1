# Shared by the Preview packager and its fixture-based dependency tests.
function Get-PreviewPeDependencies([string]$Path) {
  $report = @(& dumpbin.exe /nologo /dependents $Path 2>&1)
  if ($LASTEXITCODE -ne 0) { throw "Cannot inspect PE dependencies: $Path`n$report" }
  @($report | ForEach-Object {
    if ("$_" -match '^\s+([^\s]+\.dll)\s*$') { $matches[1] }
  } | Sort-Object -Unique)
}

function Copy-PreviewCudaRuntime([string]$Toolkit, [string]$Stage, [string]$SystemDirectory) {
  $cudaDirectories = @((Join-Path $Toolkit 'bin/x64'), (Join-Path $Toolkit 'bin'))
  $required = @('cudart64_13.dll', 'cublas64_13.dll', 'cublasLt64_13.dll')
  $pending = [Collections.Generic.Queue[string]]::new()
  foreach ($name in $required) {
    $source = $cudaDirectories | ForEach-Object { Join-Path $_ $name } | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (!$source) { throw "Missing CUDA 13 runtime: $name under $Toolkit" }
    Copy-Item $source (Join-Path $Stage $name) -Force
    $pending.Enqueue($name)
  }
  $seen = @{}
  $imports = [ordered]@{}
  while ($pending.Count -gt 0) {
    $name = $pending.Dequeue()
    if ($seen.ContainsKey($name)) { continue }
    $seen[$name] = $true
    $dependencies = @(Get-PreviewPeDependencies (Join-Path $Stage $name))
    $imports[$name] = $dependencies
    foreach ($dependency in $dependencies) {
      $local = Join-Path $Stage $dependency
      if (Test-Path $local) { $pending.Enqueue($dependency); continue }
      $source = $cudaDirectories | ForEach-Object { Join-Path $_ $dependency } | Where-Object { Test-Path $_ } | Select-Object -First 1
      if ($source) {
        Copy-Item $source $local -Force
        $pending.Enqueue($dependency)
      } elseif ($dependency -eq 'nvcuda.dll' -or $dependency -match '^(api|ext)-ms-' -or (Test-Path (Join-Path $SystemDirectory $dependency))) {
        # nvcuda.dll belongs to the user's NVIDIA driver, never the toolkit.
        continue
      } else {
        throw "Unresolved CUDA runtime dependency: $name -> $dependency"
      }
    }
  }
  # CUDA 13's minimal Windows packages install an extensionless LICENSE.
  $license = @('LICENSE', 'EULA.txt', 'doc/EULA.txt') | ForEach-Object { Join-Path $Toolkit $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
  if (!$license) { throw "CUDA toolkit license missing under $Toolkit" }
  Copy-Item $license (Join-Path $Stage 'CUDA-EULA.txt') -Force
  return $imports
}
