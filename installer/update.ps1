$ErrorActionPreference = 'Stop'
$directory = Split-Path -Parent $env:RDIFF_MSI
$statusPath = Join-Path $directory 'status.txt'
$logPath = Join-Path $directory 'install.log'
$lock = $null
try {
    $lockPath = Join-Path ([IO.Path]::GetDirectoryName($env:RDIFF_EXE)) '.rdiff-update.lock'
    $lock = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    Set-Content $statusPath 'Waiting for all rdiff windows to close. Save your work and close them normally.'
    # Never terminate the editor or ask Restart Manager to close unsaved windows.
    Wait-Process -Id ([int]$env:RDIFF_PARENT_PID) -ErrorAction SilentlyContinue
    $deadline = [DateTime]::UtcNow.AddHours(24)
    while (Get-Process -Name rdiff -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $env:RDIFF_EXE }) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Timed out waiting for rdiff to close; run Check updates again.' }
        Start-Sleep -Seconds 2
    }
    Set-Content $statusPath "Installing rdiff $env:RDIFF_UPDATE_VERSION through Windows Installer."
    $stream = [IO.File]::OpenRead($env:RDIFF_MSI)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') }
    finally { $stream.Dispose(); $sha.Dispose() }
    if ($hash -ne $env:RDIFF_MSI_SHA256) {
        throw 'MSI SHA-256 changed while waiting; installation canceled.'
    }
    $arguments = @('/i', ('"' + $env:RDIFF_MSI + '"'), '/qn', '/norestart', 'MSIRESTARTMANAGERCONTROL=Disable', '/L*v', ('"' + $logPath + '"'))
    $process = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\msiexec.exe') -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -notin @(0, 3010)) { throw "Windows Installer failed with exit code $($process.ExitCode). See $logPath" }
    Set-Content $statusPath "Installed rdiff $env:RDIFF_UPDATE_VERSION. Windows Installed apps updated. Exit code: $($process.ExitCode)."
} catch {
    Set-Content $statusPath "Update failed: $($_.Exception.Message)"
    exit 1
} finally {
    if ($null -ne $lock) { $lock.Dispose() }
}
