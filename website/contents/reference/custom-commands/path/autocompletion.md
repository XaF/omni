---
sidebar_position: 2
description: Handling autocompletion for custom commands from path
---

# Autocompletion

Omni supports autocompletion of its commands, for instance to autocomplete available omni commands but also to autocomplete parameters of [builtin commands](/reference/builtin-commands).

To improve path commands integration to omni, when the [`autocompletion`](metadata#autocompletion) metadata is set to `true`, omni will transfer any autocompletion to the command that will be executing the call. This is done by calling the command with its first argument set to `--complete`, followed by all the arguments passed so far to the omni command. Omni also sets the `COMP_CWORD` environment variable to the current expected autocompletion position.

It is expected from the command to print, on the standard output, all the potential values for autocompletion. The user's shell will then take it over and offer autocompletion to the user.

## Examples

### Basic completion

Let's consider a command `say hello`, provided by the bash script at `/path/to/say/hello.sh`, that takes `--help`, `-h` or `--say-goodbye` as parameters.

We could define the following in our bash script:
```bash title="/path/to/say/hello.sh" showLineNumbers
#!/usr/bin/env bash

if [[ "$1" == "--complete" ]]; then
        echo "--help"
        echo "--say-goodbye"
        echo "-h"
        exit 0
fi

# Rest of the code
```

#### Autocompletion without prefix

If the user were to type:
```bash
omni say hello <tab><tab>
```

This would trigger the following call to the `say hello` command:
```bash
COMP_CWORD=0 /path/to/say/hello.sh --complete
```

Which would return all the possible values to the user for autocompletion.

#### Autocompletion with prefix

If we had started prefixing the parameter:
```bash
omni say hello --<tab><tab>
```

This would trigger the following call:
```bash
COMP_CWORD=0 /path/to/say/hello.sh --complete --
```

And while our script would still return the same list of values, the shell will identify that only `--help` and `--say-goodbye` are sharing the same prefix and the command that was started, and only these two would be offered to the user for autocompletion.

### Complex completion

Let's consider a `server` command, provided by the bash script at `/path/to/server.sh`, command that allows to show metadata of a server when the `server` option is passed, but otherwise shows a list of the servers. That command also takes `--help`, `-h` and `--skip-inactive` parameters.

We will want to write a slightly more complicated logic so that when the completion does not start with `-`, and if we don't already have a `server` defined in our parameters, we want to load the list of servers to offer them for completion. We also will want to avoid returning the `-`-prefixed parameters when the current work does not start with a dash.

Here is an example completion we could add to `/path/to/server.sh`:

```bash title="/path/to/server.sh" showLineNumbers
#!/usr/bin/env bash

if [[ "$1" == "--complete" ]]; then
        # We get rid of the first parameter, since we now know
        # we're in completion mode
        shift

        # Check all the parameters until $COMP_CWORD to identify
        # if they all start with a -, which would indicate we
        # don't have a server yet, and need to offer to autocomplete
        # to one, except if the current word also starts with -.
        local COMPLETE_OPTIONS=(
                "--help"
                "-h"
                "--skip-inactive"
        )
        local COMPLETE_SERVERS=true
        if [[ -n "$@" ]]; then
                # If the last word is not empty, we can just use it
                # as a hint of what the user is trying to complete
                if [[ -n "${!COMP_CWORD}" ]]; then
                        if [[ "${!COMP_CWORD}" =~ ^- ]]; then
                                # The current completion word starts with
                                # a dash, so we know it's not going to be
                                # a server name
                                COMPLETE_SERVERS=false
                        else
                                # But if the current completion word does
                                # not start with a dash, we know it's not
                                # going to be an option
                                COMPLETE_OPTIONS=()
                        fi
                fi

                # Now let's check the previous parameters, if any,
                # and see if all our options have been used already
                # or if we already have a server in the parameters
                for ((i=1; i < COMP_CWORD; i++)); do
                        if [[ "${!i}" =~ ^- ]]; then
                                # Remove the value from available options
                                # since it was already used
                                LEFT_OPTIONS=()
                                for param in "${COMPLETE_OPTIONS[@]}"; do
                                        if [[ "$param" != "${!i}" ]]; then
                                                LEFT_OPTIONS+=("$param")
                                        fi
                                done
                                LEFT_OPTIONS=("$LEFT_OPTIONS[@]}")
                        else
                                # A parameter not prefixed by dash, we
                                # can skip completing servers
                                COMPLETE_SERVERS=false
                        fi
                done
        fi

        if [[ "${#COMPLETE_OPTIONS[@]}" -gt 0 ]]; then
                # Print completion for options if we have any we
                # can offer to complete
                for param in "${COMPLETE_OPTIONS[@]}"; do
                        echo "$param"
                done
        fi

        if [[ "$COMPLETE_SERVERS" == "true" ]]; then
                # Let's consider a separate omni command that lists
                # all the available servers
                omni list-servers
        fi

        exit 0
fi

# Rest of the code
```
