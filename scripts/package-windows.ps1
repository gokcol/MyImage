# Packages the exact app checkout, using installer tooling from this checkout.
[CmdletBinding()]
param(
    [string]$SourceDirectory = (Split-Path $PSScriptRoot -Parent),
    [string]$OutputDirectory = 'dist/windows',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = (Resolve-Path $SourceDirectory).Path
$out = [IO.Path]::GetFullPath($OutputDirectory)
$toolsRoot = Split-Path $PSScriptRoot -Parent
if ($env:OS -ne 'Windows_NT') { throw 'Run Windows packaging on Windows.' }
$null = New-Item -ItemType Directory -Force $out
Push-Location $source
try {
    $version = (Select-String -Path Cargo.toml -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"').Matches[0].Groups[1].Value
    [xml]$plist = Get-Content packaging/macos/Info.plist -Raw
    $build = $plist.SelectSingleNode("/plist/dict/key[text()='CFBundleVersion']/following-sibling::string[1]").InnerText
    if ($build -notmatch '^[0-9]+$' -or [int]$build -gt 65535) { throw 'Invalid build number.' }
    $name = "MyImage-$version-build$build"
    $setup = Join-Path $out "$name-Setup.exe"
    $archive = Join-Path $out "$name-windows-x64.zip"
    foreach ($path in @($setup, $archive)) {
        if (Test-Path $path) { throw "Output already exists: $path" }
    }
    if (-not $SkipBuild) {
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
        & cargo build --locked --release --target x86_64-pc-windows-msvc --bin myimage
        if ($LASTEXITCODE -ne 0) { throw 'Windows build failed.' }
    }
    $exe = Join-Path $source 'target/x86_64-pc-windows-msvc/release/myimage.exe'
    $signature = Get-AuthenticodeSignature $exe
    # This explicit unsigned channel matches MyPDF. Do not describe these
    # downloads as Microsoft-signed; Store signing applies only to Store delivery.
    if ($signature.Status -ne 'NotSigned') { throw 'Review signing policy before shipping a signed or invalidly signed binary through the unsigned channel.' }
    $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
    if (-not (Test-Path $iscc)) { throw 'Install Inno Setup 6 before packaging.' }
    $work = Join-Path $out ('.work-' + [guid]::NewGuid().ToString('N'))
    $layout = Join-Path $work 'MyImage'
    $null = New-Item -ItemType Directory -Force $layout
    try {
        Copy-Item $exe (Join-Path $layout 'MyImage.exe')
        Copy-Item LICENSE (Join-Path $layout 'LICENSE.txt')
        Copy-Item assets/stamps/LICENSE-GRAPHICS (Join-Path $layout 'Twemoji-LICENSE.txt')
        Copy-Item assets/stamps/ATTRIBUTION.txt (Join-Path $layout 'Twemoji-ATTRIBUTION.txt')
        # ICO supports an embedded PNG; retain the existing 256px artwork exactly.
        $png = [IO.File]::ReadAllBytes((Join-Path $source 'assets/branding/myimage-app-icon-256.png'))
        $ico = [IO.BinaryWriter]::new([IO.File]::Create((Join-Path $layout 'MyImage.ico')))
        try {
            $ico.Write([uint16]0); $ico.Write([uint16]1); $ico.Write([uint16]1)
            $ico.Write([byte]0); $ico.Write([byte]0); $ico.Write([byte]0); $ico.Write([byte]0)
            $ico.Write([uint16]1); $ico.Write([uint16]32)
            $ico.Write([uint32]$png.Length); $ico.Write([uint32]22); $ico.Write($png)
        } finally { $ico.Dispose() }
        @"
MyImage $version (build $build) - Windows x64

This GitHub download is unsigned, like the MyPDF GitHub installer.
Windows may show an unknown publisher or SmartScreen warning. Do not disable
Windows security. Microsoft Store signing is separate and applies only to
packages delivered by the Store after certification.

Windows 10 version 2004 or later / Windows 11 is required.
The installer installs for the current user without administrator rights.
It adds a Start menu shortcut and an uninstaller. The desktop shortcut and
Open with integration are optional; default file associations are preserved.
Start at login remains off until enabled in MyImage Settings.

Save your work and quit MyImage from its tray menu before updating/removing it.
Uninstall removes installed application files, not projects, captured images,
settings, stamps or recovery data. The portable ZIP can instead be extracted
to a folder and run with MyImage.exe; remove that folder to remove that copy.

Packaging/install/uninstall are checked in CI. Interactive Windows capture,
clipboard, pen/touch, scaling and tray behavior still require device testing.
Source and releases: https://github.com/gokcol/MyImage
"@ | Set-Content -Encoding utf8 (Join-Path $layout 'READ-ME-WINDOWS.txt')
        & $iscc "/DAppVersion=$version" "/DBuildNumber=$build" "/DSource=$layout" "/O$work" "/F$name-Setup" (Join-Path $toolsRoot 'packaging/windows/MyImage.iss')
        if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
        $candidate = Join-Path $work "$name-Setup.exe"
        if ((Get-AuthenticodeSignature $candidate).Status -ne 'NotSigned') { throw 'Unexpected installer signing status.' }
        Compress-Archive -Path $layout -DestinationPath (Join-Path $work "$name-windows-x64.zip")
        $commit = & git rev-parse HEAD
        if ($LASTEXITCODE -ne 0) { throw 'Cannot identify application source.' }
        $toolCommit = & git -C $toolsRoot rev-parse HEAD
        if ($LASTEXITCODE -ne 0) { throw 'Cannot identify packaging source.' }
        @{
            version = $version; build = $build; architecture = 'x64'
            applicationCommit = $commit; packagingCommit = $toolCommit
            authenticode = 'NotSigned'; windowsInteractiveTesting = 'pending'
            executableSha256 = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
        } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $work "$name-windows.json")
        foreach ($path in @($candidate, (Join-Path $work "$name-windows-x64.zip"))) {
            $hash = (Get-FileHash $path -Algorithm SHA256).Hash.ToLower()
            "$hash  $(Split-Path $path -Leaf)" | Set-Content -Encoding ascii "$path.sha256"
        }
        Get-ChildItem $work -File | Move-Item -Destination $out
    } finally { if (Test-Path $work) { Remove-Item $work -Recurse -Force } }
} finally { Pop-Location }
