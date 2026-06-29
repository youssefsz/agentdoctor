#!/usr/bin/env sh
set -eu

repo="${AGENTDOCTOR_REPO:-youssefsz/agentdoctor}"
install_dir="${AGENTDOCTOR_INSTALL_DIR:-}"
install_mode="${AGENTDOCTOR_INSTALL_MODE:-user}"
tmp_dir="${TMPDIR:-/tmp}/agentdoctor-install-$$"
no_path_update="${AGENTDOCTOR_NO_PATH_UPDATE:-}"
source_only="${AGENTDOCTOR_INSTALLER_SOURCE_ONLY:-}"
install_dir_was_on_path=0

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

has_command() {
  command -v "$1" >/dev/null 2>&1
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

path_contains_dir() {
  case ":$PATH:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

default_install_dir() {
  case "$install_mode" in
    user) user_install_dir ;;
    system) system_install_dir ;;
    *)
      echo "error: AGENTDOCTOR_INSTALL_MODE must be 'user' or 'system'" >&2
      exit 1
      ;;
  esac
}

user_install_dir() {
  for candidate in "$HOME/.local/bin" "$HOME/bin" "$HOME/.cargo/bin"; do
    if path_contains_dir "$candidate"; then
      printf "%s" "$candidate"
      return 0
    fi
  done

  printf "%s/.local/bin" "$HOME"
}

system_install_dir() {
  os="$(uname_s)"
  if [ "$os" = "Darwin" ]; then
    for candidate in /usr/local/bin /opt/homebrew/bin; do
      if path_contains_dir "$candidate"; then
        printf "%s" "$candidate"
        return 0
      fi
    done
  else
    for candidate in /usr/local/bin; do
      if path_contains_dir "$candidate"; then
        printf "%s" "$candidate"
        return 0
      fi
    done
  fi

  printf "/usr/local/bin"
}

resolve_install_dir() {
  if [ -z "$install_dir" ]; then
    install_dir="$(default_install_dir)"
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
  resolve_install_dir
  printf "case \":\$PATH:\" in\n"
  printf "  *\":%s:\"*) ;;\n" "$install_dir"
  printf "  *) export PATH=\"%s:\$PATH\" ;;\n" "$install_dir"
  printf "esac\n"
}

append_path_block() {
  resolve_install_dir
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
  resolve_install_dir
  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
      PATH="$install_dir:$PATH"
      export PATH
      ;;
  esac
}

can_animate() {
  [ -t 2 ] && [ "${TERM:-}" != "dumb" ]
}

use_color() {
  can_animate && [ -z "${NO_COLOR:-}" ]
}

color_text() {
  code="$1"
  text="$2"

  if use_color; then
    printf "\033[%sm%s\033[0m" "$code" "$text"
  else
    printf "%s" "$text"
  fi
}

spinner_frame() {
  case "$1" in
    0) printf "-" ;;
    1) printf "\\" ;;
    2) printf "|" ;;
    *) printf "/" ;;
  esac
}

run_step() {
  message="$1"
  shift

  if can_animate; then
    "$@" &
    pid="$!"
    frame_index=0

    while kill -0 "$pid" 2>/dev/null; do
      frame="$(spinner_frame "$frame_index")"
      printf "\r%s %s" "$(color_text "36" "$frame")" "$(color_text "1" "$message")" >&2
      frame_index=$(((frame_index + 1) % 4))
      sleep 0.1
    done

    if wait "$pid"; then
      printf "\r%s %s\n" "$(color_text "32" "[ok]")" "$message" >&2
    else
      status="$?"
      printf "\r%s %s\n" "$(color_text "31" "[failed]")" "$message" >&2
      return "$status"
    fi
  else
    echo "$message"
    "$@"
  fi
}

configure_path() {
  resolve_install_dir

  if path_contains_dir "$install_dir"; then
    return 0
  fi

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

resolve_latest_release() {
  release_tag="$(
    curl -fsSL "https://api.github.com/repos/$repo/releases/latest" \
      | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1
  )"

  if [ -z "$release_tag" ]; then
    echo "error: could not resolve the latest release tag for $repo" >&2
    exit 1
  fi

  printf "%s" "$release_tag" > "$release_tag_file"
}

download_archive() {
  if ! curl -fsSL "$download_url" -o "$archive"; then
    echo "error: failed to download expected release asset:" >&2
    echo "  $download_url" >&2
    echo "Check that release $release_tag contains $archive_name." >&2
    exit 1
  fi
}

extract_archive() {
  tar -xzf "$archive" -C "$tmp_dir"
}

allow_sudo() {
  [ "$install_mode" = "system" ] || [ "${AGENTDOCTOR_ALLOW_SUDO:-}" = "1" ]
}

install_binary() {
  resolve_install_dir

  if mkdir -p "$install_dir" 2>/dev/null && [ -w "$install_dir" ]; then
    chmod +x "$binary"
    cp "$binary" "$install_dir/agentdoctor"
    chmod 755 "$install_dir/agentdoctor"
    return 0
  fi

  if ! allow_sudo; then
    echo "error: $install_dir is not writable." >&2
    echo "Choose a user-writable install dir with AGENTDOCTOR_INSTALL_DIR, or set AGENTDOCTOR_INSTALL_MODE=system to allow sudo." >&2
    exit 1
  fi

  if ! has_command sudo; then
    echo "error: $install_dir is not writable and sudo was not found" >&2
    exit 1
  fi

  echo "Installing to $install_dir requires administrator permission." >&2
  sudo mkdir -p "$install_dir"
  sudo cp "$binary" "$install_dir/agentdoctor"
  sudo chmod 755 "$install_dir/agentdoctor"
}

verify_install() {
  resolve_install_dir
  installed="$("$install_dir/agentdoctor" --version 2>/dev/null || true)"
  if [ -z "$installed" ]; then
    echo "error: installed binary did not run: $install_dir/agentdoctor" >&2
    exit 1
  fi

  echo "Installed $installed"

  if [ "$install_dir_was_on_path" -eq 1 ]; then
    echo "agentdoctor is available on PATH."
  else
    echo "agentdoctor will be available after your shell reloads its PATH."
  fi
}

main() {
  need curl
  need tar
  need uname

  target="$(detect_target)"
  release_tag_file="$tmp_dir/latest-release-tag.txt"
  resolve_install_dir
  if path_contains_dir "$install_dir"; then
    install_dir_was_on_path=1
  fi

  mkdir -p "$tmp_dir"

  color_text "1;36" "AgentDoctor installer"
  echo
  run_step "Resolving latest release for $target" resolve_latest_release
  release_tag="$(cat "$release_tag_file")"
  archive_name="agentdoctor-$release_tag-$target.tar.gz"
  download_url="https://github.com/$repo/releases/download/$release_tag/$archive_name"
  archive="$tmp_dir/$archive_name"

  run_step "Downloading $archive_name" download_archive
  run_step "Extracting release archive" extract_archive

  binary="$(find "$tmp_dir" -type f -name agentdoctor | head -n 1)"
  if [ -z "$binary" ]; then
    echo "error: release archive did not contain agentdoctor" >&2
    exit 1
  fi

  run_step "Installing binary" install_binary
  configure_path
  verify_install

  echo "AgentDoctor installed to $install_dir/agentdoctor"
  if [ "$install_dir_was_on_path" -eq 1 ]; then
    echo "Run: agentdoctor --version"
  else
    echo "Open a new terminal or run this now:"
    echo "  export PATH=\"$install_dir:\$PATH\""
  fi
}

if [ -z "$source_only" ]; then
  main "$@"
fi
