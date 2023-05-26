#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>&-

  # Configure OMNIORG for faster lookup
  export OMNI_ORG="git@github.com:test1org,https://github.com/test2org,https://bitbucket.org/test3org"

  # Add two repositories
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/github.com/test2org/test2repo" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/bitbucket.org/test3org/test3repo" "https://bitbucket.org/test3org/test3repo.git"

  # Configure the repo_path_format
  echo "repo_path_format: \"%{host}/%{org}/%{repo}\"" > "${HOME}/.omni.yaml"
}


# bats test_tags=omni:cd
@test "omni cd finds directories in the expected locations (host/org/repo), without OMNI_ORG" {
  unset OMNI_ORG

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  echo "PATH = $PATH"
  omni cd test1repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Check that we can cd to a repo
  omni cd test2repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test2repo"
  [ $(pwd) = "${HOME}/git/github.com/test2org/test2repo" ]

  # Check that we can cd to a repo
  omni cd test3repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test3repo"
  [ $(pwd) = "${HOME}/git/bitbucket.org/test3org/test3repo" ]
}

# bats test_tags=omni:cd
@test "omni cd finds directories in the expected locations (host/org/repo), OMNI_ORG configured" {
  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  omni cd test1repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Check that we can cd to a repo
  omni cd test2repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test2repo"
  [ $(pwd) = "${HOME}/git/github.com/test2org/test2repo" ]

  # Check that we can cd to a repo
  omni cd test3repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test3repo"
  [ $(pwd) = "${HOME}/git/bitbucket.org/test3org/test3repo" ]
}

# bats test_tags=omni:cd
@test "omni cd finds directories using org/repo" {
  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  omni cd test1org/test1repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Check that we can cd to a repo
  omni cd test2org/test2repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test2repo"
  [ $(pwd) = "${HOME}/git/github.com/test2org/test2repo" ]

  # Check that we can cd to a repo
  omni cd test3org/test3repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test3repo"
  [ $(pwd) = "${HOME}/git/bitbucket.org/test3org/test3repo" ]
}

# bats test_tags=omni:cd
@test "omni cd finds directories using host/org/repo" {
  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  omni cd github.com/test1org/test1repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Check that we can cd to a repo
  omni cd github.com/test2org/test2repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test2repo"
  [ $(pwd) = "${HOME}/git/github.com/test2org/test2repo" ]

  # Check that we can cd to a repo
  omni cd bitbucket.org/test3org/test3repo 3>&-
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test3repo"
  [ $(pwd) = "${HOME}/git/bitbucket.org/test3org/test3repo" ]
}

# bats test_tags=omni:cd
@test "omni cd allows to switch directories using a relative path" {
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
@test "omni cd allows to switch directories using an absolute path" {
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
@test "omni cd allows to switch directories using ~" {
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

