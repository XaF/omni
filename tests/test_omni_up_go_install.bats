#!/usr/bin/env bats

load 'helpers/utils'
load 'helpers/mise'

setup() {
  # Setup the environment for the test; this should override $HOME too
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Add one repository
  setup_git_dir "git/example.com/test1org/test1repo" "git@example.com:test1org/test1repo.git"

  # Change directory to the repository
  cd "git/example.com/test1org/test1repo"
}

teardown() {
  check_commands
}

add_brew_golang_calls() {
  add_mise_tool_brew_calls go
}

add_mise_golang_calls() {
  add_mise_tool_calls tool="go" "$@"
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
@test "[omni_up_go_install=01] omni up go-install operation installs a new tool" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go list -m -versions -json example.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v example.com/test/tool@v2.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=02] omni up go-install operation fallbacks to parents to list versions" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/my/super/test/tool
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go list -m -versions -json example.com/my/super/test/tool exit=1
  add_command go list -m -versions -json example.com/my/super/test exit=1
  add_command go list -m -versions -json example.com/my/super <<< '{"Version":"v0.0.0"}'
  add_command go list -m -versions -json example.com/my \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v example.com/my/super/test/tool@v2.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=03] omni up go-install operation uses specified version" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: example.com/test/tool
      version: v1.1.0
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go list -m -versions -json example.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v example.com/test/tool@v1.1.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=04] omni up go-install operation handles prerelease versions when specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: example.com/test/tool
      prerelease: true
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go list -m -versions -json example.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0-beta"]}'
  add_command go install -v example.com/test/tool@v2.0.0-beta <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=05] omni up go-install operation handles build versions when specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: example.com/test/tool
      build: true
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go list -m -versions -json example.com/test/tool \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0+build"]}'
  add_command go install -v example.com/test/tool@v2.0.0+build <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=06] omni up go-install operation uses exact version when specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: example.com/test/tool
      version: v1.0.0
      exact: true
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go install -v example.com/test/tool@v1.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=07] omni up go-install operation uses exact version using pseudo-version" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - path: example.com/test/tool
      version: v0.0.0-20210101000000-abcdef123456
      exact: true
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go install -v example.com/test/tool@v0.0.0-20210101000000-abcdef123456 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=08] omni up go-install operation handles multiple tools" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool1
    - example.com/test/tool2
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go list -m -versions -json example.com/test/tool1 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v1.1.0","v2.0.0"]}'
  add_command go install -v example.com/test/tool1@v2.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json example.com/test/tool2 \
    <<< '{"Version":"v0.0.0","Versions":["v3.0.0","v4.4.0","v5.0.0"]}'
  add_command go install -v example.com/test/tool2@v5.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=09] omni up go-install operation handles mixed ways to declare tools" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool1@v2.0.0
    - example.com/test/tool2: v5.0.0
    - example.com/test/tool3:
        version: 4
    - example.com/test/tool4:
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  add_command go install -v example.com/test/tool1@v2.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json example.com/test/tool2 \
    <<< '{"Version":"v0.0.0","Versions":["v3.0.0","v4.4.0","v5.0.0"]}'
  add_command go install -v example.com/test/tool2@v5.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json example.com/test/tool3 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v4.0.0","v7.0.0"]}'
  add_command go install -v example.com/test/tool3@v4.0.0 <<< "$(go_install_success)"

  add_command go list -m -versions -json example.com/test/tool4 \
    <<< '{"Version":"v0.0.0","Versions":["v1.0.0","v7.0.0","v42.0.0"]}'
  add_command go install -v example.com/test/tool4@v42.0.0 <<< "$(go_install_success)"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=10] omni up go-install operation fails on invalid import path" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
    - "@invalid/path"
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}

# bats test_tags=omni:up,omni:up:go-install
@test "[omni_up_go_install=11] omni up go-install operation fails when no tools specified" {
  cat > .omni.yaml <<EOF
up:
  - go-install:
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}

# bats test_tags=omni:up,omni:up:go-install,supply-chain
@test "[omni_up_go_install=12] omni up go-install operation fails when go-install is disabled through the supply chain" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    allowed:
      - '!go-install'
EOF

  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"go-install operation is not allowed"* ]]
}

# bats test_tags=omni:up,omni:up:go-install,supply-chain
@test "[omni_up_go_install=13] omni up go-install operation fails when go-install is not allow-listed but others are through the supply chain" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    allowed:
      - 'blah'
EOF

  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"go-install operation is not allowed"* ]]
}

# bats test_tags=omni:up,omni:up:go-install,supply-chain
@test "[omni_up_go_install=14] omni up go-install operation fails when a specific url is disabled through the supply chain (global)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    sources:
      - '!https://example.com/test/tool'
EOF

  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"go-install source not allowed: example.com/test/tool"* ]]
}

# bats test_tags=omni:up,omni:up:go-install,supply-chain
@test "[omni_up_go_install=15] omni up go-install operation fails when a specific url is disabled through the supply chain (global, wildcard)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    sources:
      - '!https://example.com/test/*'
EOF

  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"go-install source not allowed: example.com/test/tool"* ]]
}

# bats test_tags=omni:up,omni:up:go-install,supply-chain
@test "[omni_up_go_install=16] omni up go-install operation fails when a specific url is disabled through the supply chain (mise)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    go-install:
      sources:
        - '!https://example.com/test/tool'
EOF

  cat > .omni.yaml <<EOF
up:
  - go-install:
    - example.com/test/tool
EOF

  go_version=$(mise_tool_latest_version go)
  add_fakebin "$(mise_tool_path go "$go_version")/bin/go"
  add_brew_golang_calls
  add_mise_golang_calls version="$go_version"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"go-install source not allowed: example.com/test/tool"* ]]
}

