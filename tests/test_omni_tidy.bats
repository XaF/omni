#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>&-
}

# bats test_tags=omni:tidy
@test "[omni_tidy=1] omni tidy moves repositories in their expected locations (host/org/repo)" {
  # Add two repositories
  setup_git_dir "git/test1" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/test2" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/test3" "https://bitbucket.org/test3org/test3repo.git"

  # Configure omni
  setup_omni_config 'repo_path_format=%{host}/%{org}/%{repo}'

  run omni tidy --yes 3>&-
  [ "$status" -eq 0 ]

  # Check that the repositories were cloned
  [ -d "git/github.com/test1org/test1repo" ]
  [ -d "git/github.com/test2org/test2repo" ]
  [ -d "git/bitbucket.org/test3org/test3repo" ]

  # Check that the repositories were moved
  [ ! -d "git/test1" ]
  [ ! -d "git/test2" ]
  [ ! -d "git/test3" ]
}

# bats test_tags=omni:tidy
@test "[omni_tidy=2] omni tidy moves repositories in their expected locations (org/repo)" {
  # Add two repositories
  setup_git_dir "git/test1" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/test2" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/test3" "https://bitbucket.org/test3org/test3repo.git"

  # Configure omni
  setup_omni_config 'repo_path_format=%{org}/%{repo}'

  run omni tidy --yes 3>&-
  [ "$status" -eq 0 ]

  # Check that the repositories were cloned
  [ -d "git/test1org/test1repo" ]
  [ -d "git/test2org/test2repo" ]
  [ -d "git/test3org/test3repo" ]

  # Check that the repositories were moved
  [ ! -d "git/test1" ]
  [ ! -d "git/test2" ]
  [ ! -d "git/test3" ]
}

# bats test_tags=omni:tidy
@test "[omni_tidy=3] omni tidy moves repositories in their expected locations (repo)" {
  # Add two repositories
  setup_git_dir "git/test1" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/test2" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/test3" "https://bitbucket.org/test3org/test3repo.git"

  # Configure omni
  setup_omni_config 'repo_path_format=%{repo}'

  run omni tidy --yes 3>&-
  [ "$status" -eq 0 ]

  # Check that the repositories were cloned
  [ -d "git/test1repo" ]
  [ -d "git/test2repo" ]
  [ -d "git/test3repo" ]

  # Check that the repositories were moved
  [ ! -d "git/test1" ]
  [ ! -d "git/test2" ]
  [ ! -d "git/test3" ]
}

# bats test_tags=omni:tidy
@test "[omni_tidy=4] omni tidy moves repositories in their expected locations (one/host/two/org/three/repo/four)" {
  # Add two repositories
  setup_git_dir "git/test1" "git@github.com:test1org/test1repo.git"
  setup_git_dir "git/test2" "https://github.com/test2org/test2repo.git"
  setup_git_dir "git/test3" "https://bitbucket.org/test3org/test3repo.git"

  # Configure omni
  setup_omni_config 'repo_path_format=one/%{host}/two/%{org}/three/%{repo}/four'

  run omni tidy --yes 3>&-
  [ "$status" -eq 0 ]

  # Check that the repositories were cloned
  [ -d "git/one/github.com/two/test1org/three/test1repo/four" ]
  [ -d "git/one/github.com/two/test2org/three/test2repo/four" ]
  [ -d "git/one/bitbucket.org/two/test3org/three/test3repo/four" ]

  # Check that the repositories were moved
  [ ! -d "git/test1" ]
  [ ! -d "git/test2" ]
  [ ! -d "git/test3" ]
}
