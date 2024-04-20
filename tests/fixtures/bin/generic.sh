#!/usr/bin/env bash
#
# This is a generic fixture binary; it is used to look
# for the expected commands in the test suite. It works
# by using a reference directory that contains files
# specially crafted to indicate the expected commands.
#
# The directory is expected to contain files named
# following the pattern '<binary>_<order>', where
# <binary> is the binary name of the command that is
# expected to be executed and <order> is a number that
# indicates the order in which the command is expected
# to be executed.
#
# The first line of that file is expected to contain the
# arguments that are expected to be passed to the command.
#
# The second line of that file is expected to contain the
# exit code that the command is expected to return.
#
# Any line after the fourth line is the response that the
# command is expected to return. To indicate if it needs
# to be written to stdout or stderr, the line should
# start with either 'stdout:' or 'stderr:'. If neither
# is present, it is assumed that the response is expected
# to be written to stdout.
#
# In case of issue, the script will exit with exit code
# 127 and write the error message to stderr.

set -o pipefail

# Get the binary name
binary=$(basename "$0")

# If the binary name is this file name, exit on error
if [ "$binary" = "generic.sh" ]; then
    echo "This script is not meant to be executed directly" >&2
    exit 127
fi

# Get the reference directory
commands="${HOME}/.commands"

# Write to the command log that we have been called
actual_args=("$@")
call_dump="${binary} $(for arg in "${actual_args[@]}"; do echo -n "\"$arg\" "; done)"
echo "$call_dump" >> "${commands}/called.log"

# Make sure the commands directory exists
mkdir -p "${commands}"

# Get the current position
position_file="${commands}/${binary}_position"
position=$(cat "$position_file" 2>/dev/null)
if [ -z "$position" ]; then
    position=0
fi

# Get the expected command definition
expected=$(cat ${commands}/${binary}_${position} 2>/dev/null)
if [ -z "$expected" ]; then
    echo "${binary}: command: $@" >&2
    echo "${binary}: no more commands expected" >&2
    echo "$call_dump" >> "${commands}/unexpected.log"
    exit 127
fi

# Get the expected args, doing shell parsing to allow for
expected_args_raw=$(echo "$expected" | head -n 1)
if [ -z "$expected_args_raw" ]; then
    # Allow for empty arguments
    expected_args=()
else
    # Evaluate the arguments to allow for shell parsing
    eval "expected_args=($expected_args_raw)"
fi

# Get the expected exit code
exit_code=$(echo "$expected" | head -n 2 | tail -n 1)
if [ -z "$exit_code" ]; then
    echo "${binary}: command: $@" >&2
    echo "${binary} (#${position}): no exit code defined" >&2
    echo "$call_dump" >> "${commands}/unexpected.log"
    exit 127
fi
if ! [[ "$exit_code" =~ ^[0-9]+$ ]]; then
    echo "${binary}: command: $@" >&2
    echo "${binary} (#${position}): invalid exit code: $exit_code" >&2
    echo "$call_dump" >> "${commands}/unexpected.log"
    exit 127
fi

# Get the expected response
response=()
while IFS= read -r line; do
    response+=("$line")
done < <(echo "$expected" | tail -n +4)

# If the first line of the response is '#omni-test-bash-function', then
# we want to eval the response as a bash function, and then check that
# the process_response function exists
unset process_response
if [ "${response[0]}" = "#omni-test-bash-function" ]; then
    eval "$(printf '%s\n' "${response[@]}")"
    if ! declare -f process_response > /dev/null; then
	echo "${binary}: command: $@" >&2
	echo "${binary} (#${position}): response is a bash function, but process_response is not defined" >&2
	echo "$call_dump" >> "${commands}/unexpected.log"
	exit 127
    fi
fi

# Compare the expected arguments with the actual arguments
if [ "${#actual_args[@]}" -ne "${#expected_args[@]}" ]; then
    echo "${binary}: command: $@" >&2
    echo "${binary} (#${position}): expected ${#expected_args[@]} arguments, got ${#actual_args[@]}" >&2
    echo "${binary} (#${position}): expected arguments: ${expected_args[@]}" >&2
    echo "$call_dump" >> "${commands}/unexpected.log"
    exit 127
fi

for i in $(seq 0 $((${#expected_args[@]} - 1))); do
    current_expected="${expected_args[$i]}"
    current_actual="${actual_args[$i]}"

    # Check if starts with regex: if so, check if it matches using
    # the regex; otherwise, check if the strings are equal
    matches=true
    if [[ "$current_expected" =~ ^regex:(.*)$ ]]; then
	if [[ ! "$current_actual" =~ ${BASH_REMATCH[1]} ]]; then
	    matches=false
	fi
    elif [ "$current_expected" != "$current_actual" ]; then
	matches=false
    fi

    if [ "$matches" = false ]; then
	echo "${binary}: command: $@" >&2
	echo "${binary} (#${position}): argument mismatch at position $i ($current_expected != $current_actual)" >&2
	echo "${binary} (#${position}): expected arguments: ${expected_args[@]}" >&2
	echo "$call_dump" >> "${commands}/unexpected.log"
	exit 127
    fi
done

# If we get here, the arguments match, so we can decrement the required file
required_file="${commands}/${binary}_${position}_required"
if [[ -f "$required_file" ]]; then
    required=$(($(cat "$required_file") - 1))
    if [ "$required" -eq 0 ]; then
	rm "$required_file"
    else
	echo $required > "$required_file"
    fi
fi

# If we get here, the arguments match; increment the position
position=$((position + 1))
echo $position > "$position_file"

# Write the expected response
if declare -f process_response > /dev/null; then
    process_response "${actual_args[@]}"
else
    for line in "${response[@]}"; do
	if [ "${line:0:7}" = "stdout:" ]; then
	    echo "${line:7}"
	elif [ "${line:0:7}" = "stderr:" ]; then
	    echo "${line:7}" >&2
	else
	    echo "$line"
	fi
    done
fi

# Exit with the expected exit code
exit $exit_code
