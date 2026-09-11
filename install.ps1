# PowerShell installer for Tarvos
# Checks for rustup, installs the stable toolchain, builds the project, and adds the binary to PATH.

function Ensure-Rustup {
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
        Write-Host "rustup not found. Installing rustup..."
        Invoke-WebRequest -Uri https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe -OutFile $env:TEMP\rustup-init.exe
        & $env:TEMP\rustup-init.exe -y --no-modify-path
        $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine")
    } else {
        Write-Host "rustup is already installed."
    }
}

function Install-Tarvos {
    Write-Host "Installing Tarvos CLI via cargo..."
    cargo install --locked --path crates/tarvos-cli --force
}

function Add-ToPath {
    $cargoBin = "$env:USERPROFILE\.cargo\bin"
    if (-not ($env:Path -split ";" | Where-Object { $_ -eq $cargoBin })) {
        Write-Host "Adding $cargoBin to PATH..."
        [System.Environment]::SetEnvironmentVariable("Path", $env:Path + ";" + $cargoBin, "User")
    }
}

Ensure-Rustup
Install-Tarvos
Add-ToPath

Write-Host "Installation complete. You can now run 'tarvos --help'."
