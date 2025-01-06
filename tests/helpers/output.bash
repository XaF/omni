#!/usr/bin/env bats

sanitize_output() {
  local output="$1"
  shift

  local skip_lines=0
  local reading_args=true
  while [[ "$reading_args" = true ]]; do
    case "$1" in
      skip_lines=*)
        skip_lines=${1#skip_lines=}
        shift
        ;;
      *)
        reading_args=false
        ;;
    esac
  done

  # Remove the first line so we can avoid the version number in the comparison
  if [ "$skip_lines" -gt 0 ]; then
    output=$(echo "$output" | tail -n +$((skip_lines + 1)))
  fi

  # Replace all references to the test directories
  # echo "$BATS_TEST_FILENAME" # The current test file
  local real_test_filename="$(cd -P "$(dirname "$BATS_TEST_FILENAME")" && pwd)/$(basename "$BATS_TEST_FILENAME")"
  output=$(echo "$output" | perl -pe "s|${real_test_filename}|<BATS_TEST_FILENAME>|g")
  output=$(echo "$output" | perl -pe "s|${BATS_TEST_FILENAME}|<BATS_TEST_FILENAME>|g")
  # echo "$BATS_TEST_TMPDIR" # Only available to the current test
  local real_test_tmpdir="$(cd -P "$BATS_TEST_TMPDIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_test_tmpdir}|<BATS_TEST_TMPDIR>|g")
  output=$(echo "$output" | perl -pe "s|${BATS_TEST_TMPDIR}|<BATS_TEST_TMPDIR>|g")
  # echo "$BATS_FILE_TMPDIR" # Shared with the whole file
  local real_file_tmpdir="$(cd -P "$BATS_FILE_TMPDIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_file_tmpdir}|<BATS_FILE_TMPDIR>|g")
  output=$(echo "$output" | perl -pe "s|${BATS_FILE_TMPDIR}|<BATS_FILE_TMPDIR>|g")
  # echo "$BATS_SUITE_TMPDIR" # Shared with the whole suite
  local real_suite_tmpdir="$(cd -P "$BATS_SUITE_TMPDIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_suite_tmpdir}|<BATS_SUITE_TMPDIR>|g")
  output=$(echo "$output" | perl -pe "s|${BATS_SUITE_TMPDIR}|<BATS_SUITE_TMPDIR>|g")
  # echo "$BATS_RUN_TMPDIR" # The run directory
  local real_run_tmpdir="$(cd -P "$BATS_RUN_TMPDIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_run_tmpdir}|<BATS_RUN_TMPDIR>|g")
  output=$(echo "$output" | perl -pe "s|${BATS_RUN_TMPDIR}|<BATS_RUN_TMPDIR>|g")

  # Replace references to the temp directory
  local real_tmpdir="$(cd -P "$TMPDIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_tmpdir}|<TMPDIR>|g")
  output=$(echo "$output" | perl -pe "s|${TMPDIR}|<TMPDIR>|g")

  # Replace references to the fixtures directory
  local real_fixtures_dir="$(cd -P "$FIXTURES_DIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_fixtures_dir}|<FIXTURES_DIR>|g")
  output=$(echo "$output" | perl -pe "s|${FIXTURES_DIR}|<FIXTURES_DIR>|g")

  # Replace references to the project directory
  local real_project_dir="$(cd -P "$PROJECT_DIR" && pwd)"
  output=$(echo "$output" | perl -pe "s|${real_project_dir}|<PROJECT_DIR>|g")
  output=$(echo "$output" | perl -pe "s|${PROJECT_DIR}|<PROJECT_DIR>|g")

  echo "$output"
}

validate_test_output() {
  local fixture_file="$1"
  shift

  local exit_code=0
  local skip_lines=0
  local reading_args=true
  while [[ "$reading_args" = true ]]; do
    case "$1" in
      exit_code=*)
        exit_code=${1#exit_code=}
        shift
        ;;
      skip_lines=*)
        skip_lines=${1#skip_lines=}
        shift
        ;;
      *)
        reading_args=false
        ;;
    esac
  done

  # Handle the fixtures
  fixture_file="${FIXTURES_DIR}/${fixture_file}"
  if [ "$GENERATE_FIXTURES" = "true" ]; then
    run mkdir -p "$(dirname "$fixture_file")"
    [ "$status" -eq 0 ]
    run "$@" 3>&-
    [ "$status" -eq "$exit_code" ]
    output=$(sanitize_output "$output" skip_lines="$skip_lines")
    echo "$output" >"$fixture_file"
    return 0
  fi
  expected=$(cat "$fixture_file")

  # Run the test
  run "$@" 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq "$exit_code" ]

  output=$(sanitize_output "$output" skip_lines="$skip_lines")

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat "-$CAT_OPTS" 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}
