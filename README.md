# Chloride 🧪

A simple and powerful file management CLI tool for Windows.

[![Build Status](https://github.com/chloride-team/chloride/workflows/Build%20and%20Release/badge.svg)](https://github.com/chloride-team/chloride/actions)
[![Latest Release](https://img.shields.io/github/release/chloride-team/chloride.svg)](https://github.com/chloride-team/chloride/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- **Simple Commands**: Easy-to-use file operations
- **Dual Command Names**: Use either `chloride` or `cl` for convenience
- **Windows Integration**: Native MSI installer with PATH integration
- **Fast & Lightweight**: Built with Rust for optimal performance

## Installation

### Option 1: MSI Installer (Recommended)

1. Download the latest `chloride-{version}-installer.msi` from [Releases](https://github.com/chloride-team/chloride/releases)
2. Run the installer and follow the setup wizard
3. Restart your terminal
4. Start using `chloride` or `cl` commands

### Option 2: Portable Executable

1. Download `chloride-{version}-windows-x64.exe` from [Releases](https://github.com/chloride-team/chloride/releases)
2. Rename to `chloride.exe`
3. Place in a directory in your PATH

### Option 3: Build from Source

```cmd
git clone https://github.com/chloride-team/chloride.git
cd chloride
cargo build --release
```

## Usage

### Creating Files

```cmd
# Create a new file
chloride touch myfile.txt
cl touch document.docx

# Multiple files
chloride touch file1.txt file2.txt file3.txt
```

### Removing Files

```cmd
# Remove a file (with confirmation)
chloride rm oldfile.txt
cl rm unwanted.log

# Batch removal
chloride rm *.tmp
```

### Getting Help

```cmd
chloride --help
cl --help
chloride touch --help
```

### Dashboard

Run `chloride` without arguments to see the interactive dashboard:

```cmd
chloride
```

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [WiX Toolset](https://wixtoolset.org/) (for building installers)

### Building

```cmd
# Build debug version
cargo build

# Build release version
cargo build --release

# Run tests
cargo test

# Check formatting
cargo fmt --all -- --check

# Run linter
cargo clippy -- -D warnings
```

### Building Installer

The project includes sophisticated installer build scripts that support multiple WiX versions:

```cmd
# PowerShell (recommended)
cd installer
powershell -ExecutionPolicy Bypass -File build-installer.ps1

# Batch script
cd installer
build-installer.bat

# Test WiX compatibility
powershell -ExecutionPolicy Bypass -File test-wix.ps1
```

## GitHub Workflows

This project uses automated GitHub Actions workflows for continuous integration and releases:

### 🔄 Continuous Integration (`ci.yml`)
- **Triggers**: Push to main/develop, Pull Requests
- **Purpose**: Fast feedback on code quality
- Runs formatting checks, linting, tests
- Builds project and tests executable
- Performs security audits

### 🧪 Installer Testing (`test-installer.yml`)
- **Triggers**: Pull Requests affecting installer/source code
- **Purpose**: Validates installer build process
- Tests WiX compatibility across versions
- Creates and validates MSI installer
- Dry-run installation testing

### 🚀 Build and Release (`build-and-release.yml`)
- **Triggers**: Version tags (`v*`), Releases
- **Purpose**: Production builds and automated releases
- Builds MSI installer and portable executable
- Creates GitHub releases with download links
- Uploads release artifacts

### Creating a Release

#### Automatic (Recommended)
```cmd
# Use the helper script
.github/scripts/release.ps1 -Version "1.2.3"

# Or manually
git tag v1.2.3
git push origin v1.2.3
```

#### Manual via GitHub UI
1. Go to Releases → "Create a new release"
2. Create tag `v1.2.3`
3. Publish release
4. Artifacts are built and uploaded automatically

## Project Structure

```
chloride/
├── src/
│   └── main.rs              # Main application code
├── installer/
│   ├── build-installer.ps1  # PowerShell build script
│   ├── build-installer.bat  # Batch build script
│   ├── chloride.wxs         # WiX installer definition
│   ├── test-wix.ps1         # WiX compatibility testing
│   └── license.rtf          # License for installer
├── .github/
│   ├── workflows/           # GitHub Actions workflows
│   ├── scripts/             # Helper scripts
│   └── README.md            # Workflow documentation
├── Cargo.toml               # Rust project manifest
└── README.md                # This file
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes and test them
4. Ensure CI passes: `cargo test && cargo fmt && cargo clippy`
5. Commit your changes: `git commit -m 'Add amazing feature'`
6. Push to the branch: `git push origin feature/amazing-feature`
7. Open a Pull Request

### Development Workflow

- All PRs must pass CI checks
- Installer builds are tested automatically
- Use semantic version for releases
- Update CHANGELOG.md for notable changes

## Compatibility

- **OS**: Windows 10/11, Windows Server 2019+
- **Architecture**: x64
- **Installer**: WiX Toolset 3.x, 4.x, 6.x
- **Rust**: MSRV 1.70+

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support

- **Issues**: [GitHub Issues](https://github.com/chloride-team/chloride/issues)
- **Discussions**: [GitHub Discussions](https://github.com/chloride-team/chloride/discussions)
- **Wiki**: [Project Wiki](https://github.com/chloride-team/chloride/wiki)

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- Installer powered by [WiX Toolset](https://wixtoolset.org/)
- CI/CD via [GitHub Actions](https://github.com/features/actions)

---

Made with ❤️ by the Chloride Team
