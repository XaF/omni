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


# bats test_tags=generate,omni:status,omni:status:self
@test "[omni_status=1] omni status shows the status of the omni setup" {
  validate_test_output omni/status.txt skip_lines=1 omni status
}

# bats test_tags=generate,omni:status,omni:status:shell
@test "[omni_status=2] omni status shows the status of the shell integration" {
  validate_test_output omni/status-shell.txt omni status --shell-integration
}

# bats test_tags=generate,omni:status,omni:status:config
@test "[omni_status=3] omni status shows the status of the configuration" {
  validate_test_output omni/status-config.txt omni status --config
}

# bats test_tags=generate,omni:status,omni:status:config-files
@test "[omni_status=4] omni status shows the status of the configuration files" {
  validate_test_output omni/status-config-files.txt omni status --config-files
}

# bats test_tags=generate,omni:status,omni:status:worktree
@test "[omni_status=5] omni status shows the default worktree" {
  validate_test_output omni/status-worktree.txt omni status --worktree
}

# bats test_tags=generate,omni:status,omni:status:orgs
@test "[omni_status=6] omni status shows the status of the organizations" {
  validate_test_output omni/status-orgs.txt omni status --orgs
}

# bats test_tags=generate,omni:status,omni:status:path
@test "[omni_status=7] omni status shows the status of the omnipath" {
  validate_test_output omni/status-path.txt omni status --path
}

# bats test_tags=generate,omni:status,omni:status:multiple
@test "[omni_status=8] omni status shows the status of multiple components" {
  validate_test_output omni/status-multiple.txt skip_lines=1 omni status --config-files --path
}
