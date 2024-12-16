#!/usr/bin/env bats

load 'helpers/utils'
load 'helpers/asdf'

setup() {
  # Setup the environment for the test; this should override $HOME too
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Add one repository
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"

  # Change directory to the repository
  cd "git/github.com/test1org/test1repo"
}

teardown() {
  check_commands
}

add_asdf_python_calls() {
  add_asdf_tool_calls tool=python venv=true "$@"
}

add_brew_python_calls() {
  add_asdf_tool_brew_calls python
}

add_nix_python_calls() {
  local tmpdir="${TMPDIR:-/tmp}"
  # Make sure that tmpdir does not end with /
  tmpdir="${tmpdir%/}"

  local nix=(nix --extra-experimental-features "nix-command flakes")

  add_command "${nix[@]}" print-dev-env --verbose --print-build-logs --profile "regex:${tmpdir}/omni_up_nix\..*/profile" --impure --expr 'with import <nixpkgs> {}; mkShell { buildInputs = [ bzip2 gawk gcc gdbm gnumake gnused libffi ncurses openssl pkg-config readline sqlite xz zlib ]; }'
  add_command "${nix[@]}" build --print-out-paths --out-link "regex:${HOME}/\.local/share/omni/wd/.*/nix/profile-pkgs-.*" "regex:${tmpdir}/omni_up_nix\..*/profile" <<EOF
#omni-test-bash-function
function process_response() {
  local args=("\$@")

  # Find the location of --out-link, and use the next argument
  local out_link=""
  for ((i=0; i<\${#args[@]}; i++)); do
    if [[ "\${args[i]}" == "--out-link" ]]; then
      out_link="\${args[i+1]}"
      break
    fi
  done

  # Write a fake nix profile to the file
  mkdir -p "\$(dirname "\$out_link")"
  echo '{"variables": {}}' > "\$out_link"
}
EOF
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (other versions installed)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls others_installed="3.11.6,3.11.8"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies and call pip (single requirements file)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      pip: requirements.txt
EOF

  add_brew_python_calls
  add_asdf_python_calls

  touch requirements.txt
  add_fakebin "${HOME}/bin/pip"
  add_command pip install -r requirements.txt

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies and call pip (multiple requirements file)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      pip:
      - requirements.txt
      - requirements2.txt
EOF

  add_brew_python_calls
  add_asdf_python_calls

  touch requirements.txt
  touch requirements2.txt
  add_fakebin "${HOME}/bin/pip"
  add_command pip install -r requirements.txt
  add_command pip install -r requirements2.txt

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (already installed)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls installed=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (already installed + other versions)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls installed=true others_installed="3.11.6,3.11.8"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (plugin already installed)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls plugin_list=installed

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (install fail fallback to matching installed version)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls installed=fail others_installed="3.11.6,3.11.8" fallback_version=3.11.8

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (cache versions hit)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls cache_versions=true list_versions=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (cache versions expired)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls cache_versions=expired

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (cache versions expired but list versions fail)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls cache_versions=expired list_versions=fail

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) using brew for dependencies (cache versions expired but plugin update fail)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls cache_versions=expired list_versions=fail-update

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) with upgrade configured for the python version" {
  cat > .omni.yaml <<EOF
up:
  - python:
      upgrade: true
EOF

  add_brew_python_calls
  add_asdf_python_calls upgrade=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) with upgrade configured at the work directory level" {
  cat > .omni.yaml <<EOF
up:
  - python

up_command:
  upgrade: true
EOF

  add_brew_python_calls
  add_asdf_python_calls upgrade=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) with upgrade configured as a command-line parameter" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls upgrade=true

  run omni up --trust --upgrade 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) with upgrade disabled and only an older major installed" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  # Expect that it won't match latest since older major, and install is required
  add_asdf_python_calls no_upgrade_installed="2.7.18"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (2) with upgrade disabled and a version 2 installed" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_asdf_python_calls installed=true list_versions=false version="2.7.9" no_upgrade_installed="2.7.9" venv=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (latest) with upgrade disabled and the current major installed" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_asdf_python_calls installed=true version="3.7.1" no_upgrade_installed="3.7.1"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (3) with upgrade disabled and a version 3 installed" {
  cat > .omni.yaml <<EOF
up:
  - python: 3
EOF

  add_brew_python_calls
  add_asdf_python_calls installed=true list_versions=false version="3.7.1" no_upgrade_installed="3.7.1"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (*) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: "*"
EOF

  add_brew_python_calls
  add_asdf_python_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (3.11.6) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 3.11.6
EOF

  add_brew_python_calls
  add_asdf_python_calls version=3.11.6

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (>=3.10, <3.11) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: ">=3.10, <3.11"
EOF

  add_brew_python_calls
  add_asdf_python_calls version=3.10.14

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (2.6.x || >3.10.12 <=3.11.2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: "2.6.x || >3.10.12 <=3.11.2"
EOF

  add_brew_python_calls
  add_asdf_python_calls version=3.11.2

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (~3.11.6) using brew for dependencies" {
  cat > .omni.yaml <<EOF
    up:
    - python: "~3.11.6"
EOF

  add_brew_python_calls
  add_asdf_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (3.11.x) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 3.11.x
EOF

  add_brew_python_calls
  add_asdf_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (3.11) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 3.11
EOF

  add_brew_python_calls
  add_asdf_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (2) using brew for dependencies (install fail does not fallback when no matching version installed)" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_asdf_python_calls version=2.7.18 venv=false installed=fail others_installed="3.11.6,3.11.8"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_asdf_python_calls version=2.7.18 venv=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (^2.5.2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: "^2.5.2"
EOF

  add_brew_python_calls
  add_asdf_python_calls version=2.7.18 venv=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:nix
@test "omni up python operation (latest) using nix for dependencies" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  preferred_tools:
  - nix
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_nix_python_calls
  add_asdf_python_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python
@test "omni up python operation (auto) with only the root directory" {
  cat > .omni.yaml <<EOF
up:
  - python: auto
EOF

  echo "3.11.9" > .python-version

  add_brew_python_calls
  add_asdf_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python
@test "omni up python operation (auto) with only a subdirectory" {
  cat > .omni.yaml <<EOF
up:
  - python: auto
EOF

  mkdir subdir
  echo "3.11.9" > subdir/.python-version

  add_brew_python_calls
  add_asdf_python_calls version=3.11.9 subdir=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python
@test "omni up python operation (auto) with multiple directories" {
  cat > .omni.yaml <<EOF
up:
  - python: auto
EOF

  echo "3.12.0" > .python-version
  mkdir subdir1
  echo "3.11.7" > subdir1/.python-version
  mkdir subdir2
  echo "python 3.11.9" > subdir2/.tool-versions

  add_brew_python_calls
  add_asdf_python_calls version=3.11.7 subdir=true
  add_asdf_python_calls version=3.11.9 subdir=true list_versions=false asdf_update=false plugin_list=skip
  add_asdf_python_calls version=3.12.0 list_versions=false asdf_update=false plugin_list=skip

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}
