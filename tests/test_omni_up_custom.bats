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
}

teardown() {
  check_commands
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation" {
  cat > .omni.yaml <<EOF
up:
  - custom:
      name: "Custom Operation"
      meet: "customcmd run"
EOF

  add_fakebin "${HOME}/bin/customcmd"
  add_command customcmd run

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation fails if command fails" {
  cat > .omni.yaml <<EOF
up:
  - custom:
      name: "Custom Operation"
      meet: "customcmd run"
EOF

  add_fakebin "${HOME}/bin/customcmd"
  add_command customcmd run exit=1

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom only executes first command if failing" {
  cat > .omni.yaml <<EOF
up:
  - custom:
      name: "Custom Operation"
      meet: |
        set -e
        customcmd run
        customcmd should-not-run
EOF

  add_fakebin "${HOME}/bin/customcmd"
  add_command customcmd run exit=1

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should not run if 'met' condition is true" {
  cat > .omni.yaml <<EOF
up:
  - custom:
      name: "Custom Operation"
      met?: "customcmd met?"
      meet: "customcmd should-not-run"
EOF

  add_fakebin "${HOME}/bin/customcmd"
  add_command customcmd met?

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should run if 'met' condition is false" {
  cat > .omni.yaml <<EOF
up:
  - custom:
      name: "Custom Operation"
      met?: "customcmd met?"
      meet: "customcmd run"
EOF

  add_fakebin "${HOME}/bin/customcmd"
  add_command customcmd met? exit=1
  add_command customcmd run

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to set environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "ENV_VAR=VALUE" >> "$OMNI_ENV"
        echo "ENV_VAR2=VALUE2" >> "$OMNI_ENV"
EOF

  [ -z "$ENV_VAR" ]
  export ENV_VAR2="otherval"
  [ "$ENV_VAR2" = "otherval" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "ENV_VAR: $ENV_VAR"
  [ "$ENV_VAR" = "VALUE" ]
  echo "ENV_VAR2: $ENV_VAR2"
  [ "$ENV_VAR2" = "VALUE2" ]
}

custom_operation_multiline_single_arrow() {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "SIMPLE<<DELIM" > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo "DELIM" > "$OMNI_ENV"

        echo "NOINDENT<<-DELIM" > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo "DELIM" > "$OMNI_ENV"

        echo "NOINDENT_INDENTEDDELIM<<-DELIM" > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo " DELIM" > "$OMNI_ENV"

        echo "MININDENT<<~DELIM" > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo "DELIM" > "$OMNI_ENV"

        echo "WITHSPACES<< DELIM " > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo "DELIM" > "$OMNI_ENV"

        echo "WITHSQUOTES<<'DELIM'" > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo "DELIM" > "$OMNI_ENV"

        echo "WITHDQUOTES<<\"DELIM\"" > "$OMNI_ENV"
        echo "  line1" > "$OMNI_ENV"
        echo "    line2" > "$OMNI_ENV"
        echo "  line3" > "$OMNI_ENV"
        echo "DELIM" > "$OMNI_ENV"

EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ] || {
    echo "command failed, expected success"
    return 1
  }

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "SIMPLE: BEGIN"
  echo "$SIMPLE"
  echo "SIMPLE: END"
  [ "$SIMPLE" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "SIMPLE does not match"
    return 1
  }

  echo "NOINDENT: BEGIN"
  echo "$NOINDENT"
  echo "NOINDENT: END"
  [ "$NOINDENT" = "$(echo -e "line1\nline2\nline3")" ] || {
    echo "NOINDENT does not match"
    return 1
  }

  echo "NOINDENT_INDENTEDDELIM: BEGIN"
  echo "$NOINDENT_INDENTEDDELIM"
  echo "NOINDENT_INDENTEDDELIM: END"
  [ "$NOINDENT_INDENTEDDELIM" = "$(echo -e "line1\nline2\nline3")" ] || {
    echo "NOINDENT_INDENTEDDELIM does not match"
    return 1
  }

  echo "MININDENT: BEGIN"
  echo "$MININDENT"
  echo "MININDENT: END"
  [ "$MININDENT" = "$(echo -e "line1\n  line2\nline3")" ] || {
    echo "MININDENT does not match"
    return 1
  }

  echo "WITHSPACES: BEGIN"
  echo "$WITHSPACES"
  echo "WITHSPACES: END"
  [ "$WITHSPACES" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "WITHSPACES does not match"
    return 1
  }

  echo "WITHSQUOTES: BEGIN"
  echo "$WITHSQUOTES"
  echo "WITHSQUOTES: END"
  [ "$WITHSQUOTES" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "WITHSQUOTES does not match"
    return 1
  }

  echo "WITHDQUOTES: BEGIN"
  echo "$WITHDQUOTES"
  echo "WITHDQUOTES: END"
  [ "$WITHDQUOTES" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "WITHDQUOTES does not match"
    return 1
  }
}

custom_operation_multiline_double_arrow() {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "SIMPLE<<DELIM" >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo "DELIM" >> "$OMNI_ENV"

        echo "NOINDENT<<-DELIM" >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo "DELIM" >> "$OMNI_ENV"

        echo "NOINDENT_INDENTEDDELIM<<-DELIM" >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo " DELIM" >> "$OMNI_ENV"

        echo "MININDENT<<~DELIM" >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo "DELIM" >> "$OMNI_ENV"

        echo "WITHSPACES<< DELIM " >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo "DELIM" >> "$OMNI_ENV"

        echo "WITHSQUOTES<<'DELIM'" >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo "DELIM" >> "$OMNI_ENV"

        echo "WITHDQUOTES<<\"DELIM\"" >> "$OMNI_ENV"
        echo "  line1" >> "$OMNI_ENV"
        echo "    line2" >> "$OMNI_ENV"
        echo "  line3" >> "$OMNI_ENV"
        echo "DELIM" >> "$OMNI_ENV"

EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ] || {
    echo "command failed, expected success"
    return 1
  }

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "SIMPLE: BEGIN"
  echo "$SIMPLE"
  echo "SIMPLE: END"
  [ "$SIMPLE" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "SIMPLE does not match"
    return 1
  }

  echo "NOINDENT: BEGIN"
  echo "$NOINDENT"
  echo "NOINDENT: END"
  [ "$NOINDENT" = "$(echo -e "line1\nline2\nline3")" ] || {
    echo "NOINDENT does not match"
    return 1
  }

  echo "NOINDENT_INDENTEDDELIM: BEGIN"
  echo "$NOINDENT_INDENTEDDELIM"
  echo "NOINDENT_INDENTEDDELIM: END"
  [ "$NOINDENT_INDENTEDDELIM" = "$(echo -e "line1\nline2\nline3")" ] || {
    echo "NOINDENT_INDENTEDDELIM does not match"
    return 1
  }

  echo "MININDENT: BEGIN"
  echo "$MININDENT"
  echo "MININDENT: END"
  [ "$MININDENT" = "$(echo -e "line1\n  line2\nline3")" ] || {
    echo "MININDENT does not match"
    return 1
  }

  echo "WITHSPACES: BEGIN"
  echo "$WITHSPACES"
  echo "WITHSPACES: END"
  [ "$WITHSPACES" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "WITHSPACES does not match"
    return 1
  }

  echo "WITHSQUOTES: BEGIN"
  echo "$WITHSQUOTES"
  echo "WITHSQUOTES: END"
  [ "$WITHSQUOTES" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "WITHSQUOTES does not match"
    return 1
  }

  echo "WITHDQUOTES: BEGIN"
  echo "$WITHDQUOTES"
  echo "WITHDQUOTES: END"
  [ "$WITHDQUOTES" = "$(echo -e "  line1\n    line2\n  line3")" ] || {
    echo "WITHDQUOTES does not match"
    return 1
  }
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using > (1/5)" {
  custom_operation_multiline_single_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using > (2/5)" {
  custom_operation_multiline_single_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using > (3/5)" {
  custom_operation_multiline_single_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using > (4/5)" {
  custom_operation_multiline_single_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using > (5/5)" {
  custom_operation_multiline_single_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using >> (1/5)" {
  custom_operation_multiline_double_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using >> (2/5)" {
  custom_operation_multiline_double_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using >> (3/5)" {
  custom_operation_multiline_double_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using >> (4/5)" {
  custom_operation_multiline_double_arrow
}

# bats test_tags=omni:up,omni:up:custom,omni:up:custom:multiline
@test "omni up custom operation should allow to set a multiline environment variables using >> (5/5)" {
  custom_operation_multiline_double_arrow
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to unset environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        set -e
        # Make sure that the variable is set
        [ -n "$ENV_VAR" ]
        # Unset the variable
        echo "unset ENV_VAR" >> "$OMNI_ENV"
        echo "unset ENV_VAR2" >> "$OMNI_ENV"
EOF

  export ENV_VAR="VALUE"
  [ "$ENV_VAR" = "VALUE" ]
  unset ENV_VAR2
  [ -z "$ENV_VAR2" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "ENV_VAR: $ENV_VAR"
  [ -z "$ENV_VAR" ]
  echo "ENV_VAR2: $ENV_VAR2"
  [ -z "$ENV_VAR2" ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to prefix environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "ENV_VAR<=PRE" >> "$OMNI_ENV"
        echo "ENV_VAR2<=PRE2" >> "$OMNI_ENV"
EOF

  export ENV_VAR="VALUE"
  [ "$ENV_VAR" = "VALUE" ]
  unset ENV_VAR2
  [ -z "$ENV_VAR2" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "ENV_VAR: $ENV_VAR"
  [ "$ENV_VAR" = "PREVALUE" ]
  echo "ENV_VAR2: $ENV_VAR"
  [ "$ENV_VAR2" = "PRE2" ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to suffix environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "ENV_VAR>=POST" >> "$OMNI_ENV"
        echo "ENV_VAR2>=POST2" >> "$OMNI_ENV"
EOF

  export ENV_VAR="VALUE"
  [ "$ENV_VAR" = "VALUE" ]
  unset ENV_VAR2
  [ -z "$ENV_VAR2" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "ENV_VAR: $ENV_VAR"
  [ "$ENV_VAR" = "VALUEPOST" ]
  echo "ENV_VAR2: $ENV_VAR"
  [ "$ENV_VAR2" = "POST2" ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to append to a path-like environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "ENV_VAR>>=newval" >> "$OMNI_ENV"
        echo "ENV_VAR2>>=newval2" >> "$OMNI_ENV"
EOF

  export ENV_VAR="val1:val2"
  [ "$ENV_VAR" = "val1:val2" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "ENV_VAR: $ENV_VAR"
  [ "$ENV_VAR" = "val1:val2:newval" ]
  echo "ENV_VAR2: $ENV_VAR2"
  [ "$ENV_VAR2" = "newval2" ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to prepend to a path-like environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "ENV_VAR<<=newval" >> "$OMNI_ENV"
        echo "ENV_VAR2<<=newval2" >> "$OMNI_ENV"
EOF

  export ENV_VAR="val1:val2"
  [ "$ENV_VAR" = "val1:val2" ]
  unset ENV_VAR2
  [ -z "$ENV_VAR2" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "ENV_VAR: $ENV_VAR"
  [ "$ENV_VAR" = "newval:val1:val2" ]
  echo "ENV_VAR2: $ENV_VAR2"
  [ "$ENV_VAR2" = "newval2" ]
}

# bats test_tags=omni:up,omni:up:custom
@test "omni up custom operation should allow to remove from a path-like environment variables" {
  cat > .omni.yaml <<'EOF'
up:
  - custom:
      name: "Custom Operation"
      meet: |
        echo "BEGIN_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "MID_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "END_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "ONLY_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "NOVAL_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "EMPTY_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "UNSET_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "MULTI_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "MULTI_REPEAT_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "MULTI_REPEAT_ENV_VAR-=oldval" >> "$OMNI_ENV"
        echo "MULTI_REPEAT_ENV_VAR-=oldval" >> "$OMNI_ENV"
EOF

  export BEGIN_ENV_VAR="oldval:val1:val2"
  export MID_ENV_VAR="val1:oldval:val2"
  export END_ENV_VAR="val1:val2:oldval"
  export ONLY_ENV_VAR="oldval"
  export NOVAL_ENV_VAR="val1:val2"
  export EMPTY_ENV_VAR=""
  export MULTI_ENV_VAR="oldval:val1:oldval:val2:oldval"
  export MULTI_REPEAT_ENV_VAR="oldval:val1:oldval:val2:oldval"
  unset UNSET_ENV_VAR
  [ -z "$UNSET_ENV_VAR" ]

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Load the dynamic environment
  echo "DYNAMIC ENVIRONMENT -- BEGIN"
  eval "$(omni hook env --quiet | tee /dev/stderr)"
  echo "DYNAMIC ENVIRONMENT -- END"

  # Check the variable
  echo "BEGIN_ENV_VAR: $BEGIN_ENV_VAR"
  [ "$BEGIN_ENV_VAR" = "val1:val2" ]
  echo "MID_ENV_VAR: $MID_ENV_VAR"
  [ "$MID_ENV_VAR" = "val1:val2" ]
  echo "END_ENV_VAR: $END_ENV_VAR"
  [ "$END_ENV_VAR" = "val1:val2" ]
  echo "ONLY_ENV_VAR: $ONLY_ENV_VAR"
  [ -z "$ONLY_ENV_VAR" ]
  echo "NOVAL_ENV_VAR: $NOVAL_ENV_VAR"
  [ "$NOVAL_ENV_VAR" = "val1:val2" ]
  echo "EMPTY_ENV_VAR: $EMPTY_ENV_VAR"
  [ -z "$EMPTY_ENV_VAR" ]
  echo "UNSET_ENV_VAR: $UNSET_ENV_VAR"
  [ -z "$UNSET_ENV_VAR" ]
  echo "MULTI_ENV_VAR: $MULTI_ENV_VAR"
  [ "$MULTI_ENV_VAR" = "val1:oldval:val2:oldval" ]
  echo "MULTI_REPEAT_ENV_VAR: $MULTI_REPEAT_ENV_VAR"
  [ "$MULTI_REPEAT_ENV_VAR" = "val1:val2" ]
}

