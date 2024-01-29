#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Override the default columns to 100 so we have a controlled
  # environment for testing the output of the help command
  export COLUMNS=100

  echo "STATUUS" >&2
  omni status >&2
}


# bats test_tags=omni:help
@test "omni help shows the help message with default omni commands" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Usage: omni <command> [options] ARG...

General
  config ▶       Provides config commands
  help           Show help for omni commands
  hook ▶         Call one of omni's hooks for the shell
  status         Show the status of omni

Git commands
  cd             Change directory to the git directory of the specified repository
  clone          Clone the specified repository
  up, down       Sets up or tear down a repository depending on its up configuration
  scope          Runs an omni command in the context of the specified repository
  tidy           Organize your git repositories using the configured format
EOF
)

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
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Usage: omni <command> [options] ARG...

General
  config ▶       Provides config commands
  help           Show help for omni commands
  hook ▶         Call one of omni's hooks for the
                 shell
  status         Show the status of omni

Git commands
  cd             Change directory to the git directory
                 of the specified repository
  clone          Clone the specified repository
  up, down       Sets up or tear down a repository
                 depending on its up configuration
  scope          Runs an omni command in the context
                 of the specified repository
  tidy           Organize your git repositories using
                 the configured format
EOF
)

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
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Show help for omni commands

If no command is given, show a list of all available commands.

Usage: omni help [unfold] [command]

  unfold         Show all subcommands

  command        The command to get help for

Source: builtin
EOF
)

  run omni help help 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help status shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Show the status of omni

This will show the configuration that omni is loading when called from the current
directory.

Usage: omni status [--shell-integration] [--config] [--config-files] [--worktree] [--orgs] [--path]

  --shell-integration  Show if the shell integration is loaded or not.

  --config             Show the configuration that omni is using for the current directory.
                       This is not shown by default.

  --config-files       Show the configuration files that omni is loading for the current
                       directory.

  --worktree           Show the default worktree.

  --orgs               Show the organizations.

  --path               Show the current omnipath.

Source: builtin
EOF
)

  run omni help status 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help cd shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Change directory to the git directory of the specified repository

If no repository is specified, change to the git directory of the main org as specified by
OMNI_ORG, if specified, or errors out if not specified.

Usage: omni cd [--locate] [--[no-]include-packages] [repo]

  --locate                 If provided, will only return the path to the repository instead of
                           switching directory to it. When this flag is passed, interactions
                           are also disabled, as it is assumed to be used for command line
                           purposes. This will exit with 0 if the repository is found, 1
                           otherwise.

  --[no-]include-packages  If provided, will include (or not include) packages when running
                           the command; this defaults to including packages when using
                           --locate, and not including packages otherwise.

  repo                     The name of the repo to change directory to; this can be in the
                           format <org>/<repo>, or just <repo>, in which case the repo will be
                           searched for in all the organizations, trying to use OMNI_ORG if it
                           is set, and then trying all the other organizations alphabetically.

Source: builtin
EOF
)

  run omni help cd 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help clone shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Clone the specified repository

The clone operation will be handled using the first organization that matches the argument
and for which the repository exists. The repository will be cloned in a path that matches
omni's expectations, depending on your configuration.

Usage: omni clone [--package] <repo> [options...]

  --package      Clone the repository as a package (default: no)

  repo           The repository to clone; this can be in format <org>/<repo>, just <repo>, or
                 the full URL. If the case where only the repo name is specified, OMNI_ORG
                 will be used to search for the repository to clone.

  options...     Any additional options to pass to git clone.

Source: builtin
EOF
)

  run omni help clone 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help down shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Sets up or tear down a repository depending on its up configuration

Usage: omni down [--no-cache] [--bootstrap] [--clone-suggested] [--trust] [--update-repository] [--update-user-config]

  --no-cache            Whether we should disable the cache while running the command
                        (default: no)

  --bootstrap           Same as using --update-user-config --clone-suggested; if any of the
                        options are directly provided, they will take precedence over the
                        default values of the options

  --clone-suggested     Whether we should clone suggested repositories found in the
                        configuration of the repository if any (yes/ask/no) (default: no)

  --trust               Define how to trust the repository (always/yes/no) to run the command

  --update-repository   Whether we should update the repository before running the command; if
                        the repository is already up to date, the rest of the process will be
                        skipped (default: no)

  --update-user-config  Whether we should handle suggestions found in the configuration of the
                        repository if any (yes/ask/no); When using up, the suggest_config
                        configuration will be copied to the home directory of the user to be
                        loaded on every omni call (default: no)

Source: builtin
EOF
)

  run omni help down 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help scope shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Runs an omni command in the context of the specified repository

This allows to run any omni command that would be available while in the repository
directory, but without having to change directory to the repository first.

Usage: omni scope <repo> <command> [options...]

  repo           The name of the repo to run commands in the context of; this can be in the
                 format <org>/<repo>, or just <repo>, in which case the repo will be searched
                 for in all the organizations, trying to use OMNI_ORG if it is set, and then
                 trying all the other organizations alphabetically.

  command        The omni command to run in the context of the specified repository.

  options...     Any options to pass to the omni command.

Source: builtin
EOF
)

  run omni help scope 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help tidy shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Organize your git repositories using the configured format

This will offer to organize your git repositories, moving them from their current path to
the path they should be at if they had been cloned using omni clone. This is useful if you
have a bunch of repositories that you have cloned manually, and you want to start using
omni, or if you changed your mind on the repo path format you wish to use.

Usage: omni tidy [--yes] [--search-path] [--up-all]

  --yes          Do not ask for confirmation before organizing repositories

  --search-path  Extra path to search git repositories to tidy up (repeat as many times as you
                 need)

  --up-all       Run omni up in all the repositories with an omni configuration; any argument
                 passed to the tidy command after -- will be passed to omni up (e.g. omni tidy
                 --up-all -- --update-repository)

Source: builtin
EOF
)

  run omni help tidy 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=omni:help
@test "omni help up shows the help message for the command" {
  version=$(omni --version | cut -d' ' -f3)

  expected=$(cat <<EOF
omni - omnipotent tool (v$version)

Sets up or tear down a repository depending on its up configuration

Usage: omni up [--no-cache] [--bootstrap] [--clone-suggested] [--trust] [--update-repository] [--update-user-config]

  --no-cache            Whether we should disable the cache while running the command
                        (default: no)

  --bootstrap           Same as using --update-user-config --clone-suggested; if any of the
                        options are directly provided, they will take precedence over the
                        default values of the options

  --clone-suggested     Whether we should clone suggested repositories found in the
                        configuration of the repository if any (yes/ask/no) (default: no)

  --trust               Define how to trust the repository (always/yes/no) to run the command

  --update-repository   Whether we should update the repository before running the command; if
                        the repository is already up to date, the rest of the process will be
                        skipped (default: no)

  --update-user-config  Whether we should handle suggestions found in the configuration of the
                        repository if any (yes/ask/no); When using up, the suggest_config
                        configuration will be copied to the home directory of the user to be
                        loaded on every omni call (default: no)

Source: builtin
EOF
)

  run omni help up 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat -A 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}
