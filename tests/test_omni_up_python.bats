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
  fallback_version=
  plugin_installed=false
  installed=false
  others_installed=false
  venv=true
  cache_versions=false
  list_versions=true
  upgrade=false
  no_upgrade_installed=false

  for arg in "$@"; do
    case $arg in
      version=*)
        version="${arg#version=}"
        shift
        ;;
      fallback_version=*)
        fallback_version="${arg#fallback_version=}"
        shift
        ;;
      plugin_installed=*)
        plugin_installed="${arg#plugin_installed=}"
        shift
        ;;
      installed=*)
        installed="${arg#installed=}"
        shift
        ;;
      others_installed=*)
        others_installed="${arg#others_installed=}"
        shift
        ;;
      venv=*)
        venv="${arg#venv=}"
        shift
        ;;
      cache_versions=*)
        cache_versions="${arg#cache_versions=}"
        shift
        ;;
      list_versions=*)
        list_versions="${arg#list_versions=}"
        shift
        ;;
      upgrade=*)
        upgrade="${arg#upgrade=}"
        shift
        ;;
      no_upgrade_installed=*)
        no_upgrade_installed="${arg#no_upgrade_installed=}"
        shift
        ;;
      *)
        echo "Unknown argument: $arg"
        return 1
        ;;
    esac
  done

  if [ "$version" = "latest" ]; then
    version="3.12.3"
  fi

  if [ "$cache_versions" = "true" ] || [ "$cache_versions" = "expired" ]; then
    if [ "$cache_versions" = "true" ]; then
      date=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    else
      # This allows to support GNU date but also BSD date
      date=$(date -u +"%Y-%m-%dT%H:%M:%SZ" -d "300 days ago" 2>/dev/null ||
             date -u -v-300d +"%Y-%m-%dT%H:%M:%SZ")
    fi
    mkdir -p "${HOME}/.cache/omni"
    perl -pe 's/{{ UPDATED_AT }}/'"${date}"'/g' "${PROJECT_DIR}/tests/fixtures/asdf_operation_cache.json" > "${HOME}/.cache/omni/asdf_operation.json"
  fi

  add_command asdf update

  if [ "$plugin_installed" = "true" ]; then
    add_command asdf plugin list
    add_command asdf plugin add python
  else
    add_command asdf plugin list <<EOF
python
EOF
  fi

  if [ "$upgrade" = "false" ]; then
    if [ "$no_upgrade_installed" = "false" ]; then
      add_command asdf list python
    else
      add_command asdf list python <<EOF
$(for v in $(echo "${no_upgrade_installed}" | perl -pe 's/,+/,/g' | sort -u); do echo "  ${v}"; done)
EOF
    fi
  fi

  if [ "$list_versions" = "true" ]; then
    add_command asdf plugin update python
    add_command asdf list all python <"${PROJECT_DIR}/tests/fixtures/python-versions.txt"
  elif [ "$list_versions" = "fail-update" ]; then
    add_command asdf plugin update python exit=1
  elif [ "$list_versions" = "fail" ]; then
    add_command asdf plugin update python
    add_command asdf list all python exit=1
  fi

  if [ "$installed" = "true" ]; then
    installed_versions=$(echo "${others_installed},${no_upgrade_installed},${version}" | \
      perl -pe 's/(^|,)false(?=,|$)/,/g' | \
      perl -pe 's/,+/\n/g' | \
      perl -pe '/^$/d' | \
      sort -u)
    add_command asdf list python "${version}" exit=0 <<EOF
$(for v in ${installed_versions}; do echo "  ${v}"; done)
EOF

  else
    if [ "$others_installed" = "false" ]; then
      installed_versions=""
      add_command asdf list python "${version}" exit=1
    else
      installed_versions=$(echo "${others_installed}" | tr ',' '\n' | sort -u)
      add_command asdf list python "${version}" exit=0 <<EOF
$(for v in ${installed_versions}; do echo "  ${v}"; done)
EOF
    fi
    if [ "$installed" = "fail" ]; then
      add_command asdf install python "${version}" exit=1 <<EOF
stderr:Error installing python ${version}
EOF
      add_command asdf list python <<EOF
$(for v in ${installed_versions}; do echo "  ${v}"; done)
EOF

      if [ -n "${fallback_version}" ]; then
        # Replace version by the fallback version here!
        version="${fallback_version}"
        add_command asdf list python "${version}" exit=0 <<EOF
  ${version}
EOF
      fi
    else
      add_command asdf install python "${version}"
    fi
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
  add_asdf_python_calls plugin_installed=true

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

