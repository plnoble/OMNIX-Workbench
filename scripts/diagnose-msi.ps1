param(
  [switch]$SkipLight,
  [switch]$RunSuppressValidation
)

$ErrorActionPreference = "Continue"

function Write-Section {
  param([string]$Title)
  Write-Host ""
  Write-Host "=== $Title ==="
}

function Test-RegistryPath {
  param(
    [string]$Name,
    [string]$Path
  )

  $item = Get-ItemProperty -Path $Path -ErrorAction SilentlyContinue
  if ($null -eq $item) {
    Write-Host "[FAIL] $Name missing: $Path"
    return
  }

  $defaultValue = $item.'(default)'
  if ([string]::IsNullOrWhiteSpace($defaultValue)) {
    $defaultValue = $item.PSChildName
  }
  Write-Host "[ OK ] $Name -> $defaultValue"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$wixRoot = Join-Path $env:LOCALAPPDATA "tauri\WixTools314"
$light = Join-Path $wixRoot "light.exe"
$wixobj = Join-Path $repoRoot "src-tauri\target\release\wix\x64\main.wixobj"
$locale = Join-Path $repoRoot "src-tauri\target\release\wix\x64\locale.wxl"
$msiDir = Join-Path $repoRoot "src-tauri\target\release\bundle\msi"

Write-Section "MSI diagnostic context"
Write-Host "Repository: $repoRoot"
Write-Host "WiX tools:  $wixRoot"
Write-Host "light.exe:  $light"
Write-Host "wixobj:     $wixobj"
Write-Host "locale:     $locale"
Write-Host "msi output: $msiDir"

Write-Section "Windows Installer service"
try {
  $service = Get-Service msiserver -ErrorAction Stop
  Write-Host "Status:    $($service.Status)"
  Write-Host "StartType: $($service.StartType)"
} catch {
  Write-Host "[FAIL] Unable to read msiserver: $($_.Exception.Message)"
}

Write-Section "COM and script engine registration"
try {
  $installer = New-Object -ComObject WindowsInstaller.Installer
  Write-Host "[ OK ] WindowsInstaller.Installer COM: $($installer.GetType().FullName)"
} catch {
  Write-Host "[FAIL] WindowsInstaller.Installer COM: $($_.Exception.Message)"
}

Test-RegistryPath "VBScript x64" "Registry::HKEY_CLASSES_ROOT\CLSID\{B54F3741-5B07-11cf-A4B0-00AA004A55E8}\InprocServer32"
Test-RegistryPath "VBScript x86" "Registry::HKEY_CLASSES_ROOT\Wow6432Node\CLSID\{B54F3741-5B07-11cf-A4B0-00AA004A55E8}\InprocServer32"
Test-RegistryPath "JScript x64" "Registry::HKEY_CLASSES_ROOT\CLSID\{f414c260-6ac0-11cf-b6d1-00aa00bbbb58}\InprocServer32"
Test-RegistryPath "JScript x86" "Registry::HKEY_CLASSES_ROOT\Wow6432Node\CLSID\{f414c260-6ac0-11cf-b6d1-00aa00bbbb58}\InprocServer32"

if ($SkipLight) {
  Write-Section "WiX light.exe validation"
  Write-Host "Skipped by -SkipLight."
  exit 0
}

if (!(Test-Path $light)) {
  Write-Section "WiX light.exe validation"
  Write-Host "[FAIL] light.exe was not found. Run a Tauri MSI build once so Tauri can install WiX tools."
  exit 1
}

if (!(Test-Path $wixobj) -or !(Test-Path $locale)) {
  Write-Section "WiX light.exe validation"
  Write-Host "[FAIL] WiX build inputs are missing. Run: npm.cmd run tauri:build:msi"
  exit 1
}

New-Item -ItemType Directory -Force -Path $msiDir | Out-Null

Write-Section "WiX light.exe normal validation"
$normalOut = Join-Path $msiDir "diagnostic-normal.msi"
$normalArgs = @(
  "-nologo",
  "-ext", "WixUIExtension",
  "-cultures:en-us",
  "-loc", $locale,
  "-out", $normalOut,
  $wixobj
)
& $light @normalArgs
$normalExit = $LASTEXITCODE
Write-Host "Exit code: $normalExit"

if ($RunSuppressValidation) {
  Write-Section "WiX light.exe suppressed-validation control"
  $svalOut = Join-Path $msiDir "diagnostic-sval.msi"
  $svalArgs = @(
    "-nologo",
    "-sval",
    "-ext", "WixUIExtension",
    "-cultures:en-us",
    "-loc", $locale,
    "-out", $svalOut,
    $wixobj
  )
  & $light @svalArgs
  $svalExit = $LASTEXITCODE
  Write-Host "Exit code: $svalExit"
  Write-Host "Note: diagnostic-sval.msi is not a release artifact."
}

exit $normalExit
