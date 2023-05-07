#!/usr/bin/env bash

# Identify location of this current script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Check if we are currently in the omni git repository
in_omni_dir=false
if [[ -d "$SCRIPT_DIR/.git" ]] && [[ -d "$SCRIPT_DIR/shell_integration" ]] && [[ -d "$SCRIPT_DIR/bin" ]]; then
        in_omni_dir=true
fi

function search_config() {
        local param=$1

        local config_files=(
              "${HOME}/.omni"
              "${HOME}/.omni.yaml"
              "${HOME}/.config/omni"
              "${HOME}/.config/omni.yaml"
              "${OMNI_CONFIG}"
        )
        for file in "${config_files[@]}"; do
                # If file does not exist or is not readable, go to next file
                ([[ -f "$file" ]] && [[ -r "$file" ]]) || continue

                # Try and find if there is a line following the format 'param: value'
                # in the file, we want the lookup to be compatible both for macos and linux, so
                # we choose the right command line for that research, knowing that even on macos
                # someone might have installed gnu grep
                local matching_line=$(grep -E "^${param}:" "$file")
                [[ -n "$matching_line" ]] || continue

                # Use awk to extract the parameter value, and remove the potential quotes, single or double, around it
                local matching_value=$(echo "$matching_line" | \
                        sed -E "s/^${param}:\s*//" | \
                        sed -E 's/^"(.*)"/\1/' | \
                        sed -E "s/^'(.*)'/\1/")
                [[ -n "$matching_value" ]] || continue

                # If we found a value, just return it
                echo $matching_value
                break
        done
}

function query_omni_git() {
        echo -en >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m What is your git base directory? \e[90m(default: ${HOME}/git)\e[0m "
        read git_base_dir
        git_base_dir="${git_base_dir:-${HOME}/git}"
        git_base_dir="$(eval "echo $git_base_dir")"
        echo "$git_base_dir"
}

function query_repo_path_format() {
        echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m Which repository path format do you wish to use?"

        local PS3="Format index: "
        select repo_path_format in "%{host}/%{org}/%{repo}" "%{org}/%{repo}" "%{repo}" "other (custom)"; do
                if [[ "$repo_path_format" == "other (custom)" ]]; then
                        echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m Enter the format to use for repositories"
                        echo -e >&2 "\e[90m  %{host}    registry (e.g. github.com)\e[0m"
                        echo -e >&2 "\e[90m  %{org}     repository owner (e.g. XaF)\e[0m"
                        echo -e >&2 "\e[90m  %{repo}    repository name (e.g. omni)\e[0m"
                        echo -en >&2 "Format: "
                        read repo_path_format
                        break
                elif [[ "$repo_path_format" != "" ]]; then
                        break
                fi
        done

        if [[ -z "$repo_path_format" ]]; then
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m No repository format provided  \e[91m[FAILED]\e[0"
                exit 1
        fi

        # Write repo path format to configuration file (at default location)
        local config_file="${HOME}/.config/omni.yaml"
        mkdir -p "$(dirname "$config_file")"
        echo "repo_path_format: \"${repo_path_format}\"" >> "${config_file}"
        echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m Saved repository path format to ${config_file}  \e[92m[OK]\e[0m"

        echo $repo_path_format
}

function git_clone() {
        local repo_path_format
        local clone_location
        local repo_location="git@github.com:XaF/omni.git"

        # We need the base location for git repositories
        [[ -n "$OMNI_GIT" ]] || OMNI_GIT=$(query_omni_git)

        # We need the format the user wants for repo paths
        repo_path_format=$(search_config "repo_path_format")
        [[ -n "$repo_path_format" ]] || repo_path_format=$(query_repo_path_format)

        # We can then convert that to what would be the clone
        # location of the omni repository, since we know the
        # different parts of it
        clone_location="${OMNI_GIT}/"
        clone_location+=$(echo "$repo_path_format" | \
                sed -E "s/%\{host\}/github.com/" | \
                sed -E "s/%\{org\}/XaF/" | \
                sed -E "s/%\{repo\}/omni/")

        # If the expected clone location already exists, we raise
        # an error since we won't be able to clone the omni repository
        # there
        if [[ -e "$clone_location" ]]; then
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m $clone_location already exists  \e[91m[FAILED]\e[0m"
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m if this is a clone of omni, either remove it"
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m or run install.sh from there"
                exit 1
        fi

        # We make sure the path up to the directory in which we want to
        # clone do exist, or git clone will complain
        mkdir -p "$(dirname "$clone_location")"

        # Then we can clone the repository
        echo -e >&2 "\e[90m$ git clone \"${repo_location}\" \"${clone_location}\" --depth 1\e[0m"
        git clone "${repo_location}" "${clone_location}" --depth 1
        if [ $? -ne 0 ]; then
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m clone omni repository in $clone_location  \e[91m[FAILED]\e[0m"
                exit 1  # Fail fast
        else
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m clone omni repository in $clone_location  \e[92m[OK]\e[0m"
        fi

        # Finally, we echo the clone location so the caller can use it
        echo $clone_location
}

if ! $in_omni_dir; then
        clone_location=$(git_clone)
        if [[ -z "$clone_location" ]]; then
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m weird error, clone_location is empty but shouldn't be  \e[91m[FAILED]\e[0m"
                exit 1
        fi


        # Now call the install script from the newly cloned repo
        $clone_location/install.sh

        # And exit with the exit code of that command
        exit $?
fi

CURRENT_SHELL=$(basename -- "$(ps -p $PPID -o command=)" | sed 's/^-//')

function setup_shell_integration() {
        local shell="$1"
        local skip_confirmation=$([[ "$CURRENT_SHELL" == "$shell" ]] && echo "y" || echo "n")
        local setup_shell="N"
        local rc_file=""

        if [[ "$skip_confirmation" != "y" ]] && [[ "$skip_confirmation" != "Y" ]]; then
                echo -en >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m Do you want to setup the $shell integration? \e[90m[y/N]\e[0m "
                read setup_shell
                [[ "$setup_shell" == "y" ]] || [[ "$setup_shell" == "Y" ]] || return 0
        fi

        echo -en >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m Location of the .${shell}rc file to edit? \e[90m(default: ${HOME}/.${shell}rc)\e[0m "
        read rc_file
        rc_file="${rc_file:-${HOME}/.${shell}rc}"
        rc_file="$(eval "echo $rc_file")"

        echo "[[ -x \"${SCRIPT_DIR}/shell_integration/omni.${shell}\" ]] && source \"${SCRIPT_DIR}/shell_integration/omni.${shell}\"" >> "$rc_file"
        if [ $? -ne 0 ]; then
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m setup $shell integration in $rc_file  \e[91m[FAILED]\e[0m"
        else
                echo -e >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m setup $shell integration in $rc_file  \e[92m[OK]\e[0m"
        fi
}

SUPPORTED_SHELLS=("bash" "zsh")
# Setup firt the integration for the current shell
for shell in "${SUPPORTED_SHELLS[@]}"; do
        [[ "$CURRENT_SHELL" == "$shell" ]] && setup_shell_integration "$shell"
done
# Then offer to setup the other shell integrations
for shell in "${SUPPORTED_SHELLS[@]}"; do
        [[ "$CURRENT_SHELL" == "$shell" ]] || setup_shell_integration "$shell"
done
