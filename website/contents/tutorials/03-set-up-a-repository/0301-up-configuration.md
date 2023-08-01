---
description: How to configure setting up and tearing down a repository
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Write the `up` configuration

Omni can be added to manage a repository dependencies pretty easily: just create an `.omni.yaml` file at the root of your repository and define the `up` configuration for your dependencies.

Let's consider a simple repository with two tools we want to work on. One in Go, and one in Python:

<Tabs>
  <TabItem value="go" label="Go tool" default>

```go showLineNumbers title="/path/to/repo/go/src/gosayhello.go"
package main

import (
	"flag"
	"fmt"
)

func main() {
	goodbye := flag.Bool("goodbye", false, "say goodbye instead of hello")
	flag.Parse()

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

We can first add the relevant dependencies to make sure we have access to the right tools; since we don't have a version preference here, we can also simply make sure we have access to the latest version:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
```

Running `omni up` will now install those dependencies for us, and omni's [dynamic environment](/reference/dynamic-environment) will make sure that we can use them right away.

This means that we can now run our commands through `python python/pysayhello.py xaf` or `go run go/src/gosayhello.go xaf` even if Python and Go are not accessible anywhere else in our system: we are using omni's dynamic environment, which gives us access to the expected versions of those tools to run our commands.

You might observe a slight delay when running `go run go/src/gosayhello.go xaf`. This is related to calling `go run` everytime we run that command. To make things more efficient at call time, we can compile a binary during `omni up` using a [`custom` operation](/reference/configuration/parameters/up/custom):

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
# highlight-start
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"
# highlight-end
```

Which means that instead of calling `go run go/src/gosayhello.go xaf`, we can now call `go/bin/gosayhello xaf` directly. That binary will automatically be updated, if necessary, anytime we run `omni up` in that repository.
