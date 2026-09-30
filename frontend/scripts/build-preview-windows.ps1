# CI-only single-backend candidate. Production universal/updater packaging stays in
# build-universal-windows.ps1; this path never signs or publishes a release.
param(
  [ValidateSet('x86_64-pc-windows-msvc')][string]$Target = 'x86_64-pc-windows-msvc',
  [ValidateSet('cpu', 'cuda')][string]$Backend = 'cpu'
)

$ErrorActionPreference = 'Stop'
$frontend = Split-Path $PSScriptRoot -Parent
$repo = Split-Path $frontend -Parent
$tauri = Join-Path $frontend 'src-tauri'
$base = Get-Content (Join-Path $tauri 'tauri.conf.json') -Raw | ConvertFrom-Json
if ($base.version -ne '0.2.18' -or $base.identifier -ne 'com.meetily.ai') {
  throw 'Preview must retain version 0.2.18 and bundle identifier com.meetily.ai'
}
if (!$env:RUNNER_TEMP -or !$env:BUILD_COMMIT -or !$env:VCToolsRedistDir) {
  throw 'Run from the Windows Preview workflow after native prerequisites and tests'
}

$output = Join-Path $repo 'dist/windows-preview'
$stage = Join-Path $tauri 'preview-runtime'
# Prevent a previous CUDA invocation from leaking DLLs into the CPU payload.
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force $output, $stage | Out-Null

# App-local Microsoft redistributable DLLs make this preview runnable on
# machines without VS installed. The production installer has its own redist hook.
foreach ($component in @('Microsoft.VC143.CRT', 'Microsoft.VC143.OpenMP')) {
  $source = Join-Path $env:VCToolsRedistDir "x64/$component"
  if (!(Test-Path $source)) { throw "Missing MSVC redistributables: $source" }
  Copy-Item "$source/*.dll" $stage -Force
}
$features = 'custom-protocol'
$cuda = $null
if ($Backend -eq 'cuda') {
  . (Join-Path $PSScriptRoot 'preview-cuda-runtime.ps1')
  if (!$env:CUDA_PATH) { throw 'CUDA Preview requires CUDA_PATH pointing to CUDA 13.0.2' }
  $nvcc = Join-Path $env:CUDA_PATH 'bin/nvcc.exe'
  if (!(Test-Path $nvcc)) { throw "CUDA compiler missing: $nvcc" }
  $nvccVersion = @(& $nvcc --version)
  if ($LASTEXITCODE -ne 0 -or "$nvccVersion" -notmatch 'release 13\.0,') { throw 'CUDA Preview requires CUDA 13.0' }
  if ($env:CMAKE_CUDA_ARCHITECTURES -ne '86') { throw 'CUDA Preview requires CMAKE_CUDA_ARCHITECTURES=86 (RTX 3080 Laptop)' }
  $env:CUDA_TOOLKIT_ROOT_DIR = $env:CUDA_PATH
  $env:PATH = "$env:CUDA_PATH\bin;$env:CUDA_PATH\bin\x64;$env:PATH"
  $env:NVCC_APPEND_FLAGS = '-std=c++17 -Xcompiler=/Zc:preprocessor -DCCCL_IGNORE_MSVC_TRADITIONAL_PREPROCESSOR_WARNING'
  $null = Get-Command dumpbin.exe -ErrorAction Stop
  $sevenZip = (Get-Command 7z.exe -ErrorAction Stop).Source
  $cudaImports = Copy-PreviewCudaRuntime -Toolkit $env:CUDA_PATH -Stage $stage -SystemDirectory ([Environment]::SystemDirectory)
  $features = 'custom-protocol,cuda'
  $cuda = @{
    compiler_version = $nvccVersion
    architectures = $env:CMAKE_CUDA_ARCHITECTURES
    nvcc_append_flags = $env:NVCC_APPEND_FLAGS
    runtime_imports = $cudaImports
    driver_dependency = 'NVIDIA CUDA 13 compatible driver; nvcuda.dll is not redistributed'
    gpu_inference_verified = $false
  }
}
$resources = [ordered]@{}
foreach ($directory in @('templates', 'resources/diarization', 'binaries/onnxruntime')) {
  $files = @(Get-ChildItem (Join-Path $tauri $directory) -File)
  if ($files.Count -eq 0) { throw "Missing resource files: $directory" }
  foreach ($file in $files) {
    $relative = "$directory/$($file.Name)"
    $resources[$relative] = $relative
  }
}
foreach ($file in Get-ChildItem $stage -File) {
  $resources["preview-runtime/$($file.Name)"] = $file.Name
}

# Tauri merges this override after tauri.windows.conf.json. Null removes the
# release signing command and universal installer template/hooks. Stock NSIS
# installs the selected executable and required resources for the current user.
$override = Join-Path $tauri 'tauri.preview.generated.json'
@{
  build = @{ beforeBuildCommand = '' }
  bundle = @{
    createUpdaterArtifacts = $false
    resources = $resources
    windows = @{
      signCommand = $null
      nsis = @{ template = $null; installerHooks = $null; customLanguageFiles = $null }
    }
  }
} | ConvertTo-Json -Depth 8 | Set-Content $override -Encoding utf8

Push-Location $frontend
try {
  pnpm exec tauri build --target $Target --config $override --bundles nsis -- --locked --no-default-features --features $features
  if ($LASTEXITCODE -ne 0) { throw "$Backend Preview NSIS build failed" }
} finally {
  Pop-Location
  Remove-Item $override -Force
}

$bundles = @(Get-ChildItem (Join-Path $repo "target/$Target/release/bundle/nsis") -Filter '*-setup.exe')
if ($bundles.Count -ne 1) { throw 'Expected exactly one NSIS installer' }
$installer = $bundles[0].FullName
if ((Get-AuthenticodeSignature $installer).Status -ne 'NotSigned') { throw 'Preview installer unexpectedly signed' }

# Verify the actual installed payload, including Tauri resource destination paths.
$installed = Join-Path $env:RUNNER_TEMP 'meetily-preview-installed'
if (Test-Path $installed) { Remove-Item $installed -Recurse -Force }
$process = Start-Process -FilePath $installer -ArgumentList @('/S', "/D=$installed") -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Preview install failed: $($process.ExitCode)" }
$main = Join-Path $installed 'meetily.exe'
foreach ($file in @('meetily.exe', 'llama-helper.exe', 'ffmpeg.exe', 'vcruntime140.dll', 'msvcp140.dll', 'vcomp140.dll',
  'templates/standard_meeting.json', 'resources/diarization/segmentation-3.0-fp16.onnx',
  'resources/diarization/wespeaker-resnet34-LM.onnx', 'resources/diarization/xvec_transform.npz',
  'binaries/onnxruntime/onnxruntime.dll', 'binaries/onnxruntime/onnxruntime_providers_shared.dll',
  'binaries/onnxruntime/DirectML.dll', 'binaries/onnxruntime/onnxruntime-LICENSE.txt',
  'binaries/onnxruntime/DirectML-LICENSE.txt')) {
  $path = Join-Path $installed $file
  if (!(Test-Path $path) -or (Get-Item $path).Length -eq 0) { throw "Missing installed payload: $file" }
}
if ((Get-AuthenticodeSignature $main).Status -ne 'NotSigned') { throw 'Preview application unexpectedly signed' }
if ((Get-Item $main).VersionInfo.ProductVersion -notmatch '^0\.2\.18(?:\.0)?$') { throw 'Installed application version differs' }
foreach ($binary in @($main, (Join-Path $installed 'llama-helper.exe'), (Join-Path $installed 'ffmpeg.exe'))) {
  $stream = [IO.File]::OpenRead($binary)
  $reader = [IO.BinaryReader]::new($stream)
  try {
    if ($reader.ReadUInt16() -ne 0x5a4d) { throw "Missing DOS header: $binary" }
    $stream.Position = 0x3c
    $peOffset = $reader.ReadInt32()
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) {
      throw "Expected x64 PE executable: $binary"
    }
  } finally { $reader.Dispose() }
}
# Tauri patches the NSIS bundle marker, then restores the raw build output.
# Check that exact byte transformation while rejecting any other payload change.
node (Join-Path $PSScriptRoot 'verify-nsis-payload.mjs') (Join-Path $repo "target/$Target/release/meetily.exe") $main
if ($LASTEXITCODE -ne 0) { throw 'Installer application verification failed' }
foreach ($resource in $resources.GetEnumerator()) {
  if ((Get-FileHash (Join-Path $tauri $resource.Key)).Hash -ne (Get-FileHash (Join-Path $installed $resource.Value)).Hash) {
    throw "Installed resource hash mismatch: $($resource.Value)"
  }
}
foreach ($sidecar in @('llama-helper', 'ffmpeg')) {
  if ((Get-FileHash (Join-Path $installed "$sidecar.exe")).Hash -ne (Get-FileHash (Join-Path $tauri "binaries/$sidecar-$Target.exe")).Hash) {
    throw "Installed sidecar hash mismatch: $sidecar"
  }
}
if ($Backend -eq 'cuda') {
  $mainImports = @(Get-PreviewPeDependencies $main)
  # whisper-rs-sys 0.11.1 explicitly links cudart and cuBLAS on Windows.
  # Imports establish compile identity without launching GPU code on the runner.
  foreach ($dependency in @('cudart64_13.dll', 'cublas64_13.dll')) {
    if ($mainImports -notcontains $dependency) { throw "CUDA executable does not import $dependency" }
  }
  foreach ($dependency in @('cudart64_13.dll', 'cublas64_13.dll', 'cublasLt64_13.dll', 'CUDA-EULA.txt')) {
    if (!(Test-Path (Join-Path $installed $dependency))) { throw "Installed CUDA payload missing: $dependency" }
  }
  $sidecarImports = @(Get-PreviewPeDependencies (Join-Path $installed 'llama-helper.exe'))
  if ($sidecarImports -match '(?i)(cuda|cublas|nvrtc|nvjitlink)') { throw 'llama-helper must retain its CPU backend' }
  $cuda.application_imports = $mainImports
  $cuda.cpu_sidecar_imports = $sidecarImports
  $cuda.compile_identity_verified = $true
}

$audio = Join-Path $env:RUNNER_TEMP 'meetily-preview-smoke.m4a'
& (Join-Path $installed 'ffmpeg.exe') -v error -f lavfi -i 'anullsrc=r=48000:cl=mono' -t 0.25 -c:a aac -y $audio
if ($LASTEXITCODE -ne 0 -or (Get-Item $audio).Length -eq 0) { throw 'Installed FFmpeg encode failed' }
& (Join-Path $installed 'ffmpeg.exe') -v error -i $audio -f f32le -acodec pcm_f32le -y "$audio.f32"
if ($LASTEXITCODE -ne 0 -or (Get-Item "$audio.f32").Length -eq 0) { throw 'Installed FFmpeg decode failed' }
$llamaOutput = @('{"type":"ping"}', '{"type":"shutdown"}') | & (Join-Path $installed 'llama-helper.exe')
if ($LASTEXITCODE -ne 0 -or "$llamaOutput" -notmatch '"type":"pong"' -or "$llamaOutput" -notmatch '"type":"goodbye"') {
  throw 'Installed llama-helper protocol smoke test failed'
}

$stem = "Meetily-Actually-Free_$($base.version)_preview-fixes_$($env:BUILD_COMMIT.Substring(0, 12))_x64-$Backend"
Copy-Item $installer (Join-Path $output "$stem-setup.exe")
# The ZIP contains the same verified installed files, allowing qualification
# without replacing an installed executable (the app still shares its data root).
$archive = Join-Path $output "$stem.app.zip"
if ($Backend -eq 'cuda') {
  # Compress-Archive has a 2 GiB per-file limit; CUDA DLLs can exceed it.
  if (Test-Path $archive) { Remove-Item $archive -Force }
  Push-Location $installed
  try {
    & $sevenZip a -tzip -mx=5 $archive '.\*'
    if ($LASTEXITCODE -ne 0) { throw 'CUDA Preview ZIP creation failed' }
    & $sevenZip t $archive
    if ($LASTEXITCODE -ne 0) { throw 'CUDA Preview ZIP integrity check failed' }
  } finally { Pop-Location }
} else {
  Compress-Archive -Path "$installed/*" -DestinationPath $archive -Force
}
$payload = @(Get-ChildItem $installed -Recurse -File | Sort-Object FullName | ForEach-Object {
  @{ path = [IO.Path]::GetRelativePath($installed, $_.FullName).Replace('\', '/'); size = $_.Length; sha256 = (Get-FileHash $_.FullName).Hash.ToLowerInvariant() }
})
@{
  version = $base.version
  bundle_identifier = $base.identifier
  build_commit = $env:BUILD_COMMIT
  event_commit = $env:GITHUB_SHA
  repository = $env:GITHUB_REPOSITORY
  run_url = "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
  target = $Target
  whisper_backend = $Backend
  cargo_features = $features
  cuda = $cuda
  signing = 'unsigned'
  updater_artifacts = $false
  installer_payload_verified = $true
  physical_wasapi_call_verified = $false
  payload = $payload
} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $output 'windows-build-metadata.json') -Encoding utf8
@(
  "node: $(node --version)"
  "pnpm: $(pnpm --version)"
  "rustc: $(rustc --version)"
  "cargo: $(cargo --version)"
  "cmake: $((cmake --version | Select-Object -First 1))"
  if ($Backend -eq 'cuda') { "nvcc: $($nvccVersion -join ' ')" }
) | Set-Content (Join-Path $output 'tool-versions.txt') -Encoding utf8
Get-ChildItem $output -File | Where-Object Name -ne 'SHA256SUMS' | Sort-Object Name | ForEach-Object {
  "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)"
} | Set-Content (Join-Path $output 'SHA256SUMS') -Encoding ascii
@(
  "Candidate commit: $env:BUILD_COMMIT"
  "Unsigned x64 $Backend NSIS installer and app ZIP; version 0.2.18 / com.meetily.ai."
  'Silent installation, resource hashes and sidecar smoke tests passed.'
  if ($Backend -eq 'cuda') { 'CUDA 13 import identity verified for sm_86. GPU startup/inference requires a physical NVIDIA machine.' }
  'Live Windows Discord/call detection still requires physical-machine qualification.'
) >> $env:GITHUB_STEP_SUMMARY
