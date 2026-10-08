#!/usr/bin/env bash
# Installs sfxc.app to ~/Applications and sfxc-cli to ~/.local/bin.
#
#   curl -fsSL https://raw.githubusercontent.com/sarafanovn/sfxc/main/install.sh | bash
#
# Flags: --app  --cli  --all   skip the menu and install these
#        --source              build from source instead of downloading a release
#        --uninstall           remove what this script installed (the library is kept)
set -euo pipefail

REPO="${SFXC_REPO:-sarafanovn/sfxc}"
APP_DIR="$HOME/Applications"
BIN_DIR="$HOME/.local/bin"

want_app=1 want_cli=1 from_source=0 uninstall=0 flagged=0
for arg in "$@"; do
  case "$arg" in
    --app|--cli)
      if [ "$flagged" = 0 ]; then want_app=0; want_cli=0; flagged=1; fi
      if [ "$arg" = --app ]; then want_app=1; else want_cli=1; fi ;;
    --all) flagged=1; want_app=1; want_cli=1 ;;
    --source) from_source=1 ;;
    --uninstall) uninstall=1 ;;
    -h|--help) say "usage: install.sh [--app] [--cli] [--all] [--source] [--uninstall]"; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

if [ -t 1 ]; then b=$'\033[1m' d=$'\033[2m' g=$'\033[32m' r=$'\033[31m' z=$'\033[0m'; else b= d= g= r= z=; fi
say() { printf '%s\n' "$*"; }
die() { printf '%serror:%s %s\n' "$r" "$z" "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || die "sfxc is packaged for macOS only."

have_tty=0
{ : </dev/tty; } 2>/dev/null && have_tty=1

# Two toggles. Up/down moves, space flips, enter confirms, q quits.
menu() {
  local sel=0 key rest
  local labels=("sfxc.app   the desktop app   → ~/Applications" "sfxc-cli   the terminal tool  → ~/.local/bin")
  tput civis 2>/dev/null || true
  trap 'tput cnorm 2>/dev/null || true' RETURN
  say "${b}What should be installed?${z}  ${d}↑↓ move · space toggle · enter confirm${z}"
  while :; do
    local on=("$want_app" "$want_cli") i
    for i in 0 1; do
      local box="[ ]"; [ "${on[$i]}" = 1 ] && box="[${g}x${z}]"
      local ptr=" "; [ "$sel" = "$i" ] && ptr="${b}>${z}"
      printf '\r\033[K %s %s %s\n' "$ptr" "$box" "${labels[$i]}"
    done
    IFS= read -rsn1 key </dev/tty || exit 1
    case "$key" in
      $'\033') read -rsn2 rest </dev/tty || true
               case "$rest" in '[A'|'[B') sel=$((1 - sel)) ;; esac ;;
      ' ') if [ "$sel" = 0 ]; then want_app=$((1 - want_app)); else want_cli=$((1 - want_cli)); fi ;;
      '') [ "$want_app$want_cli" = 00 ] && continue; break ;;
      q|Q) say "Cancelled."; exit 0 ;;
    esac
    printf '\033[2A'
  done
}

if [ "$uninstall" = 1 ]; then
  rm -rf "$APP_DIR/sfxc.app" "$BIN_DIR/sfxc-cli"
  say "${g}Removed${z} sfxc.app and sfxc-cli. Your library in ~/Library/Application Support/sfxc is untouched."
  exit 0
fi

if [ "$flagged" = 0 ] && [ "$have_tty" = 1 ]; then menu; fi

arch="$(uname -m)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$APP_DIR" "$BIN_DIR"

# Release assets: sfxc-app-<arch>.zip (contains sfxc.app) and sfxc-cli-<arch> (single binary).
fetch() { curl -fsSL "https://github.com/$REPO/releases/latest/download/$1" -o "$2" 2>/dev/null; }

built=0
build_tree() {
  [ "$built" = 1 ] && return
  command -v cargo >/dev/null || die "No release found for $arch and Rust is not installed. Get it at https://rustup.rs and run this again."
  local here; here="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
  if [ -f "$here/Cargo.toml" ] && [ -d "$here/crates/sfxc-cli" ]; then
    SRC="$here"
  else
    command -v git >/dev/null || die "git is required to build from source."
    say "Cloning $REPO..."
    git clone -q --depth 1 "https://github.com/$REPO.git" "$work/src"
    SRC="$work/src"
  fi
  built=1
}

install_app() {
  say "${b}sfxc.app${z}"
  local got=0
  if [ "$from_source" = 0 ] && fetch "sfxc-app-$arch.zip" "$work/app.zip"; then
    rm -rf "$work/app" && mkdir "$work/app" && ditto -x -k "$work/app.zip" "$work/app" && got=1
    rm -rf "$APP_DIR/sfxc.app" && mv "$work/app/sfxc.app" "$APP_DIR/sfxc.app"
  fi
  if [ "$got" = 0 ]; then
    say "  building from source (a few minutes)..."
    build_tree
    (cd "$SRC" && ./scripts/bundle.sh >/dev/null)
    rm -rf "$APP_DIR/sfxc.app" && cp -R "$SRC/target/sfxc.app" "$APP_DIR/sfxc.app"
  fi
  xattr -dr com.apple.quarantine "$APP_DIR/sfxc.app" 2>/dev/null || true
  say "  ${g}installed${z} $APP_DIR/sfxc.app"
}

install_cli() {
  say "${b}sfxc-cli${z}"
  local got=0
  if [ "$from_source" = 0 ] && fetch "sfxc-cli-$arch" "$work/cli"; then
    install -m 755 "$work/cli" "$BIN_DIR/sfxc-cli" && got=1
  fi
  if [ "$got" = 0 ]; then
    say "  building from source..."
    build_tree
    (cd "$SRC" && cargo build --release -q -p sfxc-cli)
    install -m 755 "$SRC/target/release/sfxc-cli" "$BIN_DIR/sfxc-cli"
  fi
  say "  ${g}installed${z} $BIN_DIR/sfxc-cli"
}

[ "$want_app" = 1 ] && install_app
[ "$want_cli" = 1 ] && install_cli

if [ "$want_cli" = 1 ]; then
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
      line='export PATH="$HOME/.local/bin:$PATH"'
      say ""
      say "$BIN_DIR is not on your PATH."
      if [ "$have_tty" = 1 ]; then
        printf 'Add it to ~/.zshrc? [Y/n] '
        read -r ans </dev/tty || ans=n
        case "$ans" in
          n|N) say "Add this line yourself: $line" ;;
          *) printf '\n%s\n' "$line" >> "$HOME/.zshrc"; say "Added. Open a new terminal to pick it up." ;;
        esac
      else
        say "Add this line to ~/.zshrc: $line"
      fi ;;
  esac
fi

say ""
say "${g}Done.${z}"
[ "$want_app" = 1 ] && say "  open ~/Applications/sfxc.app"
[ "$want_cli" = 1 ] && say "  sfxc-cli schema"
exit 0
