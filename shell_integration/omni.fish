#!/usr/bin/env fish

# Setup the environment for omni, to make sure
# we have the required environment variables setup
function find_omnidir
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
        echo -e (set_color 96) "omni:" (set_color 31) "failed to find omni directory, please set OMNI_GIT to your workspace or OMNIDIR to the omni directory" (set_color normal) >&2
        return 1
    end

    if test -n "$OMNIDIR"
        if not contains -- "$OMNIPATH" "$OMNIDIR/cmd"
            # OMNIPATH is the list of directories in which omni will look
            # for commands; omni will make sure to add its own cmd directory
            # to the list, so that it can find the default commands, but
            # you can override this by setting OMNIPATH to a different path
            # or by adding any path you want to the list. Note that the
            # paths are separated by a colon (:), just like the PATH variable,
            # and that they are considered in order
            set -gx OMNIPATH "$OMNIPATH" "$OMNIDIR/cmd"
        end

        if not contains -- $PATH "$OMNIDIR/bin"
            set -gx PATH "$PATH" "$OMNIDIR/bin"
        end
    end
end

# Run the `find_omnidir` call when being sourced
find_omnidir

# Make sure that rbenv has been loaded properly, as it is a dependency of omni;
# this will also allow that if the integration is properly loaded in the shell,
# then the user will be able to use rbenv right away.
function omni_import_rbenv
    # Try and add rbenv to the path if not already present
    if not command -v rbenv >/dev/null
        set -l rbenv_paths
        command -v brew >/dev/null; and set rbenv_paths (brew --prefix)/bin $rbenv_paths
        set rbenv_paths $HOME/.rbenv/bin $rbenv_paths

        for rbenv_path in $rbenv_paths
            if test -d $rbenv_path; and not contains -- $PATH $rbenv_path; and test -x $rbenv_path/rbenv
                set -gx PATH $rbenv_path $PATH
                break
            end
        end

        set -e rbenv_paths
    end

    # Initialize rbenv if not already initialized
    if type rbenv 2>/dev/null | head -n1 | grep -q "function"; or test -z "$RBENV_SHELL"
        eval (rbenv init - fish | psub)
    end
end

# Run omni_import_rbenv function
omni_import_rbenv
functions -e omni_import_rbenv

# Make sure goenv has been loaded properly, as it is used with omni up;
# this will also allow that if the integration is properly loaded in the shell,
# then the user will be able to use goenv right away.
function omni_import_goenv
    if not command -v goenv >/dev/null
        set -l goenv_paths
        # command -v brew >/dev/null; and set goenv_paths (brew --prefix)/bin $goenv_paths
        set goenv_paths $HOME/.goenv/bin $goenv_paths

        for goenv_path in $goenv_paths
            if test -d $goenv_path; and not contains -- $PATH $goenv_path; and test -x $goenv_path/goenv
                set -gx PATH $goenv_path $PATH
                break
            end
        end
    end

    if command -v goenv >/dev/null
        # Add the shims - We need to force them at the beginning of the path in case there
        # is a homebrew version of go installed which would take preference over the shims
        # if the homebrew path is before the shims path
        set goenv_shims $HOME/.goenv/shims
        if test -d $goenv_shims; and not contains -- $PATH $goenv_shims
            set -gx PATH $goenv_shims $PATH
        end

        # Initialize goenv if not already initialized
        if type goenv 2>/dev/null | head -n1 | grep -q "function"; or test -z "$GOENV_SHELL"
            eval (goenv init - fish | psub)
        end
    end
end

# Run omni_import_goenv function
omni_import_goenv
functions -e omni_import_goenv

function omni
    # Find the OMNIDIR
    find_omnidir; or return 1

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
    string split ' ' -- $opts
end

complete -c omni -n "_omni_complete_fish" -f
