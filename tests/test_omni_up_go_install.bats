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

add_brew_golang_calls() {
  add_asdf_tool_brew_calls golang
}

add_asdf_golang_calls() {
  add_asdf_tool_calls tool="golang" "$@"
}

go_install_success() {
  cat <<EOF
#omni-test-bash-function
function process_response() {
  local gobin="\${GOBIN:-\$(go env GOPATH)/bin}"
  mkdir -p "\$gobin"
  touch "\$gobin/mytool"
  chmod +x "\$gobin/mytool"
  echo "fake bin installed at \$gobin/mytool"
}
EOF
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation installs a new tool" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - github.com/test/tool
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go list -m -versions -json github.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v github.com/test/tool@v2.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation uses specified version" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: github.com/test/tool
      version: v1.1.0
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go list -m -versions -json github.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v github.com/test/tool@v1.1.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# # bats test_tags=omni:up,omni:up:go-install
# @test "omni up go-install operation upgrades when specified" {
  # cat > .omni.yaml <<EOF
# up:
  # - go-install:
      # tools:
      # - path: github.com/test/tool
        # upgrade: true
# EOF

  # add_command go list -m -versions -json github.com/test/tool output='{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  # add_command go install -v github.com/test/tool@v2.0.0
  # add_command go --prefix

  # run omni up --trust 3>&-
  # [ "$status" -eq 0 ]
# }

# # bats test_tags=omni:up,omni:up:go-install
# @test "omni up go-install operation upgrades with command line flag" {
  # cat > .omni.yaml <<EOF
# up:
  # - go-install:
      # tools:
      # - github.com/test/tool
# EOF

  # add_command go list -m -versions -json github.com/test/tool output='{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  # add_command go install -v github.com/test/tool@v2.0.0
  # add_command go --prefix

  # run omni up --trust --upgrade 3>&-
  # [ "$status" -eq 0 ]
# }

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation handles prerelease versions when specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: github.com/test/tool
      prerelease: true
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go list -m -versions -json github.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0-beta"]}'
  add_command go install -v github.com/test/tool@v2.0.0-beta <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation handles build versions when specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: github.com/test/tool
      build: true
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go list -m -versions -json github.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0+build"]}'
  add_command go install -v github.com/test/tool@v2.0.0+build <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation uses exact version when specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: github.com/test/tool
      version: v1.0.0
      exact: true
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go install -v github.com/test/tool@v1.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation uses exact version using pseudo-version" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: github.com/test/tool
      version: v0.0.0-20210101000000-abcdef123456
      exact: true
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go install -v github.com/test/tool@v0.0.0-20210101000000-abcdef123456 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation handles multiple tools" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - github.com/test/tool1
    - github.com/test/tool2
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go list -m -versions -json github.com/test/tool1 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v github.com/test/tool1@v2.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json github.com/test/tool2 \
    <<< '{"Version":"v0.0.0","Versions":["v3.0.0","v4.4.0","v5.0.0"]}'
  add_command go install -v github.com/test/tool2@v5.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation handles mixed ways to declare tools" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - github.com/test/tool1@v2.0.0
    - github.com/test/tool2: v5.0.0
    - github.com/test/tool3:
        version: v4.0.0
    - github.com/test/tool4:
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  add_command go list -m -versions -json github.com/test/tool1 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v github.com/test/tool1@v2.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json github.com/test/tool2 \
    <<< '{"Version":"v0.0.0","Versions":["v3.0.0","v4.4.0","v5.0.0"]}'
  add_command go install -v github.com/test/tool2@v5.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json github.com/test/tool3 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v4.0.0","v7.0.0"]}'
  add_command go install -v github.com/test/tool3@v4.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json github.com/test/tool4 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v7.0.0","v42.0.0"]}'
  add_command go install -v github.com/test/tool4@v42.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation fails on invalid import path" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - "@invalid/path"
EOF

  go_version=$(asdf_tool_latest_version golang)
  add_fakebin "$(asdf_tool_path golang "$go_version")/bin/go"
  add_brew_golang_calls
  add_asdf_golang_calls version="$go_version"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "omni up go-install operation fails when no tools specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}
