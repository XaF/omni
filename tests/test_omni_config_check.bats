#!/usr/bin/env bats

load 'helpers/utils'
load 'helpers/output'

setup() {
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Add one repository
  setup_git_dir "git/example.com/test1org/test1repo" "git@example.com:test1org/test1repo.git"

  # Change directory to the repository
  cd "git/example.com/test1org/test1repo"

  # Disable colors
  export NO_COLOR=1
}

# bats test_tags=omni:config,omni:config:check
@test "[omni_config_check=1] omni config check succeeds for valid configuration" {
  cat > .omni.yaml <<EOF
up:
  - whatever
EOF

  run omni config check --config-file .omni.yaml
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:config,omni:config:check
@test "[omni_config_check=2] omni config check fails for operations with missing details" {
  cat > .omni.yaml <<EOF
up:
  - go-install
  - cargo-install
  - github-release
EOF

  run omni config check --config-file .omni.yaml
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  [[ "${output}" == *".omni.yaml:0:C002:value for key 'up[0].go-install' is empty"* ]]
  [[ "${output}" == *".omni.yaml:0:C002:value for key 'up[1].cargo-install' is empty"* ]]
  [[ "${output}" == *".omni.yaml:0:C002:value for key 'up[2].github-release' is empty"* ]]
}

# bats test_tags=omni:config,omni:config:check
@test "[omni_config_check=3] omni config check fails for operations with null details" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
  - cargo-install:
  - github-release:
EOF

  run omni config check --config-file .omni.yaml
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  [[ "${output}" == *".omni.yaml:0:C101:value for key 'up[0].go-install' should be any type of [\"string\", \"array\", \"table\"] but found Null"* ]]
  [[ "${output}" == *".omni.yaml:0:C101:value for key 'up[1].cargo-install' should be any type of [\"string\", \"array\", \"table\"] but found Null"* ]]
  [[ "${output}" == *".omni.yaml:0:C101:value for key 'up[2].github-release' should be any type of [\"string\", \"array\", \"table\"] but found Null"* ]]
}

# bats test_tags=generate,omni:config,omni:config:check
@test "[omni_config_check=4] omni config check fails for many issues" {
  validate_test_output omni/config-check-many-issues.txt exit_code=1 omni config check --config-file "${FIXTURES_DIR}/omni/config-check-broken-input.txt"
}

# bats test_tags=generate,omni:config,omni:config:check,omni:config:check:json
@test "[omni_config_check=5] omni config check fails for many issues (json)" {
  validate_test_output omni/config-check-many-issues-json.txt exit_code=1 omni config check --output json --config-file "${FIXTURES_DIR}/omni/config-check-broken-input.txt"
}
