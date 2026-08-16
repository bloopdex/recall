#!/bin/sh
# Recall install script (Linux/macOS) - copies the release binary into a
# user bin directory. Deliberately minimal and explicit:
#   - never touches shell profiles or rc files (prints the PATH guidance);
#   - never enables shell/git integrations - those are separate, explicit
#     commands (`recall shell install`, `recall git install`);
#   - never touches the database or the embedding model.
#
# Usage:
#   sh install.sh [release-dir]   # default: this script's directory
#   BINDIR=~/.local/bin sh install.sh /path/to/release
#
# If SHA256SUMS exists next to the binary, it is verified (override with
# SKIP_SHA_CHECK=1 only if you know what you are doing).

set -eu

echo "Recall - local-first engineering memory"
echo "Installing the recall binary ..."
echo ""

FROM="${1:-$(dirname "$0")}"
BINDIR="${BINDIR:-$HOME/.recall/bin}"
BINARY="$FROM/recall"
SUMS="$FROM/SHA256SUMS"
SHA_VERIFIED=0

if [ ! -f "$BINARY" ]; then
    echo "error: recall not found in '$FROM' - pass a release directory." >&2
    exit 1
fi

if [ -f "$SUMS" ] && [ "${SKIP_SHA_CHECK:-0}" != "1" ]; then
    expected=""
    while IFS= read -r line; do
        case "$line" in
            *"  recall") expected="${line%%  *}" ;;
        esac
    done < "$SUMS"
    if [ -z "$expected" ]; then
        echo "error: SHA256SUMS contains no entry for recall - refusing to install an unverifiable binary." >&2
        exit 1
    fi
    case "$(uname -s)" in
        Darwin) actual=$(shasum -a 256 "$BINARY" | awk '{print $1}') ;;
        *)
            actual=$(sha256sum "$BINARY" | awk '{print $1}')
            # GNU coreutils (MSYS2 / Git for Windows) print a leading '\'
            # before the hash when the path contains backslashes - the
            # documented escape marker for names needing disambiguation;
            # the 64-hex-char hash itself is untouched. Strip the marker
            # so the comparison is over hashes, not display artifacts.
            actual="${actual#\\}"
            ;;
    esac
    if [ "$actual" != "$expected" ]; then
        echo "error: checksum mismatch: expected $expected, got $actual - refusing to install." >&2
        exit 1
    fi
    echo "Checksum verified: $actual"
    SHA_VERIFIED=1
fi

mkdir -p "$BINDIR"
cp "$BINARY" "$BINDIR/recall"
chmod +x "$BINDIR/recall"

echo "Installed: $BINDIR/recall"
echo ""
echo "What changed:"
if [ "$SHA_VERIFIED" = "1" ]; then
    echo "  - recall copied into $BINDIR, checksum verified"
else
    echo "  - recall copied into $BINDIR (checksum check skipped)"
fi
echo "  - nothing else: PATH, shell profiles, hooks, database, and model were NOT touched"
echo ""
echo "Verify the install:"
echo "    $BINDIR/recall version"
echo ""
echo "To use recall from anywhere, add it to your PATH:"
echo "    export PATH=\"\$PATH:$BINDIR\"   # add to ~/.bashrc / ~/.zshrc"
echo "PATH note: on Linux/macOS recall does not modify your PATH or shell"
echo "profiles automatically (no per-user PATH API exists); the Windows"
echo "installer does add the user PATH, via the equivalent OS facility."
echo ""
echo "Next:"
echo "    recall capture    # remember how you solved your first problem"
echo ""
echo "Optional, explicit integrations (never enabled automatically):"
echo "    recall shell install   # prompt-hook failure capture"
echo "    recall git install     # post-commit hook (per repository)"
echo "    recall embeddings download  # one-time model download (the only network command)"
