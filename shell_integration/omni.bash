#!/usr/bin/env bash

# Setup the environment for omni, to make sure
# we have the required environment variables setup
if [[ -z "$OMNIDIR" ]]; then
	# OMNIDIR is the directory in which omni is located; by default
	# it will assume the path where omni would clone itself, but
	# if OMNI_GIT is setup, it will try to use that path instead;
	# you can override this by setting OMNIDIR to a different path
	export OMNIDIR="${OMNI_GIT:-${HOME}/git}/github.org/XaF/omni"

	altdir="${OMNI_GIT:-${HOME}/git}/omni"
	if [ ! -d "$OMNIDIR" ] && [ -d "$altdir" ]; then
		export OMNIDIR="$altdir"
	fi
	unset altdir
fi
if [[ "$OMNIPATH" != *"${OMNIDIR}/cmd"* ]]; then
	# OMNIPATH is the list of directories in which omni will look
	# for commands; omni will make sure to add its own cmd directory
	# to the list, so that it can find the default commands, but
	# you can override this by setting OMNIPATH to a different path
	# or by adding a any path you want to the list. Note that the
	# paths are separated by a colon (:), just like the PATH variable,
	# and that they are considered in order
	export OMNIPATH="${OMNIPATH:+$OMNIPATH:}${OMNIDIR}/cmd"
fi

# This function is used to run the omni command, and then operate on
# the requested shell changes from the command (changing current
# working directory, environment, etc.); this is why we require using
# a shell function for this, instead of simply calling the omni
# command from the path
function omni() {
	# Prepare the environment for omni
	export OMNI_UUID=$(uuidgen)
	local tmpdir=${TMPDIR:-/tmp}
	OMNI_FILE_PREFIX="omni_${OMNI_UUID}"
	export OMNI_CMD_FILE="${tmpdir}/${OMNI_FILE_PREFIX}.cmd"
	local omnidir="${OMNIDIR:-${OMNI_GIT:-${HOME}/git}/github.org/XaF/omni}"

	# Run the command
	"${omnidir}/omni.rb" "$@"
	EXIT_CODE=$?

	# Check if OMNI_CMD_FILE exists, and if it does, run the commands
	# inside without a subshell, so that the commands can modify the
	# environment of the current shell, and then delete the file
	if [[ -f $OMNI_CMD_FILE ]] && [[ "$EXIT_CODE" == "0" ]]; then
		while IFS= read -r cmd; do
			eval $cmd
			EXIT_CODE=$?
			if [[ "$EXIT_CODE" != "0" ]]; then
				echo -e "\033[96momni:\033[0m \033[31mcommand failed:\033[0m $cmd \033[90m(exit: $EXIT_CODE)\033[0m"
				break
			fi
		done < $OMNI_CMD_FILE
	fi

	# Delete the files, we do that with '&' so there's no delay to return
	# to the prompt, and within a subshell so that monitor mode (set +m)
	# is disabled for that command, allowing to hide the '[x] Done' message
	(find "${TMPDIR}/" -name "${OMNI_FILE_PREFIX}*" -exec rm {} \; >/dev/null 2>&1 &)

	# Unset the environment variables
	unset OMNI_UUID
	unset OMNI_FILE_PREFIX
	unset OMNI_CMD_FILE
	unset OMNI_HELPERS_FILE

	# Return the exit code of the command
	return $EXIT_CODE
}

# Setup autocompletion for omni
if [[ -z "${ZSH_VERSION-}" ]]; then
	_omni_complete_bash() {
		local cur prev opts
		COMPREPLY=()
		cur="${COMP_WORDS[COMP_CWORD]}"
		prev="${COMP_WORDS[COMP_CWORD-1]}"
		opts=$(\
			COMP_CWORD=$COMP_CWORD \
			COMP_TYPE=$COMP_TYPE \
			omni --complete ${COMP_WORDS[@]:1:$COMP_CWORD})
		COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
		return 0
	}

	# complete -o nospace -o default -o bashdefault -C 'omni --complete' omni
	complete -F _omni_complete_bash omni
elif command -v compdef >/dev/null; then
	_omni_complete_zsh() {
		opts=$(\
		    COMP_CWORD=$((CURRENT-1)) \
		    COMP_TYPE=$compstate[quote] \
		    omni --complete ${words[2,CURRENT]})
		reply=($(echo $opts))
		compadd "$reply[@]"
	}

	compdef _omni_complete_zsh omni
fi
