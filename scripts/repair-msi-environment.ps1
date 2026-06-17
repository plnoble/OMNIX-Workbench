param(
  [switch]$RunHealthScan
)

$ErrorActionPreference = "Stop"

function Write-Section {
  param([string]$Title)
  Write-Host ""
  Write-Host "=== $Title ==="
}

function Test-Administrator {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-Step {
  param(
    [string]$Title,
    [scriptblock]$Action
  )

  Write-Section $Title
  & $Action
  Write-Host "[ OK ] $Title"
}

function Invoke-Native {
  param(
    [string]$FilePath,
    [string[]]$Arguments
  )

  & $FilePath @Arguments
  # msiexec /unregister (and similar silent ops) may leave $LASTEXITCODE unset
  # (null/empty). Treat an unset code as success — a real failure surfaces as a
  # non-zero integer or a Windows Installer error dialog, not an empty code.
  $code = $LASTEXITCODE
  if ($null -ne $code -and $code -ne '' -and [int]$code -ne 0) {
    throw "$FilePath exited with code $code"
  }
}

if (!(Test-Administrator)) {
  Write-Error "This repair must run from an elevated PowerShell session."
  Write-Host "Open PowerShell as Administrator, then run:"
  Write-Host "  powershell -ExecutionPolicy Bypass -File scripts/repair-msi-environment.ps1"
  exit 1
}

Invoke-Step "Unregister Windows Installer" {
  Invoke-Native "$env:WINDIR\System32\msiexec.exe" @("/unregister")
}

Invoke-Step "Register Windows Installer" {
  Invoke-Native "$env:WINDIR\System32\msiexec.exe" @("/regserver")
}

Invoke-Step "Register VBScript and JScript x64" {
  Invoke-Native "$env:WINDIR\System32\regsvr32.exe" @("/s", "$env:WINDIR\System32\vbscript.dll")
  Invoke-Native "$env:WINDIR\System32\regsvr32.exe" @("/s", "$env:WINDIR\System32\jscript.dll")
}

Invoke-Step "Register VBScript and JScript x86" {
  Invoke-Native "$env:WINDIR\SysWOW64\regsvr32.exe" @("/s", "$env:WINDIR\SysWOW64\vbscript.dll")
  Invoke-Native "$env:WINDIR\SysWOW64\regsvr32.exe" @("/s", "$env:WINDIR\SysWOW64\jscript.dll")
}

Invoke-Step "Restart Windows Installer service" {
  $service = Get-Service msiserver -ErrorAction Stop
  if ($service.Status -eq "Running") {
    Stop-Service msiserver -Force -ErrorAction SilentlyContinue
  }
  Start-Service msiserver
  Get-Service msiserver | Format-Table Name, Status, StartType -AutoSize
}

if ($RunHealthScan) {
  Invoke-Step "DISM RestoreHealth" {
    Invoke-Native "dism.exe" @("/Online", "/Cleanup-Image", "/RestoreHealth")
  }

  Invoke-Step "SFC ScanNow" {
    Invoke-Native "sfc.exe" @("/scannow")
  }
} else {
  Write-Section "Health scan"
  Write-Host "Skipped. Re-run with -RunHealthScan only if MSI validation still fails."
}

Write-Section "Next verification"
Write-Host "Run:"
Write-Host "  npm.cmd run diagnose:msi"
Write-Host "  npm.cmd run tauri:build:msi"
