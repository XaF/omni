#!/usr/bin/env bats

load 'helpers/utils'
load 'helpers/output'

setup() {
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Depending on the 'cat' command, check if '-A' is supported
  if cat -A </dev/null 2>/dev/null; then
    export CAT_OPTS='A'
  else
    export CAT_OPTS='e'
  fi

  # Override the default columns to 100 so we have a controlled
  # environment for testing the output of the help command
  export COLUMNS=100

  # Disable colors
  export NO_COLOR=1
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with default omni commands" {
  # Avoiding any shorter-than-expected wrapping
  export COLUMNS=1000

  validate_test_output omni/help.txt skip_lines=1 omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with all omni commands when using --unfold" {
  # Avoiding any shorter-than-expected wrapping
  export COLUMNS=1000

  validate_test_output omni/help-unfold.txt skip_lines=1 omni help --unfold
}

# bats test_tags=generate,omni:help
@test "omni help shows the help message wrapped for smaller screens" {
  # Set the columns to 60 to force wrapping
  export COLUMNS=60

  validate_test_output omni/help-wrapped-60.txt skip_lines=1 omni help
}

# bats test_tags=generate,omni:help
@test "omni help config shows the help message for the command" {
  validate_test_output omni/help-config.txt skip_lines=1 omni help config
}

# bats test_tags=generate,omni:help
@test "omni help config bootstrap shows the help message for the command" {
  validate_test_output omni/help-config-bootstrap.txt skip_lines=1 omni help config bootstrap
}

# bats test_tags=generate,omni:help
@test "omni help config path shows the help message for the command" {
  validate_test_output omni/help-config-path.txt skip_lines=1 omni help config path
}

# bats test_tags=generate,omni:help
@test "omni help config path switch shows the help message for the command" {
  validate_test_output omni/help-config-path-switch.txt skip_lines=1 omni help config path switch
}

# bats test_tags=generate,omni:help
@test "omni help config reshim shows the help message for the command" {
  validate_test_output omni/help-config-reshim.txt skip_lines=1 omni help config reshim
}

# bats test_tags=generate,omni:help
@test "omni help config trust shows the help message for the command" {
  validate_test_output omni/help-config-trust.txt skip_lines=1 omni help config trust
}

# bats test_tags=generate,omni:help
@test "omni help config untrust shows the help message for the command" {
  validate_test_output omni/help-config-untrust.txt skip_lines=1 omni help config untrust
}

# bats test_tags=generate,omni:help
@test "omni help help shows the help message for the command" {
  validate_test_output omni/help-help.txt skip_lines=1 omni help help
}

# bats test_tags=generate,omni:help
@test "omni help hook shows the help message for the command" {
  validate_test_output omni/help-hook.txt skip_lines=1 omni help hook
}

# bats test_tags=generate,omni:help
@test "omni help hook env shows the help message for the command" {
  validate_test_output omni/help-hook-env.txt skip_lines=1 omni help hook env
}

# bats test_tags=generate,omni:help
@test "omni help hook init shows the help message for the command" {
  validate_test_output omni/help-hook-init.txt skip_lines=1 omni help hook init
}

# bats test_tags=generate,omni:help
@test "omni help hook uuid shows the help message for the command" {
  validate_test_output omni/help-hook-uuid.txt skip_lines=1 omni help hook uuid
}

# bats test_tags=generate,omni:help
@test "omni help status shows the help message for the command" {
  validate_test_output omni/help-status.txt skip_lines=1 omni help status
}

# bats test_tags=generate,omni:help
@test "omni help cd shows the help message for the command" {
  validate_test_output omni/help-cd.txt skip_lines=1 omni help cd
}

# bats test_tags=generate,omni:help
@test "omni help clone shows the help message for the command" {
  validate_test_output omni/help-clone.txt skip_lines=1 omni help clone
}

# bats test_tags=generate,omni:help
@test "omni help down shows the help message for the command" {
  validate_test_output omni/help-down.txt skip_lines=1 omni help down
}

# bats test_tags=generate,omni:help
@test "omni help scope shows the help message for the command" {
  validate_test_output omni/help-scope.txt skip_lines=1 omni help scope
}

# bats test_tags=generate,omni:help
@test "omni help tidy shows the help message for the command" {
  validate_test_output omni/help-tidy.txt skip_lines=1 omni help tidy
}

# bats test_tags=generate,omni:help,omni:help:up
@test "omni help up shows the help message for the command" {
  validate_test_output omni/help-up.txt skip_lines=1 omni help up
}

setup_very_long_config_command() {
  local omni_config="${HOME}/.config/omni/config.yaml"
  mkdir -p "$(dirname "$omni_config")"
  cat <<EOF >>"$omni_config"
commands:
  supercalifragilisticexpialidocious:
    aliases:
      - abracadabra
      - hocuspocus
      - open-sesame
    desc: |
      lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do
      eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut
      enim ad minim veniam, quis nostrud exercitation ullamco laboris
      nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor
      in reprehenderit in voluptate velit esse cillum dolore eu fugiat
      nulla pariatur. Excepteur sint occaecat cupidatat non proident,
      sunt in culpa qui officia deserunt mollit anim id est laborum.
    run: |
      echo "Hello, world!"
EOF
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with a very long config command (columns=1000)" {
  setup_very_long_config_command
  export COLUMNS=1000
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt skip_lines=1 omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with a very long config command (columns=100)" {
  setup_very_long_config_command
  export COLUMNS=100
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt skip_lines=1 omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with a very long config command (columns=50)" {
  setup_very_long_config_command
  export COLUMNS=50
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt skip_lines=1 omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help fails to show the help message if terminal width is too low (columns=10)" {
  setup_very_long_config_command
  export COLUMNS=10
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt skip_lines=1 exit_code=1 omni help
}

# bats test_tags=generate,omni:help
@test "omni help shows command parameters in the help message of a custom command" {
  local omni_config="${HOME}/.config/omni/config.yaml"
  mkdir -p "$(dirname "$omni_config")"
  cat <<EOF >>"$omni_config"
commands:
  custom-command:
    syntax:
      parameters:
        - name: "-a"
          desc: parameter a
          required: true
        - name: --beta
          desc: parameter b
          required: true
        - name: -c
          aliases: --charlie
          placeholders: MYPLACEHOLDER
          desc: parameter c
          required: true
    desc: |
      Custom command.
    run: |
      echo "Hello, world!"
EOF

  validate_test_output omni/help-custom-command.txt skip_lines=1 omni help custom command
}
