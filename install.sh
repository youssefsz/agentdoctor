#!/usr/bin/env sh
set -eu

repo="${AGENTDOCTOR_REPO:-youssefsz/agentdoctor}"
install_dir="${AGENTDOCTOR_INSTALL_DIR:-$HOME/.local/bin}"
tmp_dir="${TMPDIR:-/tmp}/agentdoctor-install-$$"
no_path_update="${AGENTDOCTOR_NO_PATH_UPDATE:-}"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' was not found" >&2
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
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64 | amd64) arch_part="x86_64" ;;
    arm64 | aarch64) arch_part="aarch64" ;;
    *)
      echo "error: unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  printf "%s-%s" "$arch_part" "$os_part"
}

profile_file() {
  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    zsh) printf "%s/.zshrc" "$HOME" ;;
    bash) printf "%s/.bashrc" "$HOME" ;;
    *) printf "%s/.profile" "$HOME" ;;
  esac
}

configure_path() {
  case ":$PATH:" in
    *":$install_dir:"*) return 0 ;;
  esac

  if [ -n "$no_path_update" ]; then
    echo "Add this to your shell profile:"
    echo "  export PATH=\"$install_dir:\$PATH\""
    return 0
  fi

  profile="$(profile_file)"
  marker="# Added by AgentDoctor installer"
  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if ! grep -F "$marker" "$profile" >/dev/null 2>&1; then
    {
      printf "\n%s\n" "$marker"
      printf "case \":\$PATH:\" in\n"
      printf "  *\":%s:\"*) ;;\n" "$install_dir"
      printf "  *) export PATH=\"%s:\$PATH\" ;;\n" "$install_dir"
      printf "esac\n"
    } >> "$profile"
    echo "Added $install_dir to PATH in $profile"
  fi
}

need curl
need tar
need uname

target="$(detect_target)"
api_url="https://api.github.com/repos/$repo/releases/latest"
asset="agentdoctor-"

mkdir -p "$tmp_dir" "$install_dir"

echo "Fetching latest AgentDoctor release for $target..."
download_url="$(
  curl -fsSL "$api_url" \
    | sed -n 's/.*"browser_download_url": "\(.*agentdoctor-.*-'"$target"'\.tar\.gz\)".*/\1/p' \
    | head -n 1
)"

if [ -z "$download_url" ]; then
  echo "error: could not find a release asset for $target in $repo" >&2
  exit 1
fi

archive="$tmp_dir/agentdoctor.tar.gz"
curl -fsSL "$download_url" -o "$archive"
tar -xzf "$archive" -C "$tmp_dir"

binary="$(find "$tmp_dir" -type f -name agentdoctor | head -n 1)"
if [ -z "$binary" ]; then
  echo "error: release archive did not contain agentdoctor" >&2
  exit 1
fi

chmod +x "$binary"
cp "$binary" "$install_dir/agentdoctor"
configure_path

echo "AgentDoctor installed to $install_dir/agentdoctor"
echo "Open a new terminal if the agentdoctor command is not visible yet."
