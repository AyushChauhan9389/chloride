# CLAUDE.md — Chloride

This file provides guidance for AI assistants (Claude and others) working in this repository.

---

## Project Overview

**Chloride** is a Windows CLI tool written in Rust that brings Linux-style file management commands (`touch`, `rm`, `pwd`, `mkdir`) to Windows. It is distributed as a native MSI installer and as a portable executable.

- **Language**: Rust (Edition 2024)
- **Binary name**: `chloride` (also aliased as `cl` in the installer)
- **Target platform**: Windows 10/11, Windows Server 2019+ (x64)
- **MSRV**: Rust 1.70+
- **License**: MIT

---

## Repository Structure

```
chloride/
├── src/
│   └── main.rs              # All application logic (single-file Rust binary)
├── installer/
│   ├── chloride.wxs         # WiX installer definition (XML)
│   ├── build-installer.ps1  # PowerShell script to build the MSI
│   ├── build-installer.bat  # Batch wrapper for build script
│   ├── test-wix.ps1         # WiX installation diagnostics
│   └── license.rtf          # License displayed during MSI installation
├── .github/
│   └── workflows/
│       ├── ci.yml               # CI: fmt, clippy, test, build
│       ├── build-and-release.yml # Full build + GitHub Release automation
│       └── test-installer.yml   # MSI installer integration tests
├── Cargo.toml               # Package manifest and dependencies
├── Cargo.lock               # Locked dependency versions (commit this)
├── cfg.yaml                 # Setup commands (Rust install, system update)
└── README.md                # End-user documentation
```

---

## Architecture

### Single Entry Point

All application code lives in `src/main.rs`. There are no modules or subfiles — keep it this way unless the file grows significantly.

### CLI Structure (clap derive macros)

```rust
Cli
└── command: Option<Commands>
    ├── Touch   { filename }
    ├── Rm      { path, recursive (-r), force (-f) }
    ├── Pwd
    └── Mkdir   { dirname, parents (-p) }
```

When no subcommand is given, `show_dashboard()` prints a usage panel.

### Function Conventions

Each subcommand maps to a standalone function:

| Subcommand | Function              |
|------------|-----------------------|
| `touch`    | `touch_file()`        |
| `rm`       | `remove_path_cmd()`   |
| `pwd`      | `print_working_directory()` |
| `mkdir`    | `create_directory_cmd()` |
| *(none)*   | `show_dashboard()`    |

- Use `anyhow::Result<()>` as the return type for all command functions.
- Use `eprintln!()` for error output (not `println!()`).
- Use `println!()` with emoji prefixes for success/info messages (consistent with existing code).
- Interactive confirmations go through `io::stdin().read_line()` with explicit `io::stdout().flush()` before prompting.

### Adding a New Command

1. Add a variant to the `Commands` enum with `#[derive(Subcommand)]`.
2. Write a function `fn your_command(...) -> Result<()>`.
3. Add the `Some(Commands::YourVariant { ... }) => { your_command(...)?; }` arm in `main()`.
4. Add a documentation block in `show_dashboard()`.

---

## Dependencies

| Crate       | Version  | Purpose                          |
|-------------|----------|----------------------------------|
| `clap`      | 4.5.38   | CLI argument parsing (derive + cargo features) |
| `anyhow`    | 1.0.98   | Ergonomic error handling         |
| `serde`     | 1.0.219  | Serialization framework (derive) |
| `serde_yaml`| 0.9.34   | YAML parsing (for cfg.yaml)      |
| `tokio`     | 1.45.1   | Async runtime (full features)    |

> Note: `serde`, `serde_yaml`, and `tokio` are declared but not currently used in `src/main.rs`. They are reserved for future features. Do not remove them without checking open issues/PRs.

---

## Development Workflow

### Prerequisites

- Rust toolchain (stable) with `rustfmt` and `clippy` components
- Windows environment (CI runs on `windows-latest`; local dev on Windows recommended)

Install Rust:
```bash
rustup toolchain install stable
rustup component add rustfmt clippy
```

### Build

```bash
# Debug build
cargo build

# Release build (optimized, stripped)
cargo build --release
```

Release profile settings (`Cargo.toml`):
- `opt-level = 3` — maximum optimization
- `lto = true` — link-time optimization
- `strip = true` — strip debug symbols
- `panic = "abort"` — smaller binary, no unwinding

### Run

```bash
# Run directly
cargo run -- touch myfile.txt
cargo run -- rm -rf somedir
cargo run -- pwd
cargo run -- mkdir -p a/b/c

# Run compiled release binary
./target/release/chloride touch myfile.txt
```

### Test

```bash
cargo test --verbose
```

Tests are inline in `src/main.rs` using Rust's built-in `#[cfg(test)]` module.

### Lint & Format

```bash
# Check formatting (CI enforces this)
cargo fmt --all -- --check

# Auto-fix formatting
cargo fmt --all

# Lint (CI treats warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings
```

**Both `fmt` and `clippy` must pass cleanly before merging any PR.**

### Documentation

```bash
cargo doc --no-deps --document-private-items
```

---

## CI/CD Pipelines

### `ci.yml` — Continuous Integration

Triggers on push to `main`/`develop` and PRs to `main`.

| Job                 | Steps                                          |
|---------------------|------------------------------------------------|
| `test`              | fmt check → clippy → `cargo test` → `cargo build --release` → smoke test exe |
| `check-dependencies`| `cargo audit` (security) + `cargo outdated`    |
| `build-docs`        | `cargo doc`                                    |

### `build-and-release.yml` — Release Pipeline

Triggers on push to `main`/`develop`, version tags (`v*`), and release events.

| Job               | What it does                                                   |
|-------------------|----------------------------------------------------------------|
| `test`            | Tests + clippy                                                 |
| `build-installer` | Builds release binary, runs WiX MSI build, uploads artifacts  |
| `release`         | On version tags: creates GitHub Release, attaches MSI + exe   |

### `test-installer.yml` — Installer Tests

Triggers on PRs touching `src/`, `installer/`, `Cargo.*`, or workflow files.

Tests the full MSI build pipeline including WiX compatibility checks, extraction, metadata validation, and artifact upload (7-day retention).

---

## Building the MSI Installer

The installer uses WiX Toolset (supports v3.x, v4.x, and v6.x — auto-detected by the build script).

```powershell
# From the installer/ directory on Windows
./build-installer.ps1

# Diagnostic: check WiX installation
./test-wix.ps1
```

The built installer will be at `installer/chloride.msi`.

See `installer/WIX_COMPATIBILITY.md` for version-specific details and troubleshooting.

---

## Versioning & Releases

- Version is defined in `Cargo.toml` under `[package] version`.
- Releases are triggered by pushing a git tag matching `v*` (e.g., `v0.2.0`).
- The release workflow automatically creates a GitHub Release with the MSI and portable exe attached.
- **Always update the version in `Cargo.toml` before tagging a release.**

---

## Code Conventions

- **Error handling**: Use `anyhow::Result` and `?` propagation. Use `anyhow!("message")` for custom errors.
- **Output**: `println!()` for success/info, `eprintln!()` for errors. Prefix messages with relevant emoji to match existing style.
- **No unwrap/expect**: Avoid `.unwrap()` and `.expect()` in production paths; propagate errors properly.
- **Confirmation prompts**: Non-destructive by default — prompt the user unless `--force` is passed.
- **No async in current commands**: `tokio` is available but current commands are synchronous. Only introduce `async fn` if genuinely needed.
- **Single-file structure**: Keep logic in `src/main.rs` unless it grows meaningfully large (>500–600 lines is a reasonable threshold to consider splitting).

---

## Key Files to Know

| File | Why it matters |
|------|----------------|
| `src/main.rs` | Entire application logic |
| `Cargo.toml` | Dependency versions and release profile |
| `installer/chloride.wxs` | MSI installer definition — update version here too on releases |
| `.github/workflows/ci.yml` | What must pass for a PR to merge |
| `installer/build-installer.ps1` | How the MSI is built |

---

## Branch Strategy

- `main` — stable, release-ready code
- `develop` — integration branch for features
- Feature branches — named descriptively, merged via PR

CI runs on both `main` and `develop`. Releases are triggered from `main` via version tags.
