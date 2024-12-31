#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  # Setup the environment for the test; this should override $HOME too
  omni_setup 3>&-

  # Add three repositories
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/github.com/test2org/test2repo" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/bitbucket.org/test3org/test3repo" "https://bitbucket.org/test3org/test3repo.git"
}

apply_pattern() {
  local pattern="${1}"
  local host="${2}"
  local org="${3}"
  local repo="${4}"

  # Apply the pattern, which means replacing...
  # - %{host} with the host
  # - %{org} with the org
  # - %{repo} with the repo
  # And we do that with `perl` as it is available
  # on both Linux and macOS

  echo "${pattern}" | perl -pe \
    "s|%{host}|${host}|g; s|%{org}|${org}|g; s|%{repo}|${repo}|g"
}

test_cd() {
  local cd_pattern
  local dir_pattern
  local expected_path

  cd_pattern="${1}"
  dir_pattern="${2}"

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  omni cd "$(apply_pattern "${cd_pattern}" "github.com" "test1org" "test1repo")" 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  expected_path="$(apply_pattern "${dir_pattern}" "github.com" "test1org" "test1repo")"
  echo "PWD is $(pwd), supposed to be in ${HOME}/git/${expected_path}"
  [ $(pwd) = "${HOME}/git/${expected_path}" ]

  # Check that we can cd to a repo
  omni cd "$(apply_pattern "${cd_pattern}" "github.com" "test2org" "test2repo")" 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  expected_path="$(apply_pattern "${dir_pattern}" "github.com" "test2org" "test2repo")"
  echo "PWD is $(pwd), supposed to be in ${HOME}/git/${expected_path}"
  [ $(pwd) = "${HOME}/git/${expected_path}" ]

  # Check that we can cd to a repo
  omni cd "$(apply_pattern "${cd_pattern}" "bitbucket.org" "test3org" "test3repo")" 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  expected_path="$(apply_pattern "${dir_pattern}" "bitbucket.org" "test3org" "test3repo")"
  echo "PWD is $(pwd), supposed to be in ${HOME}/git/${expected_path}"
  [ $(pwd) = "${HOME}/git/${expected_path}" ]
}

test_cd_locate() {
  local cd_pattern
  local dir_pattern
  local expected_path

  cd_pattern="${1}"
  dir_pattern="${2}"

  TEST_REPO_1=("github.com" "test1org" "test1repo")
  TEST_REPO_2=("github.com" "test2org" "test2repo")
  TEST_REPO_3=("bitbucket.org" "test3org" "test3repo")

  local curtest=1
  while true; do
    local test_repo_varname="TEST_REPO_${curtest}[@]"
    local test_repo=("${!test_repo_varname}")

    if [ -z "${test_repo}" ]; then
      break
    fi

    run omni cd --locate "$(apply_pattern "${cd_pattern}" "${test_repo[@]}")" 3>&-
    echo "STATUS[$curtest]: $status"
    echo "OUTPUT[$curtest]: $output"
    [ "$status" -eq 0 ]

    # Check that we have the right output
    expected_path="$(apply_pattern "${dir_pattern}" "${test_repo[@]}")"
    echo "output[$curtest] is $output, supposed to be ${HOME}/git/${expected_path}"
    [ "$output" = "${HOME}/git/${expected_path}" ]

    curtest=$((curtest+1))
  done

  # Check that we ran the test at least once
  [ "$curtest" -gt 1 ]
}

# bats test_tags=omni:cd
@test "[omni_cd=01] omni cd finds directories, pattern=repo, location=expected, org=no, fast_search=true" {
  setup_omni_config
  test_cd "%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=02] omni cd finds directories, pattern=org/repo, location=expected, org=no, fast_search=true" {
  setup_omni_config
  test_cd "%{org}/%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=03] omni cd finds directories, pattern=host/org/repo, location=expected, org=no, fast_search=true" {
  setup_omni_config
  test_cd "%{host}/%{org}/%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd,omni:cd:locate
@test "[omni_cd=04] omni cd --locate works, pattern=repo, location=expected, org=no, fast_search=true" {
  setup_omni_config
  test_cd_locate "%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd,omni:cd:locate
@test "[omni_cd=05] omni cd --locate works, pattern=org/repo, location=expected, org=no, fast_search=true" {
  setup_omni_config
  test_cd_locate "%{org}/%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd,omni:cd:locate
@test "[omni_cd=06] omni cd --locate works, pattern=host/org/repo, location=expected, org=no, fast_search=true" {
  setup_omni_config
  test_cd_locate "%{host}/%{org}/%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=07] omni cd finds directories, pattern=repo, location=expected, org=no, fast_search=false" {
  setup_omni_config no_fast_search
  test_cd "%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=08] omni cd finds directories, pattern=repo, location=expected, org=yes, fast_search=true" {
  setup_omni_config with_org
  test_cd "%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=09] omni cd finds directories, pattern=org/repo, location=expected, org=yes, fast_search=true" {
  setup_omni_config with_org
  test_cd "%{org}/%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=10] omni cd finds directories, pattern=host/org/repo, location=expected, org=yes, fast_search=true" {
  setup_omni_config with_org
  test_cd "%{host}/%{org}/%{repo}" "%{host}/%{org}/%{repo}"
}

# bats test_tags=omni:cd
@test "[omni_cd=11] omni cd allows to switch directories using a relative path" {
  setup_omni_config

  # Make a new directory
  dir=$(uuidgen)
  mkdir -p "${dir}"

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a relative path
  omni cd "./${dir}" 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in ${HOME}/${dir}"
  [ $(pwd) = "${HOME}/${dir}" ]

  # Check that we can cd back to the previous directory
  omni cd - 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in ${HOME}"
  [ $(pwd) = "${HOME}" ]
}

# bats test_tags=omni:cd
@test "[omni_cd=12] omni cd allows to switch directories using an absolute path" {
  setup_omni_config

  # Prepare tmp dir without any trailing slash
  tmpdir=${TMPDIR:-/tmp}
  tmpdir=${tmpdir%/}

  # Make a new directory
  dir="${tmpdir}/$(uuidgen)"
  mkdir -p "${dir}"

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to an absolute path
  omni cd "${dir}" 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in ${dir}"
  [ $(pwd) = "${dir}" ]

  # Check that we can cd back to the previous directory
  omni cd - 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in ${HOME}"
  [ $(pwd) = "${HOME}" ]
}

# bats test_tags=omni:cd
@test "[omni_cd=13] omni cd allows to switch directories using ~" {
  setup_omni_config

  # Make a new directory
  dir="$(uuidgen)"
  mkdir -p "${dir}"

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a directory starting with ~
  omni cd ~/"${dir}" 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in ${HOME}/${dir}"
  [ $(pwd) = "${HOME}/${dir}" ]

  # Check that we can cd to home using ~
  omni cd ~ 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in ${HOME}"
  [ $(pwd) = "${HOME}" ]
}

# bats test_tags=omni:cd
@test "[omni_cd=14] omni cd allows to go to the repository root using ..." {
  setup_omni_config

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Change directory to a repository root
  cd "git/github.com/test1org/test1repo" 3>&-
  [ "$?" -eq 0 ]

  # Check that we are in the expected directory
  echo "PWD is $(pwd), supposed to be in ${HOME}/git/github.com/test1org/test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Create random subdirectories
  DIR="$(uuidgen)/$(uuidgen)/$(uuidgen)"
  mkdir -p "${DIR}"

  # Check that we can cd to that deeper directory
  cd "${DIR}" 3>&-
  [ "$?" -eq 0 ]

  # Check that we are in the expected directory
  echo "PWD is $(pwd), supposed to be in ${HOME}/git/github.com/test1org/test1repo/${DIR}"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo/${DIR}" ]

  # Check that we can cd back to the repository root
  # using the `...` syntax
  omni cd ... 3>&-
  [ "$?" -eq 0 ]

  # Check that we are in the expected directory
  echo "PWD is $(pwd), supposed to be in ${HOME}/git/github.com/test1org/test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]
}

