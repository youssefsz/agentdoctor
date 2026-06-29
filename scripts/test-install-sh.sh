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

test_macos_zsh_profiles
test_linux_bash_profiles

echo "install.sh profile tests passed"
