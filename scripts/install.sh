#!/usr/bin/env sh
set -eu

APP_NAME="kagi"
REPO="Microck/kagi-cli"
BINDIR="${KAGI_INSTALL_DIR:-$HOME/.local/bin}"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command not found: $1" >&2
    exit 1
  fi
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *)
      echo "error: unsupported operating system: $os" >&2
      echo "use a GitHub Releases asset manually for this platform." >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch_part="x86_64" ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *)
      echo "error: unsupported architecture: $arch" >&2
      echo "use a GitHub Releases asset manually for this platform." >&2
      exit 1
      ;;
  esac

  printf '%s-%s' "$arch_part" "$os_part"
}

fetch_latest_tag() {
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1
}

need_cmd curl
need_cmd tar
need_cmd mktemp

target="$(detect_target)"
version="${KAGI_VERSION:-$(fetch_latest_tag)}"

if [ -z "$version" ]; then
  echo "error: could not resolve the latest release tag." >&2
  echo "set KAGI_VERSION explicitly and retry." >&2
  exit 1
fi

archive="${APP_NAME}-${version}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${version}/${archive}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT INT TERM

echo "Downloading $archive"
curl -fL "$url" -o "$tmpdir/$archive"

tar -xzf "$tmpdir/$archive" -C "$tmpdir"
mkdir -p "$BINDIR"
cp "$tmpdir/$APP_NAME" "$BINDIR/$APP_NAME"
chmod +x "$BINDIR/$APP_NAME"

echo "Installed $APP_NAME to $BINDIR/$APP_NAME"

case ":$PATH:" in
  *":$BINDIR:"*) ;;
  *)
    echo
    echo "Add $BINDIR to your PATH if it is not already there."
    ;;
esac

echo
echo "Run:"
echo "  $APP_NAME --help"
