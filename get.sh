#!/bin/sh
# Thallium Flash: sign in, download and flash Thallium 81 with one command.
#   curl -fsSL https://raw.githubusercontent.com/thallium-os/thallium-flash/main/get.sh | sh
# Installs nothing system-wide: thallium-flash (and cloudflared, if missing)
# go in ~/.local/share/thallium-flash. Downloads are invite-only.
set -eu

REPO="thallium-os/thallium-flash"
CF="https://github.com/cloudflare/cloudflared/releases/latest/download"
DIR="${XDG_DATA_HOME:-$HOME/.local/share}/thallium-flash"
mkdir -p "$DIR"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)  tf="thallium-flash-linux-x86_64"; cf="cloudflared-linux-amd64" ;;
  Darwin-arm64)  tf="thallium-flash-macos-arm64";  cf="cloudflared-darwin-arm64.tgz" ;;
  *) echo "Sorry, $(uname -s) $(uname -m) isn't supported yet (Linux x86-64 and Apple Silicon Macs are)." >&2; exit 1 ;;
esac

fetch() {
  curl -fL --progress-bar -o "$2.part" "$1"
  mv "$2.part" "$2"
}

echo "Getting Thallium Flash…"
fetch "https://github.com/$REPO/releases/latest/download/$tf" "$DIR/thallium-flash"
chmod 755 "$DIR/thallium-flash"

if ! command -v cloudflared >/dev/null 2>&1 && [ ! -x "$DIR/cloudflared" ]; then
  echo "Getting cloudflared (handles the sign-in)…"
  case "$cf" in
    *.tgz) fetch "$CF/$cf" "$DIR/cloudflared.tgz"; tar -xzf "$DIR/cloudflared.tgz" -C "$DIR"; rm -f "$DIR/cloudflared.tgz" ;;
    *)     fetch "$CF/$cf" "$DIR/cloudflared" ;;
  esac
  chmod 755 "$DIR/cloudflared"
fi
PATH="$DIR:$PATH"
export PATH

if [ "$(uname -s)" = "Linux" ]; then
  # Opens the page in the browser: sign in, pick the USB drive, press Flash.
  exec "$DIR/thallium-flash" web
fi

ISO="$HOME/Downloads/thallium-81-amd64.iso"
"$DIR/thallium-flash" flash "$ISO"
echo
echo "Saved and verified: $ISO"
echo "Write it to a USB drive with balenaEtcher (https://etcher.balena.io)."
