#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  # Setup the environment for the test; this should override $HOME too
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Add one repository
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"

  # Change directory to the repository
  cd "git/github.com/test1org/test1repo"

  # Add trust
  omni config trust 3>&-

  # Disable colors
  export NO_COLOR=1

  # Avoid wrapping
  export COLUMNS=1000
}

# bats test_tags=config:commands
@test "[config_commands=01] omni config commands not found" {
  run omni doesnotexist 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
  [ "$output" = "omni: command not found: doesnotexist" ]
}

# bats test_tags=config:commands
@test "[config_commands=02] omni config commands" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    desc: This is a custom command
    run: |
      echo "Hello, world!"
EOF

  run omni customcommand 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, world!" ]
}

# bats test_tags=config:commands
@test "[config_commands=03] omni config commands with arguments" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    desc: This is a custom command
    run: |
      echo "Hello, \$@!"
EOF

  run omni customcommand this is me 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, this is me!" ]
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=04] omni config commands argparser without syntax" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    desc: This is a custom command
    run: |
      echo "Hello, world!"
EOF

  run omni customcommand --help 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=05] omni config commands argparser with syntax" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --name
        desc: The name of the person
    desc: This is a custom command
    run: |
      echo "Hello, \${OMNI_ARG_NAME_VALUE:-world}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--name <NAME>\s*The name of the person"

  run omni customcommand --name me 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, me!" ]

  run omni customcommand 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, world!" ]
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=06] omni config commands argparser with int value" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --number
        desc: The number
        type: int
    desc: This is a custom command
    run: |
      echo "Hello, you are the \${OMNI_ARG_NUMBER_TYPE:-unknown} \${OMNI_ARG_NUMBER_VALUE:-0}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--number <NUMBER>\s*The number"

  run omni customcommand --number 42 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the int 42!" ]

  run omni customcommand 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the int 0!" ]

  run omni customcommand --number 42.5 3>&-
  echo "4. STATUS: $status"
  echo "4. OUTPUT: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "invalid value"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=07] omni config commands argparser with float value" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --number
        desc: The number
        type: float
    desc: This is a custom command
    run: |
      echo "Hello, you are the \${OMNI_ARG_NUMBER_TYPE:-unknown} \${OMNI_ARG_NUMBER_VALUE:-0}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--number <NUMBER>\s*The number"

  run omni customcommand --number 42.5 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the float 42.5!" ]

  run omni customcommand 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the float 0!" ]

  run omni customcommand --number 42 3>&-
  echo "4. STATUS: $status"
  echo "4. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the float 42!" ]

  run omni customcommand --number 42.5.5 3>&-
  echo "5. STATUS: $status"
  echo "5. OUTPUT: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "invalid value"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=08] omni config commands argparser with bool value" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --bool
        desc: The bool
        type: bool
    desc: This is a custom command
    run: |
      echo "Hello, you are the \${OMNI_ARG_BOOL_TYPE:-unknown} \${OMNI_ARG_BOOL_VALUE:-0}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--bool <BOOL>\s*The bool \[possible values: true, false\]"

  run omni customcommand 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the bool false!" ]

  run omni customcommand --bool true 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the bool true!" ]

  run omni customcommand --bool false 3>&-
  echo "4. STATUS: $status"
  echo "4. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the bool false!" ]

  run omni customcommand --bool 1 3>&-
  echo "5. STATUS: $status"
  echo "5. OUTPUT: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "invalid value '1'"

  run omni customcommand --bool 0 3>&-
  echo "6. STATUS: $status"
  echo "6. OUTPUT: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "invalid value '0'"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=09] omni config commands argparser with flag value" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --flag1
        desc: The flag 1
        type: flag
      - name: --flag2
        desc: The flag 2
        type: flag
        default: true
    desc: This is a custom command
    run: |
      printenv | grep ^OMNI_ARG_ | sort
      echo "Flag1 is \${OMNI_ARG_FLAG1_TYPE:-unknown} \${OMNI_ARG_FLAG1_VALUE:-unset}!"
      echo "Flag2 is \${OMNI_ARG_FLAG2_TYPE:-unknown} \${OMNI_ARG_FLAG2_VALUE:-unset}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--flag1\s*The flag 1"
  echo "$output" | grep -q -- "--flag2\s*The flag 2"

  run omni customcommand 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Flag1 is bool false!"
  echo "$output" | grep -q "Flag2 is bool true!"

  run omni customcommand --flag1 --flag2 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Flag1 is bool true!"
  echo "$output" | grep -q "Flag2 is bool false!"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=10] omni config commands argparser with enum value" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --enum
        desc: The enum
        type: enum
        values:
        - one
        - two
        - three
    desc: This is a custom command
    run: |
      echo "Hello, you are the \${OMNI_ARG_ENUM_TYPE:-unknown} \${OMNI_ARG_ENUM_VALUE:-unset}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--enum <ENUM>\s*The enum \[possible values: one, two, three\]"

  run omni customcommand 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the str unset!" ]

  run omni customcommand --enum one 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the str one!" ]

  run omni customcommand --enum two 3>&-
  echo "4. STATUS: $status"
  echo "4. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the str two!" ]

  run omni customcommand --enum three 3>&-
  echo "5. STATUS: $status"
  echo "5. OUTPUT: $output"
  [ "$status" -eq 0 ]
  [ "$output" = "Hello, you are the str three!" ]

  run omni customcommand --enum four 3>&-
  echo "6. STATUS: $status"
  echo "6. OUTPUT: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "invalid value 'four'"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=11] omni config commands argparser with multiple values" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --name
        desc: The name of the person
      - name: --number
        desc: The number
        type: int
      - name: --bool
        desc: The bool
        type: bool
      - name: --enum
        desc: The enum
        type: enum
        values:
        - one
        - two
        - three
    desc: This is a custom command
    run: |
      printenv | grep ^OMNI_ARG_ | sort
      echo "Hello, \${OMNI_ARG_NAME_VALUE:-world}!"
      echo "Number is \${OMNI_ARG_NUMBER_TYPE:-unknown} \${OMNI_ARG_NUMBER_VALUE:-unset}!"
      echo "Bool is \${OMNI_ARG_BOOL_TYPE:-unknown} \${OMNI_ARG_BOOL_VALUE:-unset}!"
      echo "Enum is \${OMNI_ARG_ENUM_TYPE:-unknown} \${OMNI_ARG_ENUM_VALUE:-unset}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--name <NAME>\s*The name of the person"
  echo "$output" | grep -q -- "--number <NUMBER>\s*The number"
  echo "$output" | grep -q -- "--bool <BOOL>\s*The bool \[possible values: true, false\]"
  echo "$output" | grep -q -- "--enum <ENUM>\s*The enum \[possible values: one, two, three\]"

  run omni customcommand --name me --number 42 --bool true --enum one 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Hello, me!"
  echo "$output" | grep -q "Number is int 42!"
  echo "$output" | grep -q "Bool is bool true!"
  echo "$output" | grep -q "Enum is str one!"

  run omni customcommand --name me --number 42 --bool true --enum one --enum two 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "cannot be used multiple times"
}

# bats test_tags=config:commands,config:commands:argparser
@test "[config_commands=12] omni config commands argparser with default values" {
  cat > .omni.yaml <<EOF
commands:
  customcommand:
    argparser: true
    syntax:
      parameters:
      - name: --name
        desc: The name of the person
        default: My Super Name
      - name: --number
        desc: The number
        type: int
        default: 21
      - name: --bool
        desc: The bool
        type: bool
        default: true
      - name: --enum
        desc: The enum
        type: enum
        values:
        - one
        - two
        - three
        default: two
    desc: This is a custom command
    run: |
      printenv | grep ^OMNI_ARG_ | sort
      echo "Name is \${OMNI_ARG_NAME_TYPE:-unknown} \${OMNI_ARG_NAME_VALUE:-unset}!"
      echo "Number is \${OMNI_ARG_NUMBER_TYPE:-unknown} \${OMNI_ARG_NUMBER_VALUE:-unset}!"
      echo "Bool is \${OMNI_ARG_BOOL_TYPE:-unknown} \${OMNI_ARG_BOOL_VALUE:-unset}!"
      echo "Enum is \${OMNI_ARG_ENUM_TYPE:-unknown} \${OMNI_ARG_ENUM_VALUE:-unset}!"
EOF

  run omni customcommand --help 3>&-
  echo "1. STATUS: $status"
  echo "1. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Usage: omni customcommand"
  echo "$output" | grep -q -- "--name <NAME>\s*The name of the person \[default: My Super Name\]"
  echo "$output" | grep -q -- "--number <NUMBER>\s*The number \[default: 21\]"
  echo "$output" | grep -q -- "--bool <BOOL>\s*The bool \[default: true\] \[possible values: true, false\]"
  echo "$output" | grep -q -- "--enum <ENUM>\s*The enum \[default: two\] \[possible values: one, two, three\]"

  run omni customcommand 3>&-
  echo "2. STATUS: $status"
  echo "2. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Name is str My Super Name!"
  echo "$output" | grep -q "Number is int 21!"
  echo "$output" | grep -q "Bool is bool true!"
  echo "$output" | grep -q "Enum is str two!"

  run omni customcommand --name me --number 42 --bool false --enum one 3>&-
  echo "3. STATUS: $status"
  echo "3. OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Name is str me!"
  echo "$output" | grep -q "Number is int 42!"
  echo "$output" | grep -q "Bool is bool false!"
  echo "$output" | grep -q "Enum is str one!"
}
