# Chloride MSI Installer Build Script
# This script builds the Rust project and creates an MSI installer
# Supports both WiX Toolset 3.x and 4.x

param(
    [switch]$Clean,
    [switch]$Release = $true,
    [switch]$Force
)

# Colors for output
$Green = "Green"
$Red = "Red"
$Yellow = "Yellow"
$Blue = "Blue"
$Cyan = "Cyan"

function Write-ColorOutput($ForegroundColor) {
    $fc = $host.UI.RawUI.ForegroundColor
    $host.UI.RawUI.ForegroundColor = $ForegroundColor
    if ($args) {
        Write-Output $args
    }
    $host.UI.RawUI.ForegroundColor = $fc
}

function Find-WixToolset {
    Write-ColorOutput $Blue "🔍 Detecting WiX Toolset installation..."
    
    # Common WiX installation paths
    $wixPaths = @(
        # WiX 4.x/6.x paths
        "${env:ProgramFiles}\WiX Toolset v6.0\bin",
        "${env:ProgramFiles(x86)}\WiX Toolset v6.0\bin",
        "${env:ProgramFiles}\WiX Toolset v4\bin",
        "${env:ProgramFiles(x86)}\WiX Toolset v4\bin",
        "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\WiXToolset.WiX_Microsoft.Winget.Source_*\bin",
        
        # WiX 3.x paths
        "${env:ProgramFiles}\WiX Toolset v3.11\bin",
        "${env:ProgramFiles(x86)}\WiX Toolset v3.11\bin",
        "${env:ProgramFiles}\WiX Toolset v3.14\bin",
        "${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin",
        
        # dotnet tool installation
        "$env:USERPROFILE\.dotnet\tools"
    )
    
    # Check PATH first
    try {
        $wixExe = Get-Command "wix.exe" -ErrorAction Stop
        Write-ColorOutput $Green "✅ WiX 4.x/6.x found in PATH: $($wixExe.Source)"
        return @{
            Version = "4.x"
            Path = Split-Path $wixExe.Source
            WixExe = $wixExe.Source
        }
    }
    catch {
        Write-ColorOutput $Yellow "   WiX 4.x/6.x not found in PATH, checking other locations..."
    }
    
    try {
        $candleExe = Get-Command "candle.exe" -ErrorAction Stop
        $lightExe = Get-Command "light.exe" -ErrorAction Stop
        Write-ColorOutput $Green "✅ WiX 3.x found in PATH"
        return @{
            Version = "3.x"
            Path = Split-Path $candleExe.Source
            CandleExe = $candleExe.Source
            LightExe = $lightExe.Source
        }
    }
    catch {
        Write-ColorOutput $Yellow "   WiX 3.x not found in PATH, checking installation directories..."
    }
    
    # Check common installation paths
    foreach ($path in $wixPaths) {
        # Expand wildcards
        $expandedPaths = Get-ChildItem -Path (Split-Path $path -Parent) -Directory -ErrorAction SilentlyContinue | 
                        Where-Object { $_.Name -like (Split-Path $path -Leaf) }
        
        if ($expandedPaths) {
            foreach ($expandedPath in $expandedPaths) {
                $fullPath = Join-Path $expandedPath.FullName (Split-Path $path -Leaf)
                if (Test-Path $fullPath) {
                    $path = $fullPath
                    break
                }
            }
        }
        
        if (Test-Path $path) {
            # Check for WiX 4.x/6.x
            $wixExePath = Join-Path $path "wix.exe"
            if (Test-Path $wixExePath) {
                Write-ColorOutput $Green "✅ WiX 4.x/6.x found at: $path"
                return @{
                    Version = "4.x"
                    Path = $path
                    WixExe = $wixExePath
                }
            }
            
            # Check for WiX 3.x
            $candlePath = Join-Path $path "candle.exe"
            $lightPath = Join-Path $path "light.exe"
            if ((Test-Path $candlePath) -and (Test-Path $lightPath)) {
                Write-ColorOutput $Green "✅ WiX 3.x found at: $path"
                return @{
                    Version = "3.x"
                    Path = $path
                    CandleExe = $candlePath
                    LightExe = $lightPath
                }
            }
        }
    }
    
    # Check if WiX is installed via dotnet tool
    try {
        $dotnetTools = dotnet tool list --global | Select-String "wix"
        if ($dotnetTools) {
            Write-ColorOutput $Green "✅ WiX found as dotnet global tool"
            return @{
                Version = "dotnet"
                Path = ""
                WixExe = "wix"
            }
        }
    }
    catch {
        # dotnet not available or no global tools
    }
    
    return $null
}

function Install-WixSuggestions {
    Write-ColorOutput $Red "❌ WiX Toolset not found!"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Yellow "🔧 Installation options:"
    Write-ColorOutput $Yellow "   Option 1 (Recommended): Install via Windows Package Manager"
    Write-ColorOutput $Cyan "     winget install --id WiXToolset.WiX"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Yellow "   Option 2: Install via .NET Global Tool"
    Write-ColorOutput $Cyan "     dotnet tool install --global wix"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Yellow "   Option 3: Download from official website"
    Write-ColorOutput $Cyan "     https://wixtoolset.org/releases/"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Yellow "   Option 4: Install via Chocolatey"
    Write-ColorOutput $Cyan "     choco install wixtoolset"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Yellow "After installation, restart your terminal and try again."
}

function Check-Prerequisites {
    Write-ColorOutput $Blue "🔍 Checking prerequisites..."
    
    # Check if Rust is installed
    try {
        $rustVersion = cargo --version
        Write-ColorOutput $Green "✅ Rust found: $rustVersion"
    }
    catch {
        Write-ColorOutput $Red "❌ Rust not found. Please install Rust first."
        Write-ColorOutput $Yellow "   Download from: https://rustup.rs/"
        exit 1
    }
    
    # Find WiX installation
    $script:WixInfo = Find-WixToolset
    if (-not $script:WixInfo) {
        Install-WixSuggestions
        exit 1
    }
    
    Write-ColorOutput $Green "✅ Found WiX Toolset $($script:WixInfo.Version)"
    if ($script:WixInfo.Version -eq "4.x") {
        Write-ColorOutput $Cyan "   Using modern WiX build command"
    }
}

function Build-RustProject {
    Write-ColorOutput $Blue "🔨 Building Rust project..."
    
    # Determine if we're in the installer directory or project root
    $currentDir = Get-Location
    $projectRoot = $currentDir
    
    if ((Split-Path $currentDir -Leaf) -eq "installer") {
        $projectRoot = Split-Path $currentDir -Parent
    }
    
    Push-Location $projectRoot
    
    if ($Clean) {
        Write-ColorOutput $Yellow "🧹 Cleaning previous build..."
        cargo clean
    }
    
    if ($Release) {
        cargo build --release
    } else {
        cargo build
    }
    
    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput $Red "❌ Rust build failed"
        Pop-Location
        exit 1
    }
    
    # Verify the executable was created
    $targetDir = if ($Release) { "release" } else { "debug" }
    $exePath = Join-Path $projectRoot "target\$targetDir\chloride.exe"
    
    if (-not (Test-Path $exePath)) {
        Write-ColorOutput $Red "❌ Expected executable not found: $exePath"
        Pop-Location
        exit 1
    }
    
    Pop-Location
    Write-ColorOutput $Green "✅ Rust project built successfully"
}

function Build-Installer-Wix3 {
    Write-ColorOutput $Blue "📦 Building MSI installer with WiX 3.x..."
    
    # Change to installer directory for WiX build
    $currentDir = Get-Location
    $installerDir = if ((Split-Path $currentDir -Leaf) -eq "installer") { $currentDir } else { Join-Path $currentDir "installer" }
    Push-Location $installerDir
    
    # Compile WiX source
    Write-ColorOutput $Yellow "   Compiling WiX source..."
    & $script:WixInfo.CandleExe "chloride.wxs" -out "chloride.wixobj"
    
    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput $Red "❌ WiX compilation failed"
        Pop-Location
        exit 1
    }
    
    # Link and create MSI
    Write-ColorOutput $Yellow "   Linking and creating MSI..."
    & $script:WixInfo.LightExe "chloride.wixobj" -ext "WixUIExtension" -out "chloride.msi"
    
    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput $Red "❌ MSI creation failed"
        Pop-Location
        exit 1
    }
    
    Pop-Location
}

function Build-Installer-Wix4 {
    Write-ColorOutput $Blue "📦 Building MSI installer with WiX 4.x/6.x..."
    
    # Change to installer directory for WiX build
    $currentDir = Get-Location
    $installerDir = if ((Split-Path $currentDir -Leaf) -eq "installer") { $currentDir } else { Join-Path $currentDir "installer" }
    Push-Location $installerDir
    
    Write-ColorOutput $Yellow "   Building MSI with WiX 4.x/6.x..."
    & $script:WixInfo.WixExe "build" "chloride.wxs" -out "chloride.msi"
    
    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput $Red "❌ MSI creation failed"
        Pop-Location
        exit 1
    }
    
    Pop-Location
}

function Build-Installer-DotNet {
    Write-ColorOutput $Blue "📦 Building MSI installer with WiX (dotnet tool)..."
    
    # Change to installer directory for WiX build
    $currentDir = Get-Location
    $installerDir = if ((Split-Path $currentDir -Leaf) -eq "installer") { $currentDir } else { Join-Path $currentDir "installer" }
    Push-Location $installerDir
    
    Write-ColorOutput $Yellow "   Building MSI with dotnet wix tool..."
    wix build "chloride.wxs" -out "chloride.msi"
    
    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput $Red "❌ MSI creation failed"
        Pop-Location
        exit 1
    }
    
    Pop-Location
}

function Build-Installer {
    # Determine installer directory and clean up any previous build artifacts
    $currentDir = Get-Location
    $installerDir = if ((Split-Path $currentDir -Leaf) -eq "installer") { $currentDir } else { Join-Path $currentDir "installer" }
    
    Remove-Item -Path (Join-Path $installerDir "chloride.wixobj") -ErrorAction SilentlyContinue
    Remove-Item -Path (Join-Path $installerDir "chloride.wixpdb") -ErrorAction SilentlyContinue
    Remove-Item -Path (Join-Path $installerDir "chloride.msi") -ErrorAction SilentlyContinue
    
    switch ($script:WixInfo.Version) {
        "3.x" { Build-Installer-Wix3 }
        "4.x" { Build-Installer-Wix4 }
        "dotnet" { Build-Installer-DotNet }
        default {
            Write-ColorOutput $Red "❌ Unsupported WiX version: $($script:WixInfo.Version)"
            exit 1
        }
    }
    
    # Clean up intermediate files
    Remove-Item -Path (Join-Path $installerDir "chloride.wixobj") -ErrorAction SilentlyContinue
    Remove-Item -Path (Join-Path $installerDir "chloride.wixpdb") -ErrorAction SilentlyContinue
    
    # Verify MSI was created
    $msiPath = Join-Path $installerDir "chloride.msi"
    if (Test-Path $msiPath) {
        $msiSize = (Get-Item $msiPath).Length
        Write-ColorOutput $Green "✅ MSI installer created successfully: chloride.msi ($([math]::Round($msiSize/1KB, 2)) KB)"
    } else {
        Write-ColorOutput $Red "❌ MSI file was not created"
        exit 1
    }
}

function Show-Instructions {
    Write-ColorOutput $Blue ""
    Write-ColorOutput $Blue "📋 Installation Instructions:"
    Write-ColorOutput $Yellow "   1. Run the installer: .\chloride.msi"
    Write-ColorOutput $Yellow "   2. Follow the installation wizard"
    Write-ColorOutput $Yellow "   3. After installation, open a new command prompt or PowerShell"
    Write-ColorOutput $Yellow "   4. Use 'chloride' or 'cl' commands from anywhere"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Green "🎉 Example usage after installation:"
    Write-ColorOutput $Cyan "   chloride touch myfile.txt"
    Write-ColorOutput $Cyan "   cl rm oldfile.txt"
    Write-ColorOutput $Cyan "   chloride --help"
    Write-ColorOutput $Yellow ""
    Write-ColorOutput $Blue "💡 Tips:"
    Write-ColorOutput $Yellow "   • The installer adds Chloride to your system PATH"
    Write-ColorOutput $Yellow "   • You can use either 'chloride' or 'cl' as the command"
    Write-ColorOutput $Yellow "   • Restart your terminal after installation for PATH changes to take effect"
}

function Test-Installation {
    $currentDir = Get-Location
    $installerDir = if ((Split-Path $currentDir -Leaf) -eq "installer") { $currentDir } else { Join-Path $currentDir "installer" }
    $msiPath = Join-Path $installerDir "chloride.msi"
    
    if (Test-Path $msiPath) {
        Write-ColorOutput $Blue ""
        Write-ColorOutput $Blue "🧪 Testing installer (optional):"
        Write-ColorOutput $Yellow "   To test the installer without installing:"
        Write-ColorOutput $Cyan "     msiexec /a `"$msiPath`" /qb TARGETDIR=`"$installerDir\test_install`""
        Write-ColorOutput $Yellow "   This will extract files to .\installer\test_install for verification"
    }
}

# Main execution
Write-ColorOutput $Blue "🧪 Chloride MSI Installer Builder"
Write-ColorOutput $Blue "=================================="
Write-ColorOutput $Yellow "PowerShell Edition - Supports WiX 3.x and 4.x"
Write-ColorOutput $Blue ""

try {
    Check-Prerequisites
    Build-RustProject
    Build-Installer
    Show-Instructions
    Test-Installation
    
    Write-ColorOutput $Green ""
    Write-ColorOutput $Green "🎯 Build completed successfully!"
    Write-ColorOutput $Green "📦 Your installer is ready: chloride.msi"
}
catch {
    Write-ColorOutput $Red "❌ Build failed with error: $($_.Exception.Message)"
    Write-ColorOutput $Yellow "💡 Try running with -Force flag or check the error details above"
    exit 1
}