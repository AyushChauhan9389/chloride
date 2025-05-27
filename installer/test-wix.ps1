# WiX Installation Test Script
# This script helps diagnose WiX installation issues

param(
    [switch]$Verbose
)

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

function Test-WixInstallation {
    Write-ColorOutput $Blue "🔍 WiX Installation Diagnostics"
    Write-ColorOutput $Blue "==============================="
    Write-ColorOutput $Blue ""
    
    # Test PATH environment
    Write-ColorOutput $Yellow "Checking PATH environment variable..."
    $pathDirs = $env:PATH -split ";"
    $wixInPath = $pathDirs | Where-Object { $_ -like "*wix*" -or $_ -like "*WiX*" }
    
    if ($wixInPath) {
        Write-ColorOutput $Green "✅ Found WiX-related paths in PATH:"
        $wixInPath | ForEach-Object { Write-ColorOutput $Cyan "   $_" }
    } else {
        Write-ColorOutput $Yellow "⚠️  No WiX paths found in PATH"
    }
    Write-ColorOutput $Blue ""
    
    # Test WiX 4.x
    Write-ColorOutput $Yellow "Testing WiX 4.x availability..."
    try {
        $wixVersion = & wix.exe --version 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-ColorOutput $Green "✅ WiX 4.x/6.x is available: $wixVersion"
            $wixPath = Get-Command "wix.exe" | Select-Object -ExpandProperty Source
            Write-ColorOutput $Cyan "   Location: $wixPath"
        } else {
            Write-ColorOutput $Red "❌ WiX 4.x/6.x command failed"
        }
    }
    catch {
        Write-ColorOutput $Yellow "⚠️  WiX 4.x/6.x not found in PATH"
    }
    Write-ColorOutput $Blue ""
    
    # Test WiX 3.x
    Write-ColorOutput $Yellow "Testing WiX 3.x availability..."
    try {
        $candleHelp = & candle.exe 2>&1
        $lightHelp = & light.exe 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-ColorOutput $Green "✅ WiX 3.x tools are available"
            $candlePath = Get-Command "candle.exe" | Select-Object -ExpandProperty Source
            $lightPath = Get-Command "light.exe" | Select-Object -ExpandProperty Source
            Write-ColorOutput $Cyan "   Candle: $candlePath"
            Write-ColorOutput $Cyan "   Light: $lightPath"
        }
    }
    catch {
        Write-ColorOutput $Yellow "⚠️  WiX 3.x tools not found in PATH"
    }
    Write-ColorOutput $Blue ""
    
    # Check common installation directories
    Write-ColorOutput $Yellow "Checking common WiX installation directories..."
    
    $commonPaths = @(
        "${env:ProgramFiles}\WiX Toolset v6.0",
        "${env:ProgramFiles(x86)}\WiX Toolset v6.0",
        "${env:ProgramFiles}\WiX Toolset v4",
        "${env:ProgramFiles(x86)}\WiX Toolset v4",
        "${env:ProgramFiles}\WiX Toolset v3.11",
        "${env:ProgramFiles(x86)}\WiX Toolset v3.11",
        "${env:ProgramFiles}\WiX Toolset v3.14",
        "${env:ProgramFiles(x86)}\WiX Toolset v3.14"
    )
    
    $foundInstallations = @()
    
    foreach ($path in $commonPaths) {
        if (Test-Path $path) {
            $foundInstallations += $path
            Write-ColorOutput $Green "✅ Found: $path"
            
            if ($Verbose) {
                $binPath = Join-Path $path "bin"
                if (Test-Path $binPath) {
                    Write-ColorOutput $Cyan "   Bin directory contents:"
                    Get-ChildItem $binPath -Filter "*.exe" | ForEach-Object {
                        Write-ColorOutput $Cyan "     $($_.Name)"
                    }
                }
            }
        }
    }
    
    if ($foundInstallations.Count -eq 0) {
        Write-ColorOutput $Yellow "⚠️  No WiX installations found in common directories"
    }
    Write-ColorOutput $Blue ""
    
    # Check dotnet global tools
    Write-ColorOutput $Yellow "Checking .NET global tools..."
    try {
        $globalTools = dotnet tool list --global 2>&1
        if ($LASTEXITCODE -eq 0) {
            $wixTool = $globalTools | Select-String "wix"
            if ($wixTool) {
                Write-ColorOutput $Green "✅ WiX found as .NET global tool:"
                Write-ColorOutput $Cyan "   $wixTool"
            } else {
                Write-ColorOutput $Yellow "⚠️  WiX not found in .NET global tools"
            }
        } else {
            Write-ColorOutput $Yellow "⚠️  Unable to list .NET global tools"
        }
    }
    catch {
        Write-ColorOutput $Yellow "⚠️  .NET CLI not available"
    }
    Write-ColorOutput $Blue ""
    
    # Check winget packages
    Write-ColorOutput $Yellow "Checking winget installed packages..."
    try {
        $wingetList = winget list | Select-String -Pattern "wix|WiX" 2>&1
        if ($wingetList) {
            Write-ColorOutput $Green "✅ WiX found in winget packages:"
            $wingetList | ForEach-Object { Write-ColorOutput $Cyan "   $_" }
        } else {
            Write-ColorOutput $Yellow "⚠️  WiX not found in winget packages"
        }
    }
    catch {
        Write-ColorOutput $Yellow "⚠️  Winget not available or failed"
    }
    Write-ColorOutput $Blue ""
    
    # Recommendations
    Write-ColorOutput $Blue "💡 Recommendations:"
    Write-ColorOutput $Blue "==================="
    
    if ($foundInstallations.Count -eq 0) {
        Write-ColorOutput $Yellow "No WiX installations detected. Install WiX using one of these methods:"
        Write-ColorOutput $Cyan "  winget install --id WiXToolset.WiX"
        Write-ColorOutput $Cyan "  dotnet tool install --global wix"
        Write-ColorOutput $Cyan "  Download from: https://wixtoolset.org/releases/"
    } else {
        Write-ColorOutput $Green "WiX installations found but may not be in PATH."
        Write-ColorOutput $Yellow "Try adding one of these to your PATH:"
        $foundInstallations | ForEach-Object {
            $binPath = Join-Path $_ "bin"
            if (Test-Path $binPath) {
                Write-ColorOutput $Cyan "  $binPath"
            }
        }
    }
    
    Write-ColorOutput $Blue ""
    Write-ColorOutput $Blue "🔧 To add to PATH temporarily (current session):"
    Write-ColorOutput $Cyan '  $env:PATH += ";C:\Program Files\WiX Toolset v4\bin"'
    Write-ColorOutput $Blue ""
    Write-ColorOutput $Blue "🔧 To add to PATH permanently:"
    Write-ColorOutput $Cyan "  Use System Properties > Environment Variables"
    Write-ColorOutput $Cyan "  Or use: setx PATH `"%PATH%;C:\Program Files\WiX Toolset v4\bin`""
}

# Main execution
Test-WixInstallation

Write-ColorOutput $Blue ""
Write-ColorOutput $Green "🎯 Diagnostics completed!"
Write-ColorOutput $Yellow "If you found issues, follow the recommendations above and restart your terminal."