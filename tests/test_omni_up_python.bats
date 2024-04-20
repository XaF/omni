#!/usr/bin/env bats

load 'helpers/utils'

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
  version="latest"
  installed=false
  venv=true

  for arg in "$@"; do
    case $arg in
      version=*)
        version="${arg#version=}"
        shift
        ;;
      installed=*)
        installed="${arg#installed=}"
        shift
        ;;
      venv=*)
        venv="${arg#venv=}"
        shift
        ;;
      *)
        echo "Unknown argument: $arg"
        return 1
        ;;
    esac
  done

  if [ "$version" = "latest" ]; then
    version="3.12.2"
  fi

  add_command asdf update
  add_command asdf plugin list
  add_command asdf plugin add python
  add_command asdf plugin update python
  add_command asdf list all python <<EOF
2.5.1
2.5.2
2.5.3
2.5.4
2.5.5
2.5.6
2.6.0
2.6.1
2.6.2
2.6.3
2.6.4
2.6.5
2.6.6
2.6.7
2.6.8
2.6.9
3.10.11
3.10.12
3.10.13
3.10.14
3.11.0
3.11-dev
3.11.1
3.11.2
3.11.3
3.11.4
3.11.5
3.11.6
3.11.7
3.11.8
3.12.0
3.12-dev
3.12.1
3.12.2
3.13.0a5
3.13-dev
EOF
  if [ "$installed" = "true" ]; then
    add_command asdf list python "${version}" exit=0
  else
    add_command asdf list python "${version}" exit=1
    add_command asdf install python "${version}"
  fi

  if [ "$venv" = "true" ]; then
    add_fakebin "${HOME}/.local/share/omni/asdf/installs/python/${version}/bin/python"
    add_command python -m venv "regex:${HOME}/\.local/share/omni/wd/.*/python/${version}/root"
  fi
}

add_brew_python_calls() {
  local checked_prefix=false
  formulas=(autoconf coreutils curl libyaml openssl@3 readline pkg-config)
  for formula in "${formulas[@]}"; do
    add_command brew list --formula "${formula}" exit=1
    add_command brew install --formula "${formula}"
    add_command brew --prefix --installed "${formula}"
    if [ "$checked_prefix" = false ]; then
      checked_prefix=true
      add_command brew --prefix
    fi
  done
}

add_nix_python_calls() {
  local tmpdir="${TMPDIR:-/tmp}"
  # Make sure that tmpdir does not end with /
  tmpdir="${tmpdir%/}"

  local nix=(nix --extra-experimental-features "nix-command flakes")

  add_command "${nix[@]}" print-dev-env --verbose --print-build-logs --profile "regex:${tmpdir}/omni_up_nix\..*/profile" --impure --expr 'with import <nixpkgs> {}; mkShell { buildInputs = [ bzip2 gawk gcc gdbm gnumake gnused libffi lzma ncurses openssl pkg-config readline sqlite zlib ]; }'
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
  add_asdf_python_calls version=3.11.8

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
  add_asdf_python_calls version=3.11.8

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
  add_asdf_python_calls version=3.11.8

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "omni up python operation (2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_asdf_python_calls version=2.6.9 venv=false

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
  add_asdf_python_calls version=2.6.9 venv=false

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

