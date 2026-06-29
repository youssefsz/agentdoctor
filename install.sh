#!/usr/bin/env sh
set -eu

repo="${AGENTDOCTOR_REPO:-youssefsz/agentdoctor}"
install_dir="${AGENTDOCTOR_INSTALL_DIR:-$HOME/.local/bin}"
tmp_dir="${TMPDIR:-/tmp}/agentdoctor-install-$$"
no_path_update="${AGENTDOCTOR_NO_PATH_UPDATE:-}"
source_only="${AGENTDOCTOR_INSTALLER_SOURCE_ONLY:-}"

cleanup() {
  rm -rf "$tmp_dir"
}

if [ -z "$source_only" ]; then
  trap cleanup EXIT INT TERM
fi

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' was not found" >&2
    exit 1
  fi
}

detect_target() {
  os="$(uname_s)"
  arch="$(uname_m)"

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

uname_s() {
  if [ -n "${AGENTDOCTOR_TEST_UNAME_S:-}" ]; then
    printf "%s" "$AGENTDOCTOR_TEST_UNAME_S"
  else
    uname -s
  fi
}

uname_m() {
  if [ -n "${AGENTDOCTOR_TEST_UNAME_M:-}" ]; then
    printf "%s" "$AGENTDOCTOR_TEST_UNAME_M"
  else
    uname -m
  fi
}

profile_files() {
  os="$(uname_s)"
  shell_name="$(basename "${SHELL:-}")"

  case "$shell_name" in
    zsh)
      if [ "$os" = "Darwin" ]; then
        printf "%s\n" "$HOME/.zprofile" "$HOME/.zshrc"
      else
        printf "%s\n" "$HOME/.zshrc" "$HOME/.profile"
      fi
      ;;
    bash)
      if [ "$os" = "Darwin" ]; then
        bash_login_profile
        printf "%s\n" "$HOME/.bashrc"
      else
        bash_login_profile
        printf "%s\n" "$HOME/.bashrc"
      fi
      ;;
    *) printf "%s\n" "$HOME/.profile" ;;
  esac
}

bash_login_profile() {
  for candidate in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
    if [ -f "$candidate" ]; then
      printf "%s\n" "$candidate"
      return 0
    fi
  done

  os="$(uname_s)"
  if [ "$os" = "Darwin" ]; then
    printf "%s\n" "$HOME/.bash_profile"
  else
    printf "%s\n" "$HOME/.profile"
  fi
}

path_snippet() {
  printf "case \":\$PATH:\" in\n"
  printf "  *\":%s:\"*) ;;\n" "$install_dir"
  printf "  *) export PATH=\"%s:\$PATH\" ;;\n" "$install_dir"
  printf "esac\n"
}

append_path_block() {
  profile="$1"
  marker="# Added by AgentDoctor installer"
  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if grep -F "$marker" "$profile" >/dev/null 2>&1; then
    return 0
  fi

  {
    printf "\n%s\n" "$marker"
    path_snippet
  } >> "$profile"
  echo "Added $install_dir to PATH in $profile"
}

ensure_current_path() {
  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
      PATH="$install_dir:$PATH"
      export PATH
      ;;
  esac
}

configure_path() {
  if [ -n "$no_path_update" ]; then
    case ":$PATH:" in
      *":$install_dir:"*) ;;
      *)
        echo "Add this to your shell profile:"
        echo "  export PATH=\"$install_dir:\$PATH\""
        ;;
    esac
    return 0
  fi

  profile_files | while IFS= read -r profile; do
    [ -n "$profile" ] || continue
    append_path_block "$profile"
  done

  ensure_current_path
}

verify_install() {
  installed="$("$install_dir/agentdoctor" --version 2>/dev/null || true)"
  if [ -z "$installed" ]; then
    echo "error: installed binary did not run: $install_dir/agentdoctor" >&2
    exit 1
  fi

  echo "Installed $installed"

  if command -v agentdoctor >/dev/null 2>&1; then
    echo "agentdoctor is available in this installer shell."
  else
    echo "agentdoctor was installed, but $install_dir is not on PATH in this shell."
  fi
}

main() {
  need curl
  need tar
  need uname

  target="$(detect_target)"
  api_url="https://api.github.com/repos/$repo/releases/latest"

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
  verify_install

  echo "AgentDoctor installed to $install_dir/agentdoctor"
  echo "Open a new terminal or run this now:"
  echo "  export PATH=\"$install_dir:\$PATH\""
}

if [ -z "$source_only" ]; then
  main "$@"
fi
