# WiX Toolset Compatibility Guide

## Overview

This project supports multiple versions of the WiX Toolset for creating Windows MSI installers. The build scripts automatically detect and adapt to different WiX versions.

## Supported WiX Versions

### ✅ WiX Toolset 6.x (Recommended)
- **Status**: Fully Supported
- **Namespace**: `http://wixtoolset.org/schemas/v4/wxs`
- **Install Command**: `winget install --id WiXToolset.WiX`
- **Binary**: `wix.exe`
- **Notes**: Latest version with modern syntax

### ✅ WiX Toolset 4.x
- **Status**: Fully Supported  
- **Namespace**: `http://wixtoolset.org/schemas/v4/wxs`
- **Install Command**: `winget install --id WiXToolset.WiX`
- **Binary**: `wix.exe`
- **Notes**: Uses same syntax as WiX 6.x

### ✅ WiX Toolset 3.x (Legacy)
- **Status**: Supported
- **Namespace**: `http://schemas.microsoft.com/wix/2006/wi`
- **Install Command**: Download from GitHub releases
- **Binaries**: `candle.exe` + `light.exe`
- **Notes**: Legacy version, requires different XML syntax

### ✅ WiX via .NET Global Tool
- **Status**: Supported
- **Install Command**: `dotnet tool install --global wix`
- **Binary**: `wix` (global tool)
- **Notes**: Cross-platform installation method

## Automatic Detection

The build scripts automatically detect WiX installations in this order:

1. **PATH Environment Variable**
   - Checks for `wix.exe` (WiX 4.x/6.x)
   - Checks for `candle.exe` + `light.exe` (WiX 3.x)

2. **Standard Installation Directories**
   - `C:\Program Files\WiX Toolset v6.0\bin`
   - `C:\Program Files\WiX Toolset v4\bin`
   - `C:\Program Files\WiX Toolset v3.11\bin`
   - `C:\Program Files\WiX Toolset v3.14\bin`
   - (Plus x86 variants)

3. **.NET Global Tools**
   - Checks `dotnet tool list --global` for WiX

4. **WinGet Package Locations**
   - Scans for WinGet-installed packages

## Version-Specific Differences

### XML Schema Changes

**WiX 3.x Syntax:**
```xml
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*" Name="App" ...>
    <Package InstallerVersion="200" Compressed="yes" />
    <!-- Content -->
  </Product>
</Wix>
```

**WiX 4.x/6.x Syntax:**
```xml
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="App" Version="1.0.0" ...>
    <SummaryInformation Description="..." />
    <!-- Content -->
  </Package>
</Wix>
```

### Build Commands

**WiX 3.x:**
```cmd
candle.exe source.wxs -out temp.wixobj
light.exe temp.wixobj -ext WixUIExtension -out installer.msi
```

**WiX 4.x/6.x:**
```cmd
wix.exe build source.wxs -out installer.msi
```

## Troubleshooting

### Detection Issues

**Problem**: "WiX Toolset not found"
```cmd
# Run diagnostic script
powershell -ExecutionPolicy Bypass -File installer/test-wix.ps1

# Manual PATH check
echo %PATH% | findstr /i wix

# Add to PATH temporarily
set PATH=%PATH%;C:\Program Files\WiX Toolset v6.0\bin
```

**Problem**: Wrong version detected
```cmd
# Check which WiX version you have
wix.exe --version     # WiX 4.x/6.x
candle.exe /?         # WiX 3.x

# Force specific version by modifying PATH
set PATH=C:\Program Files\WiX Toolset v6.0\bin;%PATH%
```

### Build Failures

**Problem**: "Cannot find input file"
- **Cause**: Running from wrong directory
- **Solution**: Ensure you're in project root or installer directory

**Problem**: "Incorrect namespace"
- **Cause**: WiX version mismatch with XML schema
- **Solution**: Build scripts automatically handle this

**Problem**: "MSI creation failed"
- **Cause**: Missing Rust executable or permissions
- **Solution**: Check that `cargo build --release` succeeds first

### Installation Issues

**Problem**: Commands not found after installation
- **Cause**: PATH not updated or terminal not restarted
- **Solution**: 
  1. Restart terminal/command prompt
  2. Check: `where chloride` and `where cl`
  3. Manually add to PATH if needed

**Problem**: Permission denied during installation
- **Cause**: Insufficient privileges for system-wide installation
- **Solution**: Run installer as Administrator

## Migration Between Versions

### From WiX 3.x to 4.x/6.x

The project automatically handles version differences, but if manually editing:

1. **Update namespace**:
   ```diff
   - <Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
   + <Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
   ```

2. **Convert Product to Package**:
   ```diff
   - <Product Id="*" Name="App" ...>
   -   <Package InstallerVersion="200" ... />
   + <Package Name="App" Version="1.0.0" ... >
   +   <SummaryInformation Description="..." />
   ```

3. **Update build commands**:
   ```diff
   - candle.exe + light.exe
   + wix.exe build
   ```

### From WiX 4.x to 6.x

No changes required - same syntax and commands.

## Best Practices

1. **Use WiX 6.x** for new projects (latest features and support)
2. **Test with multiple versions** if supporting older systems
3. **Use winget** for easiest installation experience
4. **Set explicit PATH** in CI/CD environments
5. **Run diagnostic script** when troubleshooting

## CI/CD Considerations

### GitHub Actions
```yaml
- name: Install WiX
  run: winget install --id WiXToolset.WiX

- name: Build Installer
  run: powershell -ExecutionPolicy Bypass -File installer/build-installer.ps1
```

### Azure DevOps
```yaml
- task: PowerShell@2
  displayName: 'Install WiX'
  inputs:
    script: 'winget install --id WiXToolset.WiX'

- task: PowerShell@2
  displayName: 'Build Installer'
  inputs:
    filePath: 'installer/build-installer.ps1'
```

## Version History

- **v6.0**: Current recommended version
- **v4.0**: Previous stable version  
- **v3.14**: Last 3.x release (legacy)
- **v3.11**: Common legacy version

## External Resources

- [WiX Toolset Official Site](https://wixtoolset.org/)
- [WiX 4.x Documentation](https://wixtoolset.org/docs/v4/)
- [WiX 3.x Documentation](https://wixtoolset.org/docs/v3/)
- [Migration Guide](https://wixtoolset.org/docs/v4/guides/migration/)