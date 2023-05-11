```
  :?:.!YG#&@@@&#GJ~.
 7#@&#@@@&#BGB#&@@@#Y^
 ^&@@@@&?.     :~Y#@@@Y
.G@@&#@@&5^       .J@@@G.   OMNI
P@@@! 7B@@@P~       7@@@5
@@@P    !B@@@G~      G@@&     THE OMNIPOTENT
@@@P    !B@@@B!      G@@&        DEV TOOL
P@@@~ 7B@@@P~       !@@@5
.G@@&B@@&P~       .?@@@G.
 ^#@@@@&?.     .~J#@@@Y.
 7&@&#@@@&BGGGB&@@@#5^
  :?^.!YG#@@@@@#GY!.
```

# omni - omnipotent tool

This aims at providing a number of helper commands when using `omni <target>`.
This aims at being extensible to provide any other kind of command underneath.
This is a work in progress...


## Installation

Installing omni is as simple as running:

```
curl https://raw.githubusercontent.com/XaF/omni/main/install.sh | bash
```

The installation script will attempt to install those dependencies and setup omni for you. In case of issue, here is what you need to know:

You will need the following dependencies:
- `rbenv`, which will be used to install ruby 3.2.2 (aims at allowing auto-handling of ruby versions for omni in the future)
- `uuidgen`, which is used to generate UUIDs for the subcommand sessions

In order to work as expected, omni will also require its shell integration, which you can add to your `.bashrc` or `.zshrc` as desired. We recommend using a symbolic link so that the shell integration can stay up to date with omni updates without requiring any intervention on your part.


## Configuration

### Parameters

Omni accepts the following parameters as part of its configuration:

- `clone` *[map]* configuration related to the `omni clone` command
  - `auto_up` *[boolean]* whether or not `omni up` should be run automatically when cloning a repository
- `cache` *[map]* configuration related to the cache of omni
  - `path` *[filepath]* the path to the cache file *(default; $HOME/.cache/omni)*
- `config_commands` *[map]* configuration related to the commands defined in the config file
  - `split_on_dash` *[boolean]* whether or not the commands should be split on dash (e.g. 'my-command' would be used as 'omni my command' instead of 'omni my-command') *(default: true)*
  - `split_on_slash` *[boolean]* whether or not the commands should be split on slash (e.g. 'my/command' would be used as 'omni my command' instead of 'omni my/command') *(default: true)*
- `env` *[map, string => string]* a key-value map of environment variables to be set when running omni commands
- `makefile_commands` *[map]* configuration related to the commands generated from Makefile targets
  - `enabled` *[boolean]* whether or not to load commands from the Makefiles in the current path and parents (up to the root of the git repository, or user directory) *(default: true)*
  - `split_on_dash` *[boolean]* whether or not the targets should be split on dash (e.g. 'my-target' would be used as 'omni my target' instead of 'omni my-target') *(default: true)*
  - `split_on_slash` *[boolean]* whether or not the targets should be split on slash (e.g. 'my/target' would be used as 'omni my target' instead of 'omni my/target') *(default: true)*
- `path_repo_updates` *[map]* configuration for the automated updates of the repositories in **OMNIPATH**
  - `enabled` *[boolean]* whether or not automated updates are enabled *(default: true)*
  - `interval` *[int]* the number of seconds to wait between two updates of the repositories *(default: 43200)*
- `repo_path_format` *[string]* how to format repositories when cloning them with `omni clone`, or searching them with `omni cd`. Variables `%{host}` (registry hostname), `%{org}` (repository owner) and `%{repo}` (repository name) can be used in that format. *(default: `%{host}/%{org}/%{repo}`)*
- `commands` *[map, string => command object]* commands made available through omni, where the key is the command name, see below for more details on the command object.
- `up` *[list, up object]* list of operations needed to setup (or tear down, in reverse) a repository, see below for more details on the up object.


#### Command object

A command object can contain the following parameters:
- `desc` *[string]* the description of the command that will be used in `omni help`. This can be on multiple lines, in which case the first paragraph (until the first empty line) will be shown in `omni help`, while the rest of the help message will be shown when calling `omni help <command>`.
- `run` *[string, shell script]* the command to run when the command is being called. This will be called through `bash -c` and can thus receive any kind of bash scripting, or call to an executable file.
- `syntax` *[map]* map with two accepted keys: `arguments` and `options`. Each of those keys take a *list* of *map* with a single *param name => param description* value. This will be used when calling `omni help <command>`.

#### Up object

An up object can be one of:

- `bundler` operation object, which can hold the following parameters:
  - `gemfile` *[filepath]* relative path to the Gemfile to use when calling `bundle` operations
  - `path` *[dirpath]* relative path to the vendor bundle directory *(default: vendor/bundle)*

- `homebrew` operation object, which takes a list of packages to install. Any element of the list can be a map with a single *package name => package version* if you wish to install a very specific package version.

- `custom` operation object, which can hold the following parameters:
  - `meet` **(required)** *[string, shell script]* the command to run to meet the requirement
  - `met?` *[string, shell script]* the command to run to know if we are currently meeting the requirement
  - `unmeet` *[string, shell script]* the command to run to 'unmeet' the requirement during tear down

