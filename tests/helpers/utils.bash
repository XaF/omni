#!/usr/bin/env bash

omni_setup() {
  # echo "$BATS_RUN_TMPDIR" # The run directory
  # echo "$BATS_SUITE_TMPDIR" # Shared with the whole suite
  # echo "$BATS_FILE_TMPDIR" # Shared with the whole file
  # echo "$BATS_TEST_TMPDIR" # Only available to the current test
  # echo "$BATS_TEST_FILENAME" # The current test file

  # Get the git directory
  local git_dir="$(git rev-parse --show-toplevel 2>/dev/null)"

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
      local newer_files="$(for dir in "${git_dir}/src" "${git_dir}/shell_integration"; do
	find "$dir" -type f -newer "${test_bin_dir}/omni" -print0
      done)"
      if [[ -n "$newer_files" ]]; then
	  NEEDS_BUILD=true
      fi
    fi

    # Build the omni binary
    if [[ "$NEEDS_BUILD" == true ]]; then
      echo "Building omni binary in ${git_dir}" >&2
      (cd "${git_dir}" && cargo build) >&2 || echo "ERROR building omni" >&2
    fi

    # # If the binary still does not exist, error out
    if [ ! -f "${test_bin_dir}/omni" ]; then
      echo "Could not find the omni binary at ${test_bin_dir}/omni" >&2
      return 1
    fi
  fi

  # Override home directory for the test
  export HOME="${BATS_TEST_TMPDIR}"

  # Override global git configuration
  git config --global user.email "omni@potent.tool"
  git config --global user.name "omni"
  git config --global init.defaultBranch main

  # Disable the updates by default for the tests
  export OMNI_SKIP_UPDATE=true

  # Update the PATH to be only the system's binaries, and
  # the bin directory of omni
  export PATH="${test_bin_dir}:/opt/homebrew/bin:/opt/homebrew/opt/coreutils/libexec/gnubin/:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

  # Add omni's shell integration to the temporary directory
  echo "command -v omni >/dev/null && eval \"\$(omni hook init bash)\"" >> "${BATS_TEST_TMPDIR}/.bashrc" || echo "ERROR ?" >&2

  # Source the .bashrc
  source "${BATS_TEST_TMPDIR}/.bashrc" || echo -n >&2

  # Switch current directory to that new temp one
  cd "${BATS_TEST_TMPDIR}"
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
}

