---
description: The argument parser for custom commands
---

# Argument parser

Omni provides an argument parser for custom commands. This argument parser reads the command metadata to parse the user input, and puts the resulting data into environment variables to be consumed by the command.

To configure the argument parser, set the [`argparser` metadata](metadata#argparser) to `true`, and define the required arguments and optional parameters using the [`arg` and `opt` metadata](metadata#arg-and-opt). The argument parser can be used for path commands and configuration commands.

## Environment variables

The argument parser sets the following environment variables:

| Variable                | Description                                                            |
|-------------------------|------------------------------------------------------------------------|
| `OMNI_ARG_LIST`         | The list of arguments parsed by the argument parser for the command. This environment variable is always present, even if empty in case where no argument was configured, as long as the argument parser itself was configured to be called. The absence of this variable indicates that the argument parser was not configured for the command, which can be used to raise an error in the command. |
| `OMNI_ARG_<argname>_TYPE` | The type of the argument `<argname>` parsed by the argument parser for the command. Can be one of `str`, `int`, `float`, `bool` for single-value arguments, or any `<type>/<size>` where `<type>` is one of the previous types and `<size>` is the number of values for multi-value arguments. It can also be `<type>/<size1>/<size2>` when grouping occurrences, with `<size1>` being the number of occurrences of the parameter, and `<size2>` the maximum number of values passed to an occurrence. This environment variable is present for each argument defined in the command metadata. The absence of this variable indicates that the argument was not configured for the command. |
| `OMNI_ARG_<argname>_TYPE_<index>` | The type for the occurrence group `<index>` of the argument `<argname>` parsed by the argument parser for the command. This environment variable is only present when the argument has been configured to group occurrences, and both takes multiple values and can be repeated. |
| `OMNI_ARG_<argname>_VALUE` | The value of the argument `<argname>` parsed by the argument parser for the command, if the type is a single-value type. This environment variable can be absent if the argument was not provided by the user and no default value was specified in the command metadata. |
| `OMNI_ARG_<argname>_VALUE_<index>` | The value at index `<index>` of the argument `<argname>` parsed by the argument parser for the command, if the type is a multi-value type. The index is 0-based. This environment variable can be absent if the argument was not provided by the user and no default value was specified in the command metadata. The absence of the variable at index N does not imply the absence of the variable at index N+1, the `<size>` part of the type should always be trusted to determine the number of values. |
| `OMNI_ARG_<argname>_VALUE_<index1>_<index2>` | The value at index `<index2>` of the occurrence group `<index1>` of the argument `<argname>` parsed by the argument parser for the command. The indices are 0-based. This environment variable is only present when the argument has been configured to group occurrences, and both takes multiple values and can be repeated. |

### Examples

For a command with the following metadata:

```yaml
argparser: true
syntax:
  parameters:
    - name: --input-file
      type: str
    - name: --verbose
      type: flag
    - name: workers
      type: array/str
```

The following command-line invocation:

```bash
omni my-command \
    --input-file /path/to/file \
    --verbose \
    --workers worker1 \
    --workers worker2 \
    --workers worker3
```

Will lead to the following environment variables being set:

```bash
OMNI_ARG_LIST="input_file verbose workers"
OMNI_ARG_INPUT_FILE_TYPE="str"
OMNI_ARG_INPUT_FILE_VALUE="/path/to/file"
OMNI_ARG_VERBOSE_TYPE="bool"
OMNI_ARG_VERBOSE_VALUE="true"
OMNI_ARG_WORKERS_TYPE="str/3"
OMNI_ARG_WORKERS_VALUE_0="worker1"
OMNI_ARG_WORKERS_VALUE_1="worker2"
OMNI_ARG_WORKERS_VALUE_2="worker3"
```

While calling the command like this:

```bash
omni my-command
```

Will lead to the following environment variables being set:

```bash
OMNI_ARG_LIST="input_file verbose workers"
OMNI_ARG_INPUT_FILE_TYPE="str"
OMNI_ARG_VERBOSE_TYPE="bool"
OMNI_ARG_WORKERS_TYPE="str/0"
```


## The SDKs

To simplify the process of writing custom commands, omni provides SDKs for different languages. These SDKs take care of converting the environment variables into a language-native format.

### Python

The Python SDK is available as the `omnicli-sdk` package on PyPI. You can find the source code on [`omnicli/sdk-python`](https://github.com/omnicli/sdk-python). You can install it using pip:

```bash
pip install omnicli-sdk
```

The SDK provides a `parse_args` function that reads the environment variables and returns an [`argparse.Namespace`](https://docs.python.org/3/library/argparse.html#argparse.Namespace) object, with the arguments and options as attributes, converted to the appropriate types. If the `OMNI_ARG_LIST` environment variable is not set, the function raises an `ArgListMissingError`.

#### Example

```python
from omnicli import parse_args

try:
    args = parse_args()

    # Access your command's arguments as attributes
    if args.verbose:
        print("Verbose mode enabled")

    if args.input_file:
        print(f"Processing file: {args.input_file}")

except ArgListMissingError:
    print("No Omni CLI arguments found. Make sure 'argparser: true' is set for your command.")
```

### Go

The Go SDK is available as the [`github.com/omnicli/sdk-go`](https://github.com/omnicli/sdk-go) package, where you can find the source code. You can install it using `go get`:

```bash
go get github.com/omnicli/sdk-go
```

The SDK provides a `ParseArgs(targets ...interface{}) (*Args, error)` function that reads the environment variables and returns an `Args` object. This function can be used one of two ways: by passing a pointer to a struct to be filled with the parsed arguments, or by using the resulting `Args` object directly to access the parsed arguments.

#### Example

```go
package main

import (
    "log"

    omnicli "github.com/omnicli/sdk-go"
)

type Config struct {
    // Fields are automatically mapped to kebab-case CLI arguments
    InputFile string    // maps to --input-file
    Verbose   bool      // maps to --verbose
    LogFile   *string   // maps to --log-file, optional
    Workers   []string  // maps to --workers (array)

    // Use tags for custom names or to skip fields
    DBHost    string    `omniarg:"db_host"`  // custom name
    Internal  string    `omniarg:"-"`        // skip this field
}

func main() {
    var cfg Config
    args, err := omnicli.ParseArgs(&cfg)
    if err != nil {
        log.Fatalf("Failed to parse args: %v", err)
    }

    if cfg.Verbose {
        log.Println("Verbose mode enabled")
    }
    if cfg.InputFile != "" {
        log.Printf("Processing file: %s", cfg.InputFile)
    }

    dbHost, err := args.GetString("db_host")
    if err != nil {
        log.Fatalf("Failed to get db host: %v", err)
    }
    log.Printf("DB Host: %v", logFile)
}
```

#### Extracting arguments manually

The `Args` object provides getter methods. Each methods returns a tuple with the value and a boolean indicating if the value exists. If the value does not exist, the getter returns the zero value for the type and `false`. The getter methods are:
- `GetString(name string) (string, bool)`: Get the value of a string argument.
- `GetBool(name string) (bool, bool)`: Get the value of a boolean argument.
- `GetInt(name string) (int, bool)`: Get the value of an integer argument.
- `GetFloat(name string) (float64, bool)`: Get the value of a float argument.
- `GetStringSlice(name string) ([]string, bool)`: Get the value of a string slice argument.
- `GetBoolSlice(name string) ([]bool, bool)`: Get the value of a boolean slice argument.
- `GetIntSlice(name string) ([]int, bool)`: Get the value of an integer slice argument.
- `GetFloatSlice(name string) ([]float64, bool)`: Get the value of a float slice argument.

#### Using a struct

The `Args` object can also be used to fill structs with the parsed arguments, the same way that the `ParseArgs` function does. You can use the following methods:
- `Fill(target interface{}) error`: Fill the fields of the target struct with the parsed arguments. You can use the `omniarg` tag to specify custom names for the arguments (using `omniarg:"custom_name"`), or to skip fields (using `omniarg:"-"`). The function returns an error if any field in the struct does not match an argument, or if the types do not match. It returns `nil` if all fields were filled successfully.
- `FillAll(targets... interface{}) error`: Fill multiple structs with the parsed arguments. This function is a convenience method that calls `Fill` for each target struct. This function returns the first error encountered, or `nil` if all structs were filled successfully. This function is called by the `ParseArgs` function if you provided structs to be filled.

### Ruby

The Ruby SDK is available as the [`omnicli` gem on RubyGems](https://rubygems.org/gems/omnicli). You can find the source code on [`omnicli/sdk-ruby`](https://github.com/omnicli/sdk-ruby). You can install it using gem:

```bash
gem install omnicli
```

The SDK provides a `parse!` method that reads the environment variables and returns an `Hash{Symbol => Object}` object, with the arguments and options as attributes, converted to the appropriate types. If the `OMNI_ARG_LIST` environment variable is not set, the method raises an `Omnicli::ArgListMissingError`.

#### Example

```ruby
require 'omnicli'

begin
  args = OmniCli.parse!
  # Access your command's arguments as hash keys
  if args[:verbose]
    puts "Verbose mode enabled"
  end
  if args[:input_file]
    puts "Processing file: #{args[:input_file]}"
  end
rescue OmniCli::ArgListMissingError
  puts "No Omni CLI arguments found. Make sure 'argparser: true' is set for your command."
end
```

### Shells

There is no SDK for the shells at this time, as the shells already work with environment variables. The only manual handling required is for arrays that are split in multiple environment variables instead of a single "array" shell variable. For instance in bash, these can be handled the following way:

```bash
workers=()
num_worker_values=${OMNI_ARG_WORKERS_TYPE#*/}
i=0
while [[ $i -lt $num_worker_values ]]; do
    value_var="OMNI_ARG_WORKERS_VALUE_$i"
    workers+=("${!value_var}")
    ((i++))
done
```
