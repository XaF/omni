---
description: How to make commands available through omni
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Make commands available

The [previous step](up-configuration) taught us how to handle dependencies, and now that we can execute our scripts and tools, we can also make them available as omni commands.

Making scripts and tools into omni commands allow anybody using the repository to simply call those in a standard, known way. It also makes those commands appear in `omni help` while in the repository, making it easy to discover available commands.

There are two main ways to make commands accessible with omni: [configuration commands](/reference/custom-commands/configuration) and [path commands](/reference/custom-commands/path).

## Define configuration commands

### Declare commands

One simple way to make commands available while in the repository is to define [configuration commands](/reference/custom-commands/configuration). As the name hints, those commands are defined directly in the omni configuration at the root of the git repository.

The same commands we ran on the command line previously can be defined as follows in our configuration file:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

# highlight-start
commands:
  pysayhello:
    run: python python/pysayhello.py "$@"

  gosayhello:
    run: go/bin/gosayhello "$@"
# highlight-end
```

This now enables anyone in the repository to run `omni pysayhello xaf` and `omni gosayhello xaf` to call our commands. Note the `"$@"` at the end of the `run` parameter, which is bash for "pass all the arguments received along", which will allow to pass the arguments received after `omni pysayhello` and `omni gosayhello` directly to our commands.

### Add commands help

However, running the `omni help` command would not help understanding what our commands are doing:

```shell-session title="omni help"
[...]

Configuration < .omni.yaml
  gosayhello
  pysayhello
```

We can improve that by giving a command description directly in the configuration, and categorizing our commands too:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

commands:
  pysayhello:
# highlight-start
    category: Hello world
    desc: |
      Says hello in Python

      This is a simple script to say hello,
      and it is written in Python!
# highlight-end
    run: python python/pysayhello.py "$@"

  gosayhello:
# highlight-start
    category: Hello world
    desc: |
      Says hello in Go

      This is a simple script to say hello,
      and it is written in Go!
# highlight-end
    run: go/bin/gosayhello "$@"
```

Now let's take another look at the help:

```shell-session title="omni help"
[...]

Hello world < Configuration < .omni.yaml
  gosayhello                Says hello in Go
  pysayhello                Says hello in Python
```

This is better! But what about the individual help messages?

<Tabs>
  <TabItem value="go" label="omni help gosayhello" default>

```shell-session
omni - omnipotent tool

Says hello in Go

This is a simple script to say hello, and it is written in Go!

Usage: omni gosayhello

Source: .omni.yaml
```

  </TabItem>
  <TabItem value="python" label="omni help pysayhello">

```shell-session
omni - omnipotent tool

Says hello in Python

This is a simple script to say hello, and it is written in Python!

Usage: omni pysayhello

Source: .omni.yaml
```

  </TabItem>
</Tabs>

We can see the long help of our command appearing. However, something's missing: the usage syntax does not indicate **we have to pass** the name of someone as argument to personalize the message, nor that **we can pass** the `--goodbye` option to say good bye instead of hello. We can fix this:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

commands:
  pysayhello:
    category: Hello world
    desc: |
      Says hello in Python

      This is a simple script to say hello,
      and it is written in Python!
# highlight-start
    syntax:
      arguments:
        - name: Personalize the message for that target name
      options:
        - "--goodbye": Say good bye instead of hello
# highlight-end
    run: python python/pysayhello.py "$@"

  gosayhello:
    category: Hello world
    desc: |
      Says hello in Go

      This is a simple script to say hello,
      and it is written in Go!
# highlight-start
    syntax:
      arguments:
        - name: Personalize the message for that target name
      options:
        - "--goodbye": Say good bye instead of hello
# highlight-end
    run: go/bin/gosayhello "$@"
```

And now let's check again the commands help messages:

<Tabs>
  <TabItem value="go" label="omni help gosayhello" default>

```shell-session
omni - omnipotent tool

Says hello in Go

This is a simple script to say hello, and it is written in Go!

Usage: omni gosayhello <name> [--goodbye]

  name           Personalize the message for that target name

  --goodbye      Say good bye instead of hello

Source: .omni.yaml
```

  </TabItem>
  <TabItem value="python" label="omni help pysayhello">

```shell-session
omni - omnipotent tool

Says hello in Python

This is a simple script to say hello, and it is written in Python!

Usage: omni pysayhello <name> [--goodbye]

  name           Personalize the message for that target name

  --goodbye      Say good bye instead of hello

Source: .omni.yaml
```

  </TabItem>
</Tabs>

Our commands are now both easily accessible by an omni command while inside the repository, but also provide decent help to whomever wishes to use them.

## Define path commands

### Declare commands

[Configuration commands](/reference/custom-commands/configuration) cannot be part of the `omnipath`, which would allow to make our commands usable from anywhere omni is accessible (without the help of [`omni scope`](/reference/builtin-commands/scope)). If we want to make our commands part of the `omnipath`, we can change them to be [path commands](/reference/custom-commands/path) instead.

We can remove the `commands` key in the configuration and add a new `path` key which will target directories where our executable files are located. Only executable files will be considered. In our case, this will look as follows:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

# highlight-start
path:
  append:
    - python
    - go/bin
# highlight-end
```

:::note
Do not forget to set `python/pysayhello.py` as executable (`chmod +x` on MacOS and Linux) or it won't be considered by `omni`
:::

From the repository, we can now run `omni pysayhello xaf` and `omni gosayhello xaf` as [path commands](/reference/custom-commands/path). Note that we didn't have to add anything else than the path to handle parameters, as omni will directly pass all leftover parameters after identifying the executable that needs to be run.

### Add commands help

However, we are in the same pickle as before regarding the help messages shown by `omni help`:

```shell-session title="omni help"
[...]

Uncategorized
  gosayhello
  pysayhello
```

We can fix that using [metadata headers](/reference/custom-commands/path/metadata#metadata-headers) in text files, that omni will directly read from the file when trying to provide help for a command. Of course, since we are using a binary file for our Go tool, this method cannot directly work, so we need to create a new executable file, in bash for instance, that will wrap the call to our tool and provide readable [metadata headers](/reference/custom-commands/path/metadata) to omni.

These are the new `go/wrapper/gosayhello.sh` and the modified `python/pysayhello.py` files:

<Tabs>
  <TabItem value="go" label="Go tool" default>

```bash showLineNumbers title="/path/to/repo/go/wrapper/gosayhello.sh"
#!/usr/bin/env bash
#
# category: Hello world
# arg: name: Personalize the message for that name
# opt:--goodbye: Say good bye instead of hello
# help: Says hello in Go
# help:
# help: This is a simple script to say hello,
# help: and it is written in Go!

# This script's directory
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Run the binary
exec "${DIR}/../bin/gosayhello" "$@"
```

  </TabItem>
  <TabItem value="python" label="Python tool">

```python showLineNumbers title="/path/to/repo/python/pysayhello.py"
#!/usr/bin/env python
# highlight-start
#
# category: Hello world
# arg: name: Personalize the message for that name
# opt:--goodbye: Say good bye instead of hello
# help: Says hello in Python
# help:
# help: This is a simple script to say hello,
# help: and it is written in Python!
# highlight-end

import argparse

parser = argparse.ArgumentParser(description='Say hello in Python')
parser.add_argument('name', help='name to greet')
parser.add_argument('--goodbye', action='store_true')

args = parser.parse_args()

greeting = "Goodbye" if args.goodbye else "Hello"
print("{}, {}!".format(greeting, args.name))
```

  </TabItem>
</Tabs>

We will also need to update our `path` variable to point to the new `go/wrapper` directory:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

path:
  append:
    - python
# highlight-start
    - go/wrapper
# highlight-end
```

:::note
Do not forget to set `go/wrapper/gosayhello.sh` as executable (`chmod +x` on MacOS and Linux) or it won't be considered by `omni`
:::

We can now see the commands metadata reflected in the `omni help` message:

```shell-session title="omni help"
[...]

Hello world
  gosayhello                Says hello in Go
  pysayhello                Says hello in Python
```

As well as the individual help messages:

<Tabs>
  <TabItem value="go" label="omni help gosayhello" default>

```shell-session
omni - omnipotent tool

Says hello in Go

This is a simple script to say hello, and it is written in Go!

Usage: omni gosayhello <name> [--goodbye]

  name           Personalize the message for that name

  --goodbye      Say good bye instead of hello

Source: go/wrapper/gosayhello.sh
```

  </TabItem>
  <TabItem value="python" label="omni help pysayhello">

```shell-session
omni - omnipotent tool

Says hello in Python

This is a simple script to say hello, and it is written in Python!

Usage: omni pysayhello <name> [--goodbye]

  name           Personalize the message for that name

  --goodbye      Say good bye instead of hello

Source: python/pysayhello.py
```

  </TabItem>
</Tabs>

We can also observe that, using [path commands](/reference/custom-commands/path), the help message reflects the path to our tools as `Source` of the command.

### Add commands to global `omnipath`

Finally, now that our commands are ready, if we want to make them available from anywhere in our system, we could simply edit our user configuration's `path` configuration to contain the paths to our executable directories:

```yaml showLineNumbers title="~/config/omni/config.yaml"
# [...]

path:
  append:
    # [...]
    - /path/to/repo/python
    - /path/to/repo/go/wrapper

# [...]
```

## Bonus: have commands use `omni help`

As a last step to fully integrate our commands with omni, we might want `omni gosayhello --help` and `omni pysayhello --help` to be identical to `omni help gosayhello` and `omni help pysayhello` respectively. Since omni will directly pass arguments after a command to the command itself, it needs to be handled directly from our commands.

<Tabs>
  <TabItem value="go-wrapper" label="Go tool (using shell wrapper)" default>

```bash showLineNumbers title="/path/to/repo/go/wrapper/gosayhello.sh"
#!/usr/bin/env bash
#
# category: Hello world
# arg: name: Personalize the message for that name
# opt:--goodbye: Say good bye instead of hello
# help: Says hello in Go
# help:
# help: This is a simple script to say hello,
# help: and it is written in Go!

# highlight-start
if [[ " $* " == *" --help "* ]] || [[ " $* " == *" -h "* ]]; then
    omni help ${OMNI_SUBCOMMAND:-gosayhello}
    exit 0
fi
# highlight-end

# This script's directory
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Run the binary
exec "${DIR}/../bin/gosayhello" "$@"
```

  </TabItem>
  <TabItem value="go" label="Go tool (directly in Go)" default>

```bash showLineNumbers title="/path/to/repo/go/src/gosayhello.go"
package main

import (
	"flag"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

func main() {
	goodbye := flag.Bool("goodbye", false, "say goodbye instead of hello")
# highlight-start
	help := flag.Bool("help", false, "print usage")
	shortHelp := flag.Bool("h", false, "print usage")
# highlight-end
	flag.Parse()

# highlight-start
	if *help || *shortHelp {
		// Read OMNI_SUBCOMMAND from environment, and split it over spaces
		// to get the subcommand name.  If it's empty, then just assume
		// the command is `gosayhello`
		var helpCommand []string
		omniSubcommand := os.Getenv("OMNI_SUBCOMMAND")
		if omniSubcommand != "" {
			helpCommand = strings.Split(omniSubcommand, " ")
		} else {
			helpCommand = []string{"gosayhello"}
		}

		// Prepend `help` to the subcommand name
		helpCommand = append([]string{"help"}, helpCommand...)

		// Then call omni to get the help for the subcommand, while
		// passing through stdout and stderr
		cmd := exec.Command("omni", helpCommand...)
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
		if err := cmd.Run(); err != nil {
			fmt.Printf("Error executing command: %s\n", err)
			os.Exit(1)
		}
		os.Exit(0)
	}
# highlight-end

	name := flag.Arg(0)
	if name == "" {
		panic("Who do you wanna greet?")
	}

	greeting := "Hello"
	if *goodbye {
		greeting = "Goodbye"
	}

	fmt.Printf("%s, %s!\n", greeting, name)
}
```

  </TabItem>
  <TabItem value="python" label="Python tool">

```python showLineNumbers title="/path/to/repo/python/pysayhello.py"
#!/usr/bin/env python
#
# category: Hello world
# arg: name: Personalize the message for that name
# opt:--goodbye: Say good bye instead of hello
# help: Says hello in Python
# help:
# help: This is a simple script to say hello,
# help: and it is written in Python!

import argparse
import os

# highlight-start
parser = argparse.ArgumentParser(description='Say hello in Python', add_help=False)
parser.add_argument('-h', '--help', action='store_true', help='show help')
parser.add_argument('name', help='name to greet', nargs='?', default=None)
# highlight-end
parser.add_argument('--goodbye', action='store_true')

args = parser.parse_args()

# highlight-start
if args.help:
    subcommand = (os.getenv('OMNI_SUBCOMMAND', '') or 'pysayhello').split(' ')
    args = ['omni', 'help'] + subcommand
    os.execvp(args[0], args)
    exit(1)
# highlight-end

# highlight-start
if args.name is None:
    parser.error("Who do you wanna greet?")
# highlight-end

greeting = "Goodbye" if args.goodbye else "Hello"
print("{}, {}!".format(greeting, args.name))

```

  </TabItem>
</Tabs>

:::note
For the Go tool, we have the choice to handle things in the wrapper, or directly in the Go source. There is no need to handle it in both places at once, if we consider that our tool will always be called through omni.
:::
