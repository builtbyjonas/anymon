#!/bin/sh
# Install the latest anymon release on Linux or macOS.
#
#   curl -fsSL https://anymon.xyz/install.sh | sh
#
# Environment variables:
#   ANYMON_VERSION         version to install, e.g. 1.0.0 (default: latest)
#   ANYMON_INSTALL_DIR     install directory
#                          (default: ~/.local/share/anymon on Linux,
#                           ~/Library/Application Support/anymon on macOS)
#   ANYMON_LIBC            Linux only: "musl" (default, static binary that runs
#                          on every distribution) or "gnu"
#   ANYMON_NO_MODIFY_PATH  set to 1 to not add the install directory to PATH
#
# The script is POSIX sh compatible, verifies the download's SHA-256 checksum
# and never asks for input, so it also works in CI and Dockerfiles.

set -eu

REPO="builtbyjonas/anymon"
DOCS="https://github.com/${REPO}/blob/main/docs/installation.md"

say() {
    printf 'anymon: %s\n' "$*"
}

fail() {
    printf 'anymon: error: %s\n' "$*" >&2
    exit 1
}

have() {
    command -v "$1" >/dev/null 2>&1
}

download() {
    # download <url> <file>
    if have curl; then
        curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
    elif have wget; then
        wget -q -O "$2" "$1"
    else
        fail "curl or wget is required"
    fi
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$arch" in
        x86_64 | amd64) arch="x86_64" ;;
        aarch64 | arm64) arch="aarch64" ;;
        *) fail "unsupported CPU architecture: $arch (see $DOCS to build from source)" ;;
    esac

    case "$os" in
        Linux)
            libc="${ANYMON_LIBC:-musl}"
            case "$libc" in
                musl | gnu) ;;
                *) fail "ANYMON_LIBC must be 'musl' or 'gnu', not '$libc'" ;;
            esac
            echo "${arch}-unknown-linux-${libc}"
            ;;
        Darwin)
            # A shell running under Rosetta reports x86_64 on Apple silicon.
            if [ "$arch" = "x86_64" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ]; then
                arch="aarch64"
            fi
            echo "${arch}-apple-darwin"
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT)
            fail "on Windows, run in PowerShell: irm https://anymon.xyz/install.ps1 | iex"
            ;;
        *)
            fail "unsupported operating system: $os (see $DOCS to build from source)"
            ;;
    esac
}

default_install_dir() {
    case "$(uname -s)" in
        Darwin) echo "${HOME}/Library/Application Support/anymon" ;;
        *) echo "${XDG_DATA_HOME:-${HOME}/.local/share}/anymon" ;;
    esac
}

sha256_of() {
    if have sha256sum; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif have shasum; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        echo ""
    fi
}

add_to_path() {
    dir="$1"
    case ":${PATH}:" in
        *":${dir}:"*) return 0 ;;
    esac
    if [ "${ANYMON_NO_MODIFY_PATH:-0}" = "1" ]; then
        say "add ${dir} to your PATH to use anymon"
        return 0
    fi

    shell_name="$(basename "${SHELL:-sh}")"
    case "$shell_name" in
        zsh) profile="${ZDOTDIR:-$HOME}/.zshrc"; line="export PATH=\"${dir}:\$PATH\"" ;;
        bash)
            if [ "$(uname -s)" = "Darwin" ]; then profile="${HOME}/.bash_profile"; else profile="${HOME}/.bashrc"; fi
            line="export PATH=\"${dir}:\$PATH\""
            ;;
        fish) profile="${HOME}/.config/fish/config.fish"; line="fish_add_path \"${dir}\"" ;;
        *) profile="${HOME}/.profile"; line="export PATH=\"${dir}:\$PATH\"" ;;
    esac

    if [ -f "$profile" ] && grep -F "$line" "$profile" >/dev/null 2>&1; then
        say "${dir} is already added to PATH in ${profile}; restart your shell to use anymon"
        return 0
    fi
    mkdir -p "$(dirname "$profile")"
    printf '\n# anymon\n%s\n' "$line" >> "$profile"
    say "added ${dir} to PATH in ${profile}"
    say "restart your shell or run:  ${line}"
}

main() {
    target="$(detect_target)"
    install_dir="${ANYMON_INSTALL_DIR:-$(default_install_dir)}"
    archive="anymon-${target}.tar.gz"

    if [ -n "${ANYMON_VERSION:-}" ]; then
        tag="v${ANYMON_VERSION#v}"
        url="https://github.com/${REPO}/releases/download/${tag}/${archive}"
        label="$tag"
    else
        url="https://github.com/${REPO}/releases/latest/download/${archive}"
        label="latest release"
    fi

    tmp="$(mktemp -d 2>/dev/null || mktemp -d -t anymon)"
    trap 'rm -rf "$tmp"' EXIT INT TERM

    say "downloading ${archive} (${label})"
    download "$url" "${tmp}/${archive}" || fail "download failed: ${url}
  There may be no prebuilt binary for ${target} in this release; see ${DOCS}"

    if download "${url}.sha256" "${tmp}/${archive}.sha256" 2>/dev/null; then
        expected="$(cut -d ' ' -f 1 < "${tmp}/${archive}.sha256")"
        actual="$(sha256_of "${tmp}/${archive}")"
        if [ -z "$actual" ]; then
            say "sha256sum/shasum not found; skipping checksum verification"
        elif [ "$expected" != "$actual" ]; then
            fail "checksum mismatch for ${archive} (expected ${expected}, got ${actual})"
        fi
    else
        say "no checksum published for this release; skipping verification"
    fi

    tar -xzf "${tmp}/${archive}" -C "$tmp"
    src="${tmp}/anymon-${target}"
    [ -f "${src}/anymon" ] || fail "the archive does not contain the anymon binary"

    mkdir -p "$install_dir"
    for bin in anymon anymon-shell; do
        if [ -f "${src}/${bin}" ]; then
            # Replace atomically so a running anymon is not disturbed.
            cp "${src}/${bin}" "${install_dir}/.${bin}.new"
            chmod 755 "${install_dir}/.${bin}.new"
            mv -f "${install_dir}/.${bin}.new" "${install_dir}/${bin}"
        fi
    done

    version="$("${install_dir}/anymon" --version 2>/dev/null || echo "anymon")"
    say "installed ${version} to ${install_dir}"
    add_to_path "$install_dir"
    say "get started:  anymon init  (or: anymon -e rs -- cargo run)"
}

main "$@"
