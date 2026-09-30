# CI-only CPU candidate. Production universal/updater packaging stays in
# build-universal-windows.ps1; this path never signs or publishes a release.
param([string]$Target = 'x86_64-pc-windows-msvc')

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
New-Item -ItemType Directory -Force $output, $stage | Out-Null

# App-local Microsoft redistributable DLLs make this CPU preview runnable on
# machines without VS installed. The production installer has its own redist hook.
foreach ($component in @('Microsoft.VC143.CRT', 'Microsoft.VC143.OpenMP')) {
  $source = Join-Path $env:VCToolsRedistDir "x64/$component"
  if (!(Test-Path $source)) { throw "Missing MSVC redistributables: $source" }
  Copy-Item "$source/*.dll" $stage -Force
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
foreach ($file in Get-ChildItem $stage -Filter '*.dll') {
  $resources["preview-runtime/$($file.Name)"] = $file.Name
}

# Tauri merges this override after tauri.windows.conf.json. Null removes the
# release signing command and universal installer template/hooks. Stock NSIS
# installs the single CPU executable and required resources for the current user.
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
  pnpm exec tauri build --target $Target --config $override --bundles nsis -- --locked --no-default-features --features custom-protocol
  if ($LASTEXITCODE -ne 0) { throw 'CPU Preview NSIS build failed' }
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
if ((Get-FileHash $main).Hash -ne (Get-FileHash (Join-Path $repo "target/$Target/release/meetily.exe")).Hash) {
  throw 'Installer application differs from built candidate'
}
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

$audio = Join-Path $env:RUNNER_TEMP 'meetily-preview-smoke.m4a'
& (Join-Path $installed 'ffmpeg.exe') -v error -f lavfi -i 'anullsrc=r=48000:cl=mono' -t 0.25 -c:a aac -y $audio
if ($LASTEXITCODE -ne 0 -or (Get-Item $audio).Length -eq 0) { throw 'Installed FFmpeg encode failed' }
& (Join-Path $installed 'ffmpeg.exe') -v error -i $audio -f f32le -acodec pcm_f32le -y "$audio.f32"
if ($LASTEXITCODE -ne 0 -or (Get-Item "$audio.f32").Length -eq 0) { throw 'Installed FFmpeg decode failed' }
$llamaOutput = @('{"type":"ping"}', '{"type":"shutdown"}') | & (Join-Path $installed 'llama-helper.exe')
if ($LASTEXITCODE -ne 0 -or "$llamaOutput" -notmatch '"type":"pong"' -or "$llamaOutput" -notmatch '"type":"goodbye"') {
  throw 'Installed llama-helper protocol smoke test failed'
}

$stem = "Meetily-Actually-Free_$($base.version)_preview-fixes_$($env:BUILD_COMMIT.Substring(0, 12))_x64-cpu"
Copy-Item $installer (Join-Path $output "$stem-setup.exe")
# The ZIP contains the same verified installed files, allowing qualification
# without replacing an installed executable (the app still shares its data root).
Compress-Archive -Path "$installed/*" -DestinationPath (Join-Path $output "$stem.app.zip") -Force
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
  whisper_backend = 'cpu'
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
) | Set-Content (Join-Path $output 'tool-versions.txt') -Encoding utf8
Get-ChildItem $output -File | Where-Object Name -ne 'SHA256SUMS' | Sort-Object Name | ForEach-Object {
  "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)"
} | Set-Content (Join-Path $output 'SHA256SUMS') -Encoding ascii
@(
  "Candidate commit: $env:BUILD_COMMIT"
  'Unsigned x64 CPU NSIS installer and app ZIP; version 0.2.18 / com.meetily.ai.'
  'Native synthetic regressions, silent installation, resource hashes and sidecar smoke tests passed.'
  'Live Windows Discord/call detection still requires physical-machine qualification.'
) >> $env:GITHUB_STEP_SUMMARY
