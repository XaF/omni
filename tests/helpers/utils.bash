#!/usr/bin/env bash

omni_setup() {
  # echo "$BATS_RUN_TMPDIR" # The run directory
  # echo "$BATS_SUITE_TMPDIR" # Shared with the whole suite
  # echo "$BATS_FILE_TMPDIR" # Shared with the whole file
  # echo "$BATS_TEST_TMPDIR" # Only available to the current test

  # Inject a fake `tput` bin in a directory that is in the PATH
  # This is to avoid the following error when running the tests:
  #  tput: No value for $TERM and no -T specified
  mkdir -p "${BATS_SUITE_TMPDIR}/bin"
  if [ ! -f "${BATS_SUITE_TMPDIR}/bin/tput" ]; then
    cat >"${BATS_SUITE_TMPDIR}/bin/tput" <<EOF
#!/usr/bin/env bash
# Fake tput command to avoid errors when running the tests
echo \${COLUMNS:-100}
EOF
    chmod +x "${BATS_SUITE_TMPDIR}/bin/tput"
  fi

  # Get the test script directory
  local tests_dir="$(dirname "${BATS_TEST_FILENAME}")"

  # The parent directory is the project root
  local project_root="$(dirname "$tests_dir")"

  # Set OMNIDIR to the temporary directory path
  export OMNIDIR="${BATS_SUITE_TMPDIR}/omni"

  # OMNIDIR ready file
  local readyfile="${BATS_SUITE_TMPDIR}/.omnidir.ready"

  if [ ! -d "${OMNIDIR}" ]; then
    echo "Setting up omni directory in ${OMNIDIR}"

    # We want a working copy of omni to be able to run the tests,
    # but to make sure we don't have any extra, unexpected things
    # in there, let's only copy what is currently in the git tree
    # but also what is currently staged for changes
    (
      git -C "$project_root" ls-files &&
      git -C "$project_root" diff --name-only --cached
    ) | while read -r file; do
      mkdir -p "${OMNIDIR}/$(dirname "$file")"
      cp --archive "$project_root/$file" "${OMNIDIR}/$file"
    done

    # We also want to copy the vendor directory, so we have all
    # the dependencies available for the tests
    if [ -d "${project_root}/vendor" ]; then
      ln -s "${project_root}/vendor" "${OMNIDIR}/vendor"
    fi

    # We also want to make sure we have the ruby version file
    if [ -f "${project_root}/.ruby-version" ]; then
      ln -s "${project_root}/.ruby-version" "${OMNIDIR}/.ruby-version"
    fi

    # Since we did not copy the .git directory, omni won't try to
    # update itself while running commands; if this is something
    # we want to do in a command, we will have to explicitly make
    # this directory a repository.
    touch "${readyfile}"
  fi

  # Reset all OMNI_*, RBENV_*, GOENV_* variables
  for var in $(env | grep -E '(OMNI|RBENV|GOENV)' | cut -d= -f1 | grep -v '^OMNIDIR$'); do
    unset "$var"
  done

  # Get the rbenv bin dir
  local rbenv_bin_dir="$(dirname "$(which rbenv)")"

  # Copy .rbenv directory to new HOME
  cp -r "${HOME}/.rbenv" "${BATS_TEST_TMPDIR}/.rbenv"

  # Override home directory for the test
  export HOME="${BATS_TEST_TMPDIR}"

  # Override global git configuration
  git config --global user.email "omni@potent.tool"
  git config --global user.name "omni"
  git config --global init.defaultBranch main

  # Set OMNI_GIT work tree
  export OMNI_GIT="${HOME}/git"

  # Disable the updates by default for the tests
  export OMNI_SKIP_UPDATE=true

  # Update the PATH to be only the system's binaries, and
  # the bin directory of omni
  export PATH="${BATS_SUITE_TMPDIR}/bin:${OMNIDIR}/bin:${rbenv_bin_dir}:/opt/homebrew/bin:/opt/homebrew/opt/coreutils/libexec/gnubin/:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

  # Add omni's shell integration to the temporary directory
  echo "[[ -x \"${OMNIDIR}/shell_integration/omni.bash\" ]] && source \"${OMNIDIR}/shell_integration/omni.bash\"" >> "${BATS_TEST_TMPDIR}/.bashrc" || echo "ERROR ?"

  # Source the .bashrc
  source "${BATS_TEST_TMPDIR}/.bashrc"

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

