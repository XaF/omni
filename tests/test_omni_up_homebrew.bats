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

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=01] omni up homebrew operation calls to install formula" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - fakeformula
EOF

  add_command brew list --formula fakeformula exit=1
  add_command brew install --formula fakeformula
  add_command brew --prefix --installed fakeformula
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=02] omni up homebrew operation calls to upgrade formula when upgrade is passed on the command line" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - fakeformula
EOF

  add_command brew list --formula fakeformula
  add_command brew upgrade --formula fakeformula
  add_command brew --prefix --installed fakeformula
  add_command brew --prefix

  run omni up --trust --upgrade 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=03] omni up homebrew operation calls to upgrade formula when upgrade is configured at the work directory level" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - fakeformula

up_command:
  upgrade: true
EOF

  add_command brew list --formula fakeformula
  add_command brew upgrade --formula fakeformula
  add_command brew --prefix --installed fakeformula
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=04] omni up homebrew operation calls to upgrade formula when upgrade is configured for the formula" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - formula: fakeformula
        upgrade: true
EOF

  add_command brew list --formula fakeformula
  add_command brew upgrade --formula fakeformula
  add_command brew --prefix --installed fakeformula
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=05] omni up homebrew operation does not upgrade formula if it is already installed and upgrade is not configured" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - formula: fakeformula
EOF

  add_command brew list --formula fakeformula
  add_command brew --prefix --installed fakeformula
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=06] omni up homebrew operation calls to install cask" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - cask: fakecask
EOF

  add_command brew list --cask fakecask exit=1
  add_command brew install --cask fakecask
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=07] omni up homebrew operation calls to upgrade cask when upgrade is passed on the command line" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - cask: fakecask
EOF

  add_command brew list --cask fakecask
  add_command brew upgrade --cask fakecask
  add_command brew --prefix

  run omni up --trust --upgrade 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=08] omni up homebrew operation calls to upgrade cask when upgrade is configured at the work directory level" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - cask: fakecask

up_command:
  upgrade: true
EOF

  add_command brew list --cask fakecask
  add_command brew upgrade --cask fakecask
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=09] omni up homebrew operation calls to upgrade cask when upgrade is configured for the cask" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - cask: fakecask
        upgrade: true
EOF

  add_command brew list --cask fakecask
  add_command brew upgrade --cask fakecask
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=10] omni up homebrew operation does not upgrade cask if it is already installed and upgrade is not configured" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - cask: fakecask
EOF

  add_command brew list --cask fakecask
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:install
@test "[omni_up_homebrew=11] omni up homebrew operation calls to install formula with full tap path" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      install:
      - fakerepo/fake/fakeformula
EOF

  add_command brew tap
  add_command brew tap fakerepo/fake
  add_command brew list --formula fakerepo/fake/fakeformula exit=1
  add_command brew install --formula fakerepo/fake/fakeformula
  add_command brew --prefix --installed fakerepo/fake/fakeformula
  add_command brew --prefix

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:homebrew,omni:up:homebrew:tap
@test "[omni_up_homebrew=12] omni up homebrew operation calls to tap repository" {
  cat > .omni.yaml <<EOF
up:
  - homebrew:
      tap:
      - fakerepo/fake
EOF

  add_command brew tap
  add_command brew tap fakerepo/fake

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}
