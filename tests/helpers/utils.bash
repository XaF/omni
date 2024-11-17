#!/usr/bin/env bash

omni_setup() {
  # echo "$BATS_RUN_TMPDIR" # The run directory
  # echo "$BATS_SUITE_TMPDIR" # Shared with the whole suite
  # echo "$BATS_FILE_TMPDIR" # Shared with the whole file
  # echo "$BATS_TEST_TMPDIR" # Only available to the current test
  # echo "$BATS_TEST_FILENAME" # The current test file

  # Get the git directory
  local git_dir="$(git rev-parse --show-toplevel 2>/dev/null)"
  export PROJECT_DIR="${PROJECT_DIR:-${git_dir}}"
  if [ -z "$PROJECT_DIR" ]; then
    echo "Could not find the project directory" >&2
    return 1
  fi

  # Set the fixtures directory
  export FIXTURES_DIR="${PROJECT_DIR}/tests/fixtures"

  if [[ -n "$OMNI_TEST_BIN" ]]; then
    echo "Using OMNI_TEST_BIN: ${OMNI_TEST_BIN}" >&2
    local test_bin_dir="$(dirname "${OMNI_TEST_BIN}")"
  else
    # Set the path to the debug binary
    local test_bin_dir="${git_dir}/target/debug"
    echo "Using debug binary: ${test_bin_dir}/omni" >&2

    # Make sure that the target/debug/omni binary exists, or build it
    NEEDS_BUILD=false
    if [ ! -f "${test_bin_dir}/omni" ]; then
      NEEDS_BUILD=true
    else
      # Check if the binary is older than the source files
      local newer_files="$(for dir in "${git_dir}/src" "${git_dir}/templates"; do
	find "$dir" -type f -newer "${test_bin_dir}/omni" -print0
      done)"
      if [[ -n "$newer_files" ]]; then
	  NEEDS_BUILD=true
      fi
    fi

    # Build the omni binary
    if [[ "$NEEDS_BUILD" == true ]]; then
      echo "Building omni binary in ${git_dir}" >&2
      (cd "${git_dir}" && cargo build) >&2 || { echo "ERROR building omni" >&2; return 1; }
    fi

    # # If the binary still does not exist, error out
    if [ ! -f "${test_bin_dir}/omni" ]; then
      echo "Could not find the omni binary at ${test_bin_dir}/omni" >&2
      return 1
    fi

    OMNI_TEST_BIN="${test_bin_dir}/omni"
  fi

  # Make sure that OMNI_TEST_BIN is an absolute path
  export OMNI_TEST_BIN="$(cd "$(dirname "$OMNI_TEST_BIN")" && pwd)/omni"

  # Override home directory for the test
  export HOME="${BATS_TEST_TMPDIR}"

  # Make sure that ${HOME} does not have any symlinks or some tests can be flaky
  export HOME="$(cd "${HOME}" && pwd -P)"

  # Let's unset the XDG variables to make sure that omni
  # does not use them for the tests
  unset XDG_CONFIG_HOME
  unset XDG_DATA_HOME
  unset XDG_CACHE_HOME
  unset XDG_RUNTIME_DIR

  # Let's unset other variables that could influence the tests
  unset HOMEBREW_PREFIX

  # Override global git configuration
  git config --global user.email "omni@potent.tool"
  git config --global user.name "omni"
  git config --global init.defaultBranch main

  # Disable the updates by default for the tests
  export OMNI_SKIP_UPDATE=true

  # Update the PATH to be only the system's binaries
  export PATH="$HOME/bin:/opt/homebrew/bin:/opt/homebrew/opt/coreutils/libexec/gnubin/:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
  echo "PATH is $PATH" >&2

  # Add omni's shell integration to the temporary directory
  echo "eval \"\$(\"${OMNI_TEST_BIN}\" hook init bash)\"" >> "${BATS_TEST_TMPDIR}/.bashrc" || echo "ERROR ?" >&2

  # Source the .bashrc
  source "${BATS_TEST_TMPDIR}/.bashrc" || echo -n >&2

  # Confirm omni's shell integration in case of error
  type omni >&2 || echo "ERROR: omni not found" >&2

  # Setup the fake binaries
  add_fakebin "${HOME}/bin/brew"
  add_fakebin "${HOME}/bin/nix"
  add_fakebin "${HOME}/.local/share/omni/asdf/bin/asdf"

  # Switch current directory to that new temp one
  cd "${HOME}"
}

setup_git_dir() {
  local dir="$1"
  local remote="$2"

  (
    mkdir -p "$dir" &&
    cd "$dir" &&
    git init &&
    echo "# This is git $(basename "$dir")" > README.md &&
    git add README.md &&
    git commit -m "Initial commit" &&
    git branch -M main &&
    git remote add origin "$remote"
  ) >/dev/null 3>&-
}

setup_omni_config() {
  local config_file="${HOME}/.config/omni/config.yaml"
  mkdir -p "$(dirname "${config_file}")"
  echo -n > "${config_file}"

  local HAS_REPO_PATH_FORMAT=false
  local HAS_WORKTREE=false

  for opt in "$@"; do
    case "${opt}" in
      with_org)
	cat <<EOF >>"${config_file}"
org:
  - handle: git@github.com:test1org
    trusted: true
  - handle: https://github.com/test2org
    trusted: true
  - handle:  https://bitbucket.org/test3org
    trusted: true
EOF
	;;
      repo_path_format=*)
	HAS_REPO_PATH_FORMAT=true
	echo "repo_path_format: \"${opt#*=}\"" >>"${config_file}"
	;;
      worktree=*)
	HAS_WORKTREE=true
	echo "worktree: \"${opt#*=}\"" >> "${config_file}"
	;;
      no_fast_search)
	echo "cd:" >> "${config_file}"
	echo "  fast_search: false" >> "${config_file}"
	;;
      *)
	echo "Unknown option: ${opt}" >&2
	return 1
	;;
    esac
  done

  if [[ "${HAS_REPO_PATH_FORMAT}" = false ]]; then
    echo 'repo_path_format: "%{host}/%{org}/%{repo}"' >> "${config_file}"
  fi

  if [[ "${HAS_WORKTREE}" = false ]]; then
    echo 'worktree: ~/git' >> "${config_file}"
  fi

  echo "==== OMNI CONFIG === BEGIN ====" >&2
  cat "${config_file}" >&2
  echo "==== OMNI CONFIG === END   ====" >&2
}

add_fakebin() {
  local target="${PROJECT_DIR}/tests/fixtures/bin/generic.sh"
  echo "fakebin target: ${target}" >&2
  ls -l "${target}" >&2

  local fakebin="$1"

  # Make sure the directory exists
  mkdir -p "$(dirname "${fakebin}")"

  # Create the symlink
  ln -s "${target}" "${fakebin}"

  echo "Created fake binary: ${fakebin}" >&2
  ls -l "${fakebin}" >&2
}

# Add an allowed command to the test
# Usage: add_command "command" "expected exit code" <<< "expected output"
# Example: add_command "brew install" exit=0 <<< "==> Installing"
add_command() {
  # Get the command
  cmd=("$@")

  # Get the special parameters
  exit_code=0
  required=1
  while [ 1 ]; do
    if [[ "${cmd[-1]}" == "--" ]]; then
      unset cmd[-1]
      break
    elif [[ "${cmd[-1]}" =~ ^exit=([0-9]+)$ ]]; then
      unset cmd[-1]
      exit_code="${BASH_REMATCH[1]}"
    elif [[ "${cmd[-1]}" =~ ^required=([0-1]+)$ ]]; then
      unset cmd[-1]
      required="${BASH_REMATCH[1]}"
    else
      break
    fi
  done

  # Get the binary
  binary="${cmd[0]}"

  # Get the args
  args=("${cmd[@]:1}")

  # Get the output from stdin but do not hang if there is no input,
  output=()
  if read -t 0; then
    # Read input but maintain line returns
    while read -r line; do
      output+=("${line}")
    done
  fi

  # Prepare the commands directory
  commands="${HOME}/.commands"
  mkdir -p "${commands}"

  # Get the position, looking at the files that currently exist
  position=0
  while [ -f "${commands}/${binary}_${position}" ]; do
    position=$((position + 1))
  done

  # Get the command file
  file="${commands}/${binary}_${position}"
  echo -n > "${file}"

  # Add the arguments to the command file
  for arg in "${args[@]}"; do
    echo -n "\"${arg}\" " >> "${file}"
  done
  echo >> "${file}"

  # Add the exit code to the command file
  echo "${exit_code}" >> "${file}"

  # Add the output to the command file
  if [[ "${#output[@]}" -gt 0 ]]; then
    echo >> "${file}"
    for line in "${output[@]}"; do
      echo "${line}" >> "${file}"
    done
  fi

  # Create a required file if necessary
  if [[ "${required}" -gt 0 ]]; then
    echo "${required}" > "${file}_required"
  fi

  # Check the written file
  echo "==== COMMAND FILE === BEGIN === ${binary}" >&2
  cat "${file}" >&2
  echo "==== COMMAND FILE === END   === ${binary}" >&2
}

# Check that all required commands have been called
check_commands() {
  local commands="${HOME}/.commands"
  if [ -d "$commands" ]; then
    # Print the called log
    local called_log="${commands}/called.log"
    if [ -f "$called_log" ]; then
      echo "==== CALLED LOG === BEGIN ===" >&2
      cat "$called_log" >&2
      echo "==== CALLED LOG === END   ===" >&2
    fi

    # Check for any unexpected commands
    local unexpected=0
    local unexpected_log="${commands}/unexpected.log"
    if [ -f "$unexpected_log" ]; then
      echo "==== UNEXPECTED LOG === BEGIN ===" >&2
      cat "$unexpected_log" >&2
      echo "==== UNEXPECTED LOG === END   ===" >&2
      unexpected=$(wc -l < "$unexpected_log")
      echo "Unexpected commands: $unexpected (should be 0)" >&2
    fi

    # Check that all required commands have been called
    local missing_required=0
    while read -r file; do
      missing_required=$((missing_required + 1))

      local dir=$(dirname "$file")
      local command_file=$(basename "$file" _required)
      local binary=${command_file%_*}
      local args=$(cat "${dir}/${command_file}" | head -n 1)

      echo "Missing required command call: $binary $args"
    done < <(find "$commands" -type f -name '*_required')

    # Return the status
    [ "$unexpected" -eq 0 ] && [ "$missing_required" -eq 0 ]
  fi
}
