# Chloride CLI/TUI

Rust CLI and terminal UI for Chloride file uploads, URL regeneration, quota checks, and local file browsing.

## Features

- Login/register against the Chloride API
- Persist auth config at `~/.config/chloride/config.json`
- Auto-refresh access tokens with refresh tokens
- List uploaded files by default in the TUI
- Switch to local file manager with `f`
- Upload files with inline expiry picker and progress UI
- Show storage quota and usage
- Regenerate raw presigned URLs for uploaded files

## API

Default API base URL:

```text
https://chloride.carbonkit.tech
```

The CLI stores this in the config file and uses bearer auth for protected API calls.

## Build

```bash
cargo build
```

Release build:

```bash
cargo build --release
```

The binary is configured as `cl`:

```text
target/release/cl
```

On Windows:

```text
target\release\cl.exe
```

## Usage

Launch the default TUI:

```bash
cl
```

or:

```bash
cl tui
```

Auth:

```bash
cl login
cl register
cl logout
cl whoami
```

Quota:

```bash
cl quota
```

Upload:

```bash
cl upload ./file.zip
```

Skip the interactive expiry picker:

```bash
cl upload ./file.zip --expires-in 604800
```

Regenerate raw presigned URLs:

```bash
cl regenerate
```

or by file ID:

```bash
cl regenerate 123
```

Local file commands:

```bash
cl touch file.txt
cl mkdir folder
cl rm file.txt
cl pwd
```

## TUI Keys

Default view is uploaded files from the API.

- `j` / `k` or arrow keys: move
- `Enter`: show selected file info
- `r`: refresh
- `f`: switch between remote files and local file manager
- `L`: login form
- `S`: register form
- `u`: quota overlay
- `q`: quit

Local file manager mode also supports:

- `t`: create file
- `m`: create directory
- `d`: delete selected item
- `Backspace` / `h`: go up

## Windows Installer

NSIS installer configuration lives at:

```text
installer/nsis/chloride-cli.nsi
```

Build the release binary first:

```powershell
cargo build --release
```

Then build the installer on Windows with NSIS:

```powershell
makensis installer\nsis\chloride-cli.nsi
```

The installer installs `cl.exe` to:

```text
%LOCALAPPDATA%\Programs\Chloride
```

It also adds that directory to the user `PATH`, so new terminals can run:

```powershell
cl
```

## Config

User config is stored at:

```text
~/.config/chloride/config.json
```

It contains the base URL, access token, refresh token, and cached user info.
