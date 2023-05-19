#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>&-

  # Override the default columns to 100 so we have a controlled
  # environment for testing the output of the help command
  export COLUMNS=100
}


# bats test_tags=omni:help
@test "omni help shows the help message with default omni commands" {
  expected=$(echo 'omni - omnipotent tool

Usage: omni <command> [options] ARG...

General
  help            Show help for omni commands
  status          Show status of omni

Git commands
  cd              Change directory to the git directory of the specified repository
  clone           Clone the specified repository
  down, up        Sets up or tear down a repository depending on its up configuration
  organize        Organize your git repositories using the configured format
  scope           Runs an omni command in the context of the specified repository
')

  # Avoiding any shorter-than-expected wrapping
  export COLUMNS=1000
  run omni help 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help shows the help message wrapped for smaller screens" {
  expected=$(echo 'omni - omnipotent tool

Usage: omni <command> [options] ARG...

General
  help            Show help for omni commands
  status          Show status of omni

Git commands
  cd              Change directory to the git directory of
                  the specified repository
  clone           Clone the specified repository
  down, up        Sets up or tear down a repository
                  depending on its up configuration
  organize        Organize your git repositories using the
                  configured format
  scope           Runs an omni command in the context of
                  the specified repository
')

  export COLUMNS=60
  run omni help 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help help shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Show help for omni commands

If no command is given, show a list of all available commands.

Usage: omni help [command]

  command    The command to get help for

')

  run omni help help 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/help\.rb$"
}

# bats test_tags=omni:help
@test "omni help status shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Show status of omni

This will show the configuration that omni is loading when it is being called,
which includes the configuration files but also the current cached information.

Usage: omni status

')

  run omni help status 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/status\.rb$"
}

# bats test_tags=omni:help
@test "omni help cd shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Change directory to the git directory of the specified repository

If no repository is specified, change to the git directory of the main org as
specified by OMNI_ORG, if specified, or errors out if not specified.

Usage: omni cd [repo]

  repo       The name of the repo to change directory to; this can be in the
             format <org>/<repo>, or just <repo>, in which case the repo will
             be searched for in all the organizations, trying to use OMNI_ORG
             if it is set, and then trying all the other organizations
             alphabetically.

')

  run omni help cd 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/cd\.rb$"
}

# bats test_tags=omni:help
@test "omni help clone shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Clone the specified repository

Usage: omni clone <repo> [options...]

  repo         The repository to clone; this can be in format <org>/<repo>,
               just <repo>, or the full URL. If the case where only the repo
               name is specified, OMNI_ORG will be used to search for the
               repository to clone.

  options...   Any additional options to pass to git clone.

')

  run omni help clone 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/clone\.rb$"
}

# bats test_tags=omni:help
@test "omni help down shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Sets up or tear down a repository depending on its up configuration

Usage: omni down [--update-user-config] [--trust]

  --update-user-config   Whether we should handle paths found in the
                         configuration of the repository if any (yes/ask/no);
                         When using up, the path configuration will be copied
                         to the home directory of the user to be loaded on
                         every omni call. When using down, the path
                         configuration of the repository will be removed from
                         the home directory of the user if it exists (default:
                         no)

  --trust                Define how to trust the repository (always/yes/no) to
                         run the command.

')

  run omni help down 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/down\.rb$"
}

# bats test_tags=omni:help
@test "omni help organize shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Organize your git repositories using the configured format

This will offer to organize your git repositories, moving them from their
current path to the path they should be at if they had been cloned using omni
clone. This is useful if you have a bunch of repositories that you have cloned
manually, and you want to start using omni, or if you changed your mind on the
repo path format you wish to use.

Usage: omni organize [--yes]

  --yes      Do not ask for confirmation before organizing repositories

')

  run omni help organize 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/organize.rb$"
}

# bats test_tags=omni:help
@test "omni help scope shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Runs an omni command in the context of the specified repository

This allows to run any omni command that would be available while in the
repository directory, but without having to change directory to the repository
first.

Usage: omni scope <repo> <command> [options...]

  repo         The name of the repo to run commands in the context of; this can
               be in the format <org>/<repo>, or just <repo>, in which case the
               repo will be searched for in all the organizations, trying to
               use OMNI_ORG if it is set, and then trying all the other
               organizations alphabetically.

  command      The omni command to run in the context of the specified
               repository.

  options...   Any options to pass to the omni command.

')

  run omni help scope 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/scope\.rb$"
}

# bats test_tags=omni:help
@test "omni help up shows the help message for the command" {
  expected=$(echo 'omni - omnipotent tool

Sets up or tear down a repository depending on its up configuration

Usage: omni up [--update-user-config] [--trust]

  --update-user-config   Whether we should handle paths found in the
                         configuration of the repository if any (yes/ask/no);
                         When using up, the path configuration will be copied
                         to the home directory of the user to be loaded on
                         every omni call. When using down, the path
                         configuration of the repository will be removed from
                         the home directory of the user if it exists (default:
                         no)

  --trust                Define how to trust the repository (always/yes/no) to
                         run the command.

')

  run omni help up 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Split the last line of the output from the rest
  last_line=$(echo "$output" | tail -n 1)
  output=$(echo "$output" | head -n -1)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]

  # Check the last line of the output
  echo "$last_line" | grep -q "^Source: .*cmd/up.rb$"
}
