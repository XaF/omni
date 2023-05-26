#!/usr/bin/env fish

# Setup the environment for omni, to make sure
# we have the required environment variables setup
function set_omni_env
    if test -z "$OMNIDIR"
        # OMNIDIR is the directory in which omni is located; by default
        # it will assume the path where omni would clone itself, but
        # if OMNI_GIT is set up, it will try to use that path instead;
        # you can override this by setting OMNIDIR to a different path
        set -l lookup_omnidir
        set -l lookup_srcpath
        if test -n "$OMNI_GIT"
            set lookup_srcpath "$OMNI_GIT"
        else
            test -d "$HOME/git" && set lookup_srcpath "$lookup_srcpath" "$HOME/git"
            test -n "$GOPATH" && test -d "$GOPATH/src" && set lookup_srcpath "$lookup_srcpath" "$GOPATH/src"
        end
        for srcpath in $lookup_srcpath
            set lookup_omnidir "$lookup_omnidir" "$srcpath/github.com/XaF/omni" "$srcpath/XaF/omni" "$srcpath/omni"
        end
        for lookup in $lookup_omnidir
            if test -d "$lookup"
                set -gx OMNIDIR "$lookup"
                set -gx OMNIDIR_LOCATED "true"
                break
            end
        end
    end

    if test -z "$OMNIDIR"
        set -l suggestions
        test -z "$OMNI_GIT"; and set suggestions $suggestions "OMNI_GIT to your workspace"
        set suggestions $suggestions "OMNIDIR to the omni directory"
        echo -e "\033[96momni:\033[31m failed to find omni directory, please set" (string join ' or ' -- $suggestions) "\033[0m" >&2
        return 1
    end

    if test -n "$OMNIDIR"
        if not contains -- "$OMNIDIR/cmd" $OMNIPATH
            # OMNIPATH is the list of directories in which omni will look
            # for commands; omni will make sure to add its own cmd directory
            # to the list, so that it can find the default commands, but
            # you can override this by setting OMNIPATH to a different path
            # or by adding any path you want to the list. Note that the
            # paths are separated by a colon (:), just like the PATH variable,
            # and that they are considered in order
            set -gx OMNIPATH "$OMNIPATH" "$OMNIDIR/cmd"
        end

        if not contains -- "$OMNIDIR/bin" $PATH
            set -gx PATH "$PATH" "$OMNIDIR/bin"
        end
    end

    if test -z "$OMNI_DATA_HOME"
        set xdg_data_home "$XDG_DATA_HOME"
        if test -z "$xdg_data_home"; or not string match -r '^/' "$xdg_data_home" >/dev/null
            set xdg_data_home "$HOME/.local/share"
        end
        set -gx OMNI_DATA_HOME "$xdg_data_home/omni"
    end
end

# Run the `set_omni_env` call when being sourced
set_omni_env

# Make sure asdf has been loaded properly, as it is used with omni up;
# this will also allow that if the integration is properly loaded in the shell,
# then the user will be able to use asdf right away.
function omni_import_asdf
    set -xg ASDF_DATA_DIR "$OMNI_DATA_HOME/asdf"
    source "$ASDF_DATA_DIR/asdf.fish"
end

function omni_import_shadowenv
    if not command -v shadowenv >/dev/null
        omni_import_asdf
        return
    end

    if type __shadowenv_hook >/dev/null 2>&1
        return
    end

    shadowenv init fish | source
end

omni_import_shadowenv
functions -e omni_import_shadowenv
functions -e omni_import_asdf

function omni
    # Find the OMNIDIR
    set_omni_env; or return 1

    # Prepare the environment for omni
    set -gx OMNI_UUID (uuidgen)
    set -q TMPDIR; and set -l tmpdir $TMPDIR; or set -l tmpdir /tmp
    set -gx OMNI_FILE_PREFIX "omni_$OMNI_UUID"
    set -gx OMNI_CMD_FILE "$tmpdir/$OMNI_FILE_PREFIX.cmd"

    # Run the command
    "$OMNIDIR/bin/omni" $argv
    set EXIT_CODE $status

    # Check if OMNI_CMD_FILE exists, and if it does, run the commands
    # inside without a subshell, so that the commands can modify the
    # environment of the current shell, and then delete the file
    if test -f $OMNI_CMD_FILE; and test $EXIT_CODE -eq 0
	cat $OMNI_CMD_FILE | while read -l cmd
            eval $cmd
            set EXIT_CODE $status
            if test $EXIT_CODE -ne 0
                echo -e "\033[96momni:\033[0m \033[31mcommand failed:\033[0m $cmd \033[90m(exit: $EXIT_CODE)\033[0m"
                break
            end
        end
    end

    # Delete the files using background process to avoid delay
    # in returning to the prompt, and within a subshell to disable
    # monitor mode (set +m) and hide the '[x] Done' message
    commandline -M find "$tmpdir/" -name "$OMNI_FILE_PREFIX*" -exec echo rm {} \; >/dev/null 2>&1 &

    # Unset the environment variables
    set -e OMNI_UUID
    set -e OMNI_FILE_PREFIX
    set -e OMNI_CMD_FILE
    set -e OMNI_HELPERS_FILE

    # Return the exit code of the command
    return $EXIT_CODE
end

# Setup autocompletion for omni
function _omni_complete_fish
    set -l cur (commandline -o)
    set -l prev (commandline -P)
    set -l cmdline (commandline --tokenize --cut-at-cursor)
    set -l cword (count $cmdline)

    # Remove the first element (command) from cmdline
    set -l args (echo $cmdline[2..-1])

    set -l opts (env COMP_CWORD=$cword omni --complete $args)
    string join \n -- $opts
end

complete -c omni -a "(_omni_complete_fish)" -f
