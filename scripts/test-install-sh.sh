#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/.." && pwd)"

fail() {
  echo "error: $*" >&2
  exit 1
}

assert_file_contains() {
  file="$1"
  expected="$2"
  [ -f "$file" ] || fail "missing file: $file"
  grep -F "$expected" "$file" >/dev/null 2>&1 || fail "missing '$expected' in $file"
}

test_macos_zsh_profiles() (
  tmp="${TMPDIR:-/tmp}/agentdoctor-install-test-zsh-$$"
  rm -rf "$tmp"
  mkdir -p "$tmp/home"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  HOME="$tmp/home"
  SHELL="/bin/zsh"
  PATH="/usr/bin:/bin"
  AGENTDOCTOR_TEST_UNAME_S="Darwin"
  AGENTDOCTOR_TEST_UNAME_M="arm64"
  AGENTDOCTOR_INSTALLER_SOURCE_ONLY=1
  export HOME SHELL PATH AGENTDOCTOR_TEST_UNAME_S AGENTDOCTOR_TEST_UNAME_M AGENTDOCTOR_INSTALLER_SOURCE_ONLY

  . "$repo_root/install.sh"

  [ "$(detect_target)" = "aarch64-apple-darwin" ] || fail "macOS arm64 target detection failed"
  configure_path

  assert_file_contains "$HOME/.zprofile" "# Added by AgentDoctor installer"
  assert_file_contains "$HOME/.zprofile" "$HOME/.local/bin"
  assert_file_contains "$HOME/.zshrc" "# Added by AgentDoctor installer"
  assert_file_contains "$HOME/.zshrc" "$HOME/.local/bin"

  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) fail "configure_path did not update current PATH" ;;
  esac
)

test_linux_bash_profiles() (
  tmp="${TMPDIR:-/tmp}/agentdoctor-install-test-bash-$$"
  rm -rf "$tmp"
  mkdir -p "$tmp/home"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  HOME="$tmp/home"
  SHELL="/bin/bash"
  PATH="/usr/bin:/bin"
  AGENTDOCTOR_TEST_UNAME_S="Linux"
  AGENTDOCTOR_TEST_UNAME_M="x86_64"
  AGENTDOCTOR_INSTALLER_SOURCE_ONLY=1
  export HOME SHELL PATH AGENTDOCTOR_TEST_UNAME_S AGENTDOCTOR_TEST_UNAME_M AGENTDOCTOR_INSTALLER_SOURCE_ONLY

  . "$repo_root/install.sh"

  [ "$(detect_target)" = "x86_64-unknown-linux-gnu" ] || fail "Linux x86_64 target detection failed"
  configure_path

  assert_file_contains "$HOME/.profile" "# Added by AgentDoctor installer"
  assert_file_contains "$HOME/.bashrc" "# Added by AgentDoctor installer"
)

test_macos_default_install_dir_prefers_user_directory() (
  tmp="${TMPDIR:-/tmp}/agentdoctor-install-test-default-dir-$$"
  rm -rf "$tmp"
  mkdir -p "$tmp/home"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  HOME="$tmp/home"
  SHELL="/bin/zsh"
  PATH="/usr/local/bin:/usr/bin:/bin"
  AGENTDOCTOR_TEST_UNAME_S="Darwin"
  AGENTDOCTOR_TEST_UNAME_M="arm64"
  AGENTDOCTOR_INSTALLER_SOURCE_ONLY=1
  export HOME SHELL PATH AGENTDOCTOR_TEST_UNAME_S AGENTDOCTOR_TEST_UNAME_M AGENTDOCTOR_INSTALLER_SOURCE_ONLY

  . "$repo_root/install.sh"
  resolve_install_dir

  [ "$install_dir" = "$HOME/.local/bin" ] || fail "macOS default install dir should avoid sudo-only system directories"
)

test_system_install_mode_prefers_path_system_directory() (
  tmp="${TMPDIR:-/tmp}/agentdoctor-install-test-system-dir-$$"
  rm -rf "$tmp"
  mkdir -p "$tmp/home"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  HOME="$tmp/home"
  SHELL="/bin/zsh"
  PATH="/usr/local/bin:/usr/bin:/bin"
  AGENTDOCTOR_TEST_UNAME_S="Darwin"
  AGENTDOCTOR_TEST_UNAME_M="arm64"
  AGENTDOCTOR_INSTALL_MODE="system"
  AGENTDOCTOR_INSTALLER_SOURCE_ONLY=1
  export HOME SHELL PATH AGENTDOCTOR_TEST_UNAME_S AGENTDOCTOR_TEST_UNAME_M AGENTDOCTOR_INSTALL_MODE AGENTDOCTOR_INSTALLER_SOURCE_ONLY

  . "$repo_root/install.sh"
  resolve_install_dir

  [ "$install_dir" = "/usr/local/bin" ] || fail "system install mode should prefer /usr/local/bin on macOS"
)

test_run_step_executes_command_without_tty() (
  tmp="${TMPDIR:-/tmp}/agentdoctor-install-test-step-$$"
  rm -rf "$tmp"
  mkdir -p "$tmp"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  HOME="$tmp/home"
  SHELL="/bin/sh"
  AGENTDOCTOR_INSTALLER_SOURCE_ONLY=1
  export HOME SHELL AGENTDOCTOR_INSTALLER_SOURCE_ONLY

  . "$repo_root/install.sh"

  run_step "Creating marker" sh -c 'printf "ok" > "$1"' sh "$tmp/marker"
  assert_file_contains "$tmp/marker" "ok"
)

test_release_tag_json_parser_matches_github_payload_shape() (
  tag="$(
    printf '%s\n' '{"tag_name":"v0.1.6","assets":[]}' \
      | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1
  )"

  [ "$tag" = "v0.1.6" ] || fail "tag_name parser did not match compact GitHub payload"
)

test_macos_zsh_profiles
test_linux_bash_profiles
test_macos_default_install_dir_prefers_user_directory
test_system_install_mode_prefers_path_system_directory
test_run_step_executes_command_without_tty
test_release_tag_json_parser_matches_github_payload_shape

echo "install.sh profile tests passed"
