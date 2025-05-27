@echo off
setlocal enabledelayedexpansion

echo 🧪 Chloride MSI Installer Builder
echo ==================================
echo Batch Edition - Supports WiX 3.x and 4.x
echo.

REM Check if Rust is installed
cargo --version >nul 2>&1
if %errorlevel% neq 0 (
    echo ❌ Rust not found. Please install Rust first.
    echo    Download from: https://rustup.rs/
    pause
    exit /b 1
)
echo ✅ Rust found

REM Initialize WiX detection variables
set "WIX_VERSION="
set "WIX_PATH="
set "WIX_EXE="
set "CANDLE_EXE="
set "LIGHT_EXE="

echo 🔍 Detecting WiX Toolset installation...

REM Check if WiX 4.x is in PATH
wix.exe >nul 2>&1
if %errorlevel% equ 0 (
    echo ✅ WiX 4.x found in PATH
    set "WIX_VERSION=4"
    set "WIX_EXE=wix.exe"
    goto :wix_found
)

REM Check if WiX 3.x is in PATH
candle.exe >nul 2>&1
if %errorlevel% equ 0 (
    light.exe >nul 2>&1
    if !errorlevel! equ 0 (
        echo ✅ WiX 3.x found in PATH
        set "WIX_VERSION=3"
        set "CANDLE_EXE=candle.exe"
        set "LIGHT_EXE=light.exe"
        goto :wix_found
    )
)

echo    WiX not found in PATH, checking installation directories...

REM Check common WiX 4.x/6.x installation paths
if exist "%ProgramFiles%\WiX Toolset v6.0\bin\wix.exe" (
    echo ✅ WiX 6.x found in Program Files
    set "WIX_VERSION=4"
    set "WIX_PATH=%ProgramFiles%\WiX Toolset v6.0\bin"
    set "WIX_EXE=%ProgramFiles%\WiX Toolset v6.0\bin\wix.exe"
    goto :wix_found
)

if exist "%ProgramFiles(x86)%\WiX Toolset v6.0\bin\wix.exe" (
    echo ✅ WiX 6.x found in Program Files (x86)
    set "WIX_VERSION=4"
    set "WIX_PATH=%ProgramFiles(x86)%\WiX Toolset v6.0\bin"
    set "WIX_EXE=%ProgramFiles(x86)%\WiX Toolset v6.0\bin\wix.exe"
    goto :wix_found
)

if exist "%ProgramFiles%\WiX Toolset v4\bin\wix.exe" (
    echo ✅ WiX 4.x found in Program Files
    set "WIX_VERSION=4"
    set "WIX_PATH=%ProgramFiles%\WiX Toolset v4\bin"
    set "WIX_EXE=%ProgramFiles%\WiX Toolset v4\bin\wix.exe"
    goto :wix_found
)

if exist "%ProgramFiles(x86)%\WiX Toolset v4\bin\wix.exe" (
    echo ✅ WiX 4.x found in Program Files (x86)
    set "WIX_VERSION=4"
    set "WIX_PATH=%ProgramFiles(x86)%\WiX Toolset v4\bin"
    set "WIX_EXE=%ProgramFiles(x86)%\WiX Toolset v4\bin\wix.exe"
    goto :wix_found
)
</edits>

<edits>

<old_text>
echo 📦 Building MSI installer with WiX %WIX_VERSION%...

if "%WIX_VERSION%"=="3" (
    echo    Compiling WiX source...
    "%CANDLE_EXE%" chloride.wxs -out chloride.wixobj
    if !errorlevel! neq 0 (
        echo ❌ WiX compilation failed
        pause
        exit /b 1
    )

    echo    Linking and creating MSI...
    "%LIGHT_EXE%" chloride.wixobj -ext WixUIExtension -out chloride.msi
    if !errorlevel! neq 0 (
        echo ❌ MSI creation failed
        pause
        exit /b 1
    )
) else if "%WIX_VERSION%"=="4" (
    echo    Building MSI with WiX 4.x/6.x...
    "%WIX_EXE%" build chloride.wxs -out chloride.msi
    if !errorlevel! neq 0 (
        echo ❌ MSI creation failed
        pause
        exit /b 1
    )
) else if "%WIX_VERSION%"=="dotnet" (
    echo    Building MSI with dotnet wix tool...
    wix build chloride.wxs -out chloride.msi
    if !errorlevel! neq 0 (
        echo ❌ MSI creation failed
        pause
        exit /b 1
    )
)

REM Check common WiX 3.x installation paths
if exist "%ProgramFiles%\WiX Toolset v3.11\bin\candle.exe" (
    if exist "%ProgramFiles%\WiX Toolset v3.11\bin\light.exe" (
        echo ✅ WiX 3.x found in Program Files
        set "WIX_VERSION=3"
        set "WIX_PATH=%ProgramFiles%\WiX Toolset v3.11\bin"
        set "CANDLE_EXE=%ProgramFiles%\WiX Toolset v3.11\bin\candle.exe"
        set "LIGHT_EXE=%ProgramFiles%\WiX Toolset v3.11\bin\light.exe"
        goto :wix_found
    )
)

if exist "%ProgramFiles(x86)%\WiX Toolset v3.11\bin\candle.exe" (
    if exist "%ProgramFiles(x86)%\WiX Toolset v3.11\bin\light.exe" (
        echo ✅ WiX 3.x found in Program Files (x86)
        set "WIX_VERSION=3"
        set "WIX_PATH=%ProgramFiles(x86)%\WiX Toolset v3.11\bin"
        set "CANDLE_EXE=%ProgramFiles(x86)%\WiX Toolset v3.11\bin\candle.exe"
        set "LIGHT_EXE=%ProgramFiles(x86)%\WiX Toolset v3.11\bin\light.exe"
        goto :wix_found
    )
)

REM Check WiX 3.14 paths
if exist "%ProgramFiles%\WiX Toolset v3.14\bin\candle.exe" (
    if exist "%ProgramFiles%\WiX Toolset v3.14\bin\light.exe" (
        echo ✅ WiX 3.14 found in Program Files
        set "WIX_VERSION=3"
        set "WIX_PATH=%ProgramFiles%\WiX Toolset v3.14\bin"
        set "CANDLE_EXE=%ProgramFiles%\WiX Toolset v3.14\bin\candle.exe"
        set "LIGHT_EXE=%ProgramFiles%\WiX Toolset v3.14\bin\light.exe"
        goto :wix_found
    )
)

if exist "%ProgramFiles(x86)%\WiX Toolset v3.14\bin\candle.exe" (
    if exist "%ProgramFiles(x86)%\WiX Toolset v3.14\bin\light.exe" (
        echo ✅ WiX 3.14 found in Program Files (x86)
        set "WIX_VERSION=3"
        set "WIX_PATH=%ProgramFiles(x86)%\WiX Toolset v3.14\bin"
        set "CANDLE_EXE=%ProgramFiles(x86)%\WiX Toolset v3.14\bin\candle.exe"
        set "LIGHT_EXE=%ProgramFiles(x86)%\WiX Toolset v3.14\bin\light.exe"
        goto :wix_found
    )
)

REM Check dotnet global tools
dotnet tool list --global | findstr /i "wix" >nul 2>&1
if %errorlevel% equ 0 (
    echo ✅ WiX found as dotnet global tool
    set "WIX_VERSION=dotnet"
    set "WIX_EXE=wix"
    goto :wix_found
)

REM WiX not found
echo ❌ WiX Toolset not found!
echo.
echo 🔧 Installation options:
echo    Option 1 (Recommended): Install via Windows Package Manager
echo      winget install --id WiXToolset.WiX
echo.
echo    Option 2: Install via .NET Global Tool
echo      dotnet tool install --global wix
echo.
echo    Option 3: Download from official website
echo      https://wixtoolset.org/releases/
echo.
echo    Option 4: Install via Chocolatey
echo      choco install wixtoolset
echo.
echo After installation, restart your terminal and try again.
pause
exit /b 1

:wix_found
echo ✅ Found WiX Toolset %WIX_VERSION%

echo 🔨 Building Rust project...

REM Determine if we're in the installer directory or project root
for %%i in ("%CD%") do set "CURRENT_DIR=%%~ni"
if "%CURRENT_DIR%"=="installer" (
    cd ..
    set "PROJECT_ROOT=%CD%"
    set "INSTALLER_DIR=%CD%\installer"
) else (
    set "PROJECT_ROOT=%CD%"
    set "INSTALLER_DIR=%CD%\installer"
)

cargo build --release
if %errorlevel% neq 0 (
    echo ❌ Rust build failed
    pause
    exit /b 1
)

REM Verify the executable was created
if not exist "%PROJECT_ROOT%\target\release\chloride.exe" (
    echo ❌ Expected executable not found: %PROJECT_ROOT%\target\release\chloride.exe
    pause
    exit /b 1
)

cd "%INSTALLER_DIR%"
echo ✅ Rust project built successfully

REM Clean up any previous build artifacts
if exist "chloride.wixobj" del "chloride.wixobj" >nul 2>&1
if exist "chloride.wixpdb" del "chloride.wixpdb" >nul 2>&1
if exist "chloride.msi" del "chloride.msi" >nul 2>&1

echo 📦 Building MSI installer with WiX %WIX_VERSION%...

if "%WIX_VERSION%"=="3" (
    echo    Compiling WiX source...
    "%CANDLE_EXE%" chloride.wxs -out chloride.wixobj
    if !errorlevel! neq 0 (
        echo ❌ WiX compilation failed
        pause
        exit /b 1
    )

    echo    Linking and creating MSI...
    "%LIGHT_EXE%" chloride.wixobj -ext WixUIExtension -out chloride.msi
    if !errorlevel! neq 0 (
        echo ❌ MSI creation failed
        pause
        exit /b 1
    )
) else if "%WIX_VERSION%"=="4" (
    echo    Building MSI with WiX 4.x...
    "%WIX_EXE%" build chloride.wxs -ext WixToolset.UI.wixext -out chloride.msi
    if !errorlevel! neq 0 (
        echo ❌ MSI creation failed
        pause
        exit /b 1
    )
) else if "%WIX_VERSION%"=="dotnet" (
    echo    Building MSI with dotnet wix tool...
    wix build chloride.wxs -ext WixToolset.UI.wixext -out chloride.msi
    if !errorlevel! neq 0 (
        echo ❌ MSI creation failed
        pause
        exit /b 1
    )
)

REM Clean up intermediate files
if exist "chloride.wixobj" del "chloride.wixobj" >nul 2>&1
if exist "chloride.wixpdb" del "chloride.wixpdb" >nul 2>&1

REM Verify MSI was created
if not exist "chloride.msi" (
    echo ❌ MSI file was not created
    pause
    exit /b 1
)

echo ✅ MSI installer created successfully: chloride.msi
echo.
echo 📋 Installation Instructions:
echo    1. Run the installer: chloride.msi
echo    2. Follow the installation wizard
echo    3. After installation, open a new command prompt
echo    4. Use 'chloride' or 'cl' commands from anywhere
echo.
echo 🎉 Example usage after installation:
echo    chloride touch myfile.txt
echo    cl rm oldfile.txt
echo    chloride --help
echo.
echo 💡 Tips:
echo    • The installer adds Chloride to your system PATH
echo    • You can use either 'chloride' or 'cl' as the command
echo    • Restart your terminal after installation for PATH changes to take effect
echo.
echo 🧪 Testing installer (optional):
echo    To test the installer without installing:
echo    msiexec /a chloride.msi /qb TARGETDIR="%CD%\test_install"
echo.
echo 🎯 Build completed successfully!
echo 📦 Your installer is ready: chloride.msi
pause