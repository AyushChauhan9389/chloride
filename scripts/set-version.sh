#!/bin/sh
# Write a version into every file that carries one.
#
#   scripts/set-version.sh 1.2.3
#   scripts/set-version.sh --self-test
#
# The release workflow calls this twice: once before building (so the binary
# reports the tag) and once after publishing (so main's files match what was
# released). Add any new version-carrying file to apply_all() below and both
# paths pick it up.
set -eu

# Cargo.toml — the [package] version, which is the first `version = ` line.
set_cargo_toml() {
    awk -v v="$2" '
        !done && /^version = / { print "version = \"" v "\""; done = 1; next }
        { print }
    ' "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

# Cargo.lock — the `version` belonging to our own package entry, not the 200
# dependency entries that look identical.
set_cargo_lock() {
    awk -v v="$2" '
        /^name = "chloride-tui"$/ { print; found = 1; next }
        found && /^version = / { print "version = \"" v "\""; found = 0; next }
        { print }
    ' "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

# NSIS installer — !define APP_VERSION "x.y.z"
set_nsis() {
    awk -v v="$2" '
        /^!define APP_VERSION / { print "!define APP_VERSION \"" v "\""; next }
        { print }
    ' "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

apply_all() {
    root="$1"
    version="$2"
    set_cargo_toml "$root/Cargo.toml" "$version"
    set_cargo_lock "$root/Cargo.lock" "$version"
    set_nsis "$root/installer/nsis/chloride-cli.nsi" "$version"
}

# --- self-test ---

if [ "${1:-}" = "--self-test" ]; then
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    mkdir -p "$tmp/installer/nsis"

    cat > "$tmp/Cargo.toml" <<'EOF'
[package]
name = "chloride-tui"
version = "1.0.0"
edition = "2024"

[dependencies]
anyhow = "1.0.103"
version = "should not be touched"
EOF

    cat > "$tmp/Cargo.lock" <<'EOF'
[[package]]
name = "anyhow"
version = "1.0.103"

[[package]]
name = "chloride-tui"
version = "1.0.0"
dependencies = [
 "anyhow",
]

[[package]]
name = "zerocopy"
version = "0.8.27"
EOF

    cat > "$tmp/installer/nsis/chloride-cli.nsi" <<'EOF'
!define APP_NAME "Chloride"
!define APP_VERSION "0.1.0"
OutFile "chloride-cli-setup-${APP_VERSION}.exe"
EOF

    apply_all "$tmp" 9.8.7

    fail=0
    expect() { # expect <description> <actual> <wanted>
        if [ "$2" != "$3" ]; then
            echo "FAIL: $1: got '$2', want '$3'" >&2
            fail=1
        fi
    }

    expect "Cargo.toml package version" \
        "$(awk '/^version = /{print; exit}' "$tmp/Cargo.toml")" 'version = "9.8.7"'
    expect "Cargo.toml leaves later version keys alone" \
        "$(grep -c 'should not be touched' "$tmp/Cargo.toml")" 1
    expect "Cargo.lock own package" \
        "$(awk '/^name = "chloride-tui"$/{getline; print}' "$tmp/Cargo.lock")" 'version = "9.8.7"'
    expect "Cargo.lock leaves anyhow alone" \
        "$(awk '/^name = "anyhow"$/{getline; print}' "$tmp/Cargo.lock")" 'version = "1.0.103"'
    expect "Cargo.lock leaves zerocopy alone" \
        "$(awk '/^name = "zerocopy"$/{getline; print}' "$tmp/Cargo.lock")" 'version = "0.8.27"'
    expect "NSIS APP_VERSION" \
        "$(grep '^!define APP_VERSION' "$tmp/installer/nsis/chloride-cli.nsi")" \
        '!define APP_VERSION "9.8.7"'
    expect "NSIS OutFile still interpolates" \
        "$(grep -c 'chloride-cli-setup-${APP_VERSION}.exe' "$tmp/installer/nsis/chloride-cli.nsi")" 1

    [ "$fail" = 0 ] && echo "set-version: all cases pass"
    exit "$fail"
fi

# --- main ---

version="${1:-}"
if [ -z "$version" ]; then
    echo "usage: $0 <version>   (e.g. 1.2.3, no leading v)" >&2
    exit 1
fi
version="${version#v}"

case "$version" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *)
        echo "error: '$version' is not a major.minor.patch version" >&2
        exit 1
        ;;
esac

apply_all "$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)" "$version"
echo "set version to $version"
