#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup
}


# bats test_tags=omni:cd
@test "omni cd finds directories in the expected locations (host/org/repo)" {
  # Configure OMNIORG for faster lookup
  export OMNI_ORG="git@github.com:test1org,https://github.com/test2org,https://bitbucket.org/test3org"

  # Add two repositories
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/github.com/test2org/test2repo" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/bitbucket.org/test3org/test3repo" "https://bitbucket.org/test3org/test3repo.git"

  # Configure the repo_path_format
  echo "repo_path_format: \"%{host}/%{org}/%{repo}\"" > "${HOME}/.omni.yaml"

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  omni cd test1repo
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Check that we can cd to a repo
  omni cd test2repo
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test2repo"
  [ $(pwd) = "${HOME}/git/github.com/test2org/test2repo" ]

  # Check that we can cd to a repo
  omni cd test3repo
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test3repo"
  [ $(pwd) = "${HOME}/git/bitbucket.org/test3org/test3repo" ]
}


# bats test_tags=omni:cd
@test "omni cd finds directories in the expected locations (host/org/repo), OMNI_ORG configured" {
  # Configure OMNIORG for faster lookup
  export OMNI_ORG="git@github.com:test1org,https://github.com/test2org,https://bitbucket.org/test3org"

  # Add two repositories
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/github.com/test2org/test2repo" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/bitbucket.org/test3org/test3repo" "https://bitbucket.org/test3org/test3repo.git"

  # Configure the repo_path_format
  echo "repo_path_format: \"%{host}/%{org}/%{repo}\"" > "${HOME}/.omni.yaml"

  # Check that the current directory is home
  [ $(pwd) = "${HOME}" ]

  # Check that we can cd to a repo
  omni cd test1repo
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test1repo"
  [ $(pwd) = "${HOME}/git/github.com/test1org/test1repo" ]

  # Check that we can cd to a repo
  omni cd test2repo
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test2repo"
  [ $(pwd) = "${HOME}/git/github.com/test2org/test2repo" ]

  # Check that we can cd to a repo
  omni cd test3repo
  [ "$?" -eq 0 ]

  # Check that we've cd'd to the correct directory
  echo "PWD is $(pwd), supposed to be in test3repo"
  [ $(pwd) = "${HOME}/git/bitbucket.org/test3org/test3repo" ]
}

