#!/usr/bin/env bash

# Identify location of this current script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

function print_logo() {
	cat >&2 <<EOF
  :?:.!YG#&@@@&#GJ~.
 7#@&#@@@&#BGB#&@@@#Y^
 ^&@@@@&?.     :~Y#@@@Y
.G@@&#@@&5^       .J@@@G.   OMNI
P@@@! 7B@@@P~       7@@@5
@@@P    !B@@@G~      G@@&     THE OMNIPOTENT
@@@P    !B@@@B!      G@@&        DEV TOOL
P@@@~ 7B@@@P~       !@@@5
.G@@&B@@&P~       .?@@@G.
 ^#@@@@&?.     .~J#@@@Y.
 7&@&#@@@&BGGGB&@@@#5^
  :?^.!YG#@@@@@#GY!.
EOF
}

function print_msg() { printf >&2 "\e[96momni:\e[0m \e[93minstall:\e[0m $@\n"; }
function print_ok() { print_msg "\e[92m[OK]\e[0m      $@"; }
function print_failed() { print_msg "\e[91m[FAILED]\e[0m  $@"; }
function print_pending() { print_msg "\e[90m[--]\e[0m      $@"; }
function print_query() { printf >&2 "$(print_msg "\e[90m[??]\e[0m      $*" 2>&1) "; }
function print_action() { print_msg "\e[90m[!!]\e[0m      $@"; }
function print_issue() { print_msg "\e[93m[~~]\e[0m      $@"; }

# Usage function
function usage() {
	echo -e >&2 "usage: $0 [options]"
	echo -e >&2 "  -h, --help			Show this help message"
	echo -e >&2 "  --repo-path-format		Format of the repository path, default is 'github.com/{user}/{repo}'"
	echo -e >&2 "  --git-dir			Directory where to clone the omni git repository, default is '~/.omni'"
	echo -e >&2 "  --bashrc			Setup bashrc. If a path is not provided, default is '~/.bashrc'"
	echo -e >&2 "  --zshrc			Setup zshrc. If a path is not provided, default is '~/.zshrc'"
	echo -e >&2 "  --no-interactive		Do not ask for confirmation before installing"
	exit $1
}

# Return invalid option error
function invalid_option() {
	print_msg "Unknown option '$1'"
	usage 1
}

# Parse long option and return bash code to set the variable
function parse_long_option() {
	local ARGNAME="$1"
	local VALUE_TYPE="$2" # optional, required, none
	local OPTARG="$3"
	local NEXTVAL="$4"

	local val=
	local opt=

	if [[ "$VALUE_TYPE" == "none" ]] && [[ "${OPTARG}" == *"="* ]]; then
		print_msg "Option '--${OPTARG}' does not take a value"
		echo "exit 1"
		usage 1
	elif [[ "${OPTARG}" == *"="* ]]; then
		val=${OPTARG#*=}
		opt=${OPTARG%=$val}
	elif [[ "${OPTARG}" != "$ARGNAME" ]]; then
		echo "exit 1"
		invalid_option "--${OPTARG}"
	elif [[ "$VALUE_TYPE" != "none" ]] && [[ "${NEXTVAL}" != "-"* ]]; then
		val="${NEXTVAL}";
		echo "OPTIND=\$((\$OPTIND + 1))"
		opt="${OPTARG}"
	else
		val=""
		opt="${OPTARG}"
	fi

	if [[ "$VALUE_TYPE" == "required" ]] && [[ -z "$val" ]]; then
		print_msg "Option '--${OPTARG}' requires an argument"
		echo "exit 1"
		usage 1
	fi

	echo "val=${val}"
	echo "opt=${opt}"
}

# Handle options in a way compatible with linux and macos
INTERACTIVE=${INTERACTIVE:-true}
SETUP_RBENV_PATH=${SETUP_RBENV_PATH:-false}
SETUP_RBENV_INTEGRATION=${SETUP_RBENV_INTEGRATION:-false}
SETUP_OMNI_GIT=${SETUP_OMNI_GIT:-false}
OMNI_GIT="${OMNI_GIT}"
while getopts -- ":h-:" optchar; do
	case "${optchar}" in
	-)
		case "${OPTARG}" in
		help*)
			eval "$(parse_long_option "help" "none" "${OPTARG}" "${!OPTIND}")"
			usage 0
			;;
		repo-path-format*)
			eval "$(parse_long_option "repo-path-format" "required" "${OPTARG}" "${!OPTIND}")"
			REPO_PATH_FORMAT="${val}"
			;;
		git-dir*)
			eval "$(parse_long_option "git-dir" "required" "${OPTARG}" "${!OPTIND}")"
			OMNI_GIT="${val}"
			;;
		bashrc*)
			eval "$(parse_long_option "bashrc" "optional" "${OPTARG}" "${!OPTIND}")"
			SETUP_BASHRC=true
			BASHRC_PATH="${val}"
			;;
		zshrc*)
			eval "$(parse_long_option "zshrc" "optional" "${OPTARG}" "${!OPTIND}")"
			SETUP_ZSHRC=true
			ZSHRC_PATH="${val}"
			;;
		no-interactive*)
			eval "$(parse_long_option "no-interactive" "none" "${OPTARG}" "${!OPTIND}")"
			INTERACTIVE=false
			;;
		*)
			invalid_option "--${OPTARG}"
			;;
		esac;;
	h)
		usage 0
		;;
	*)
		invalid_option "-${OPTARG}"
		;;
	esac
done

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
	if [[ "$INTERACTIVE" == "false" ]]; then
		print_failed "Missing git base directory, set OMNI_GIT environment variable or use --git-dir option"
		exit 1
	fi

	local default_git_dir="$HOME/git"
	if [[ ! -d "$default_git_dir" ]] && [[ -n "$GOPATH" ]] && [[ -d "${GOPATH}/src" ]]; then
		default_git_dir="${GOPATH}/src"
	fi

	print_query "What is your git base directory? \e[90m(default: ${default_git_dir})\e[0m "
	read git_base_dir
	git_base_dir="${git_base_dir:-${default_git_dir}}"
	git_base_dir="$(eval "echo $git_base_dir")"
	echo "$git_base_dir"
}

function query_repo_path_format() {
	if [[ -z "$REPO_PATH_FORMAT" ]]; then
		if [[ "$INTERACTIVE" == "false" ]]; then
			print_failed "Missing repo path format, use --repo-path-format option"
			exit 1
		fi

		print_query "Which repository path format do you wish to use?\n"

		local PS3="Format index: "
		select REPO_PATH_FORMAT in "%{host}/%{org}/%{repo}" "%{org}/%{repo}" "%{repo}" "other (custom)"; do
			if [[ "$REPO_PATH_FORMAT" == "other (custom)" ]]; then
				print_msg "Enter the format to use for repositories"
				echo -e >&2 "\e[90m  %{host}	registry (e.g. github.com)\e[0m"
				echo -e >&2 "\e[90m  %{org}	 repository owner (e.g. XaF)\e[0m"
				echo -e >&2 "\e[90m  %{repo}	repository name (e.g. omni)\e[0m"
				echo -en >&2 "Format: "
				read REPO_PATH_FORMAT
				break
			elif [[ "$REPO_PATH_FORMAT" != "" ]]; then
				break
			fi
		done
	fi

	if [[ -z "$REPO_PATH_FORMAT" ]]; then
		print_failed "No repository format provided"
		exit 1
	fi

	# Write repo path format to configuration file (at default location)
	local config_file="${HOME}/.config/omni.yaml"
	mkdir -p "$(dirname "$config_file")"
	echo "repo_path_format: \"${REPO_PATH_FORMAT}\"" >> "${config_file}"
	print_ok "Saved repository path format to ${config_file}"

	echo $REPO_PATH_FORMAT
}

function git_clone() {
	local repo_path_format
	local clone_location
	local repo_locations=(
	  "git@github.com:XaF/omni.git"
	  "https://github.com/XaF/omni.git"
	)

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
		# print_failed "$clone_location already exists, if this is a clone of omni, either remove it, or run $clone_location/install.sh"
		echo $clone_location
		exit 0
	fi

	# We make sure the path up to the directory in which we want to
	# clone do exist, or git clone will complain
	mkdir -p "$(dirname "$clone_location")"

	# Then we can clone the repository
	local cloned=false
	for repo_location in "${repo_locations[@]}"; do
	  echo -e >&2 "\e[90m$ git clone \"${repo_location}\" \"${clone_location}\" --depth 1\e[0m"
	  git clone "${repo_location}" "${clone_location}" --depth 1
	  if [ $? -ne 0 ]; then
		  print_failed "clone omni repository from ${repo_location} in $clone_location"
	  else
		  print_ok "clone omni repository in $clone_location"
		  cloned=true
		  break
	  fi
	done
	if [[ "$cloned" == "false" ]]; then
		print_failed "failed to clone omni repository"
		exit 1
	fi

	# Finally, we echo the clone location so the caller can use it
	echo $clone_location
}

if ! $in_omni_dir; then
	# We need the base location for git repositories
	if [[ -z "$OMNI_GIT" ]]; then
	  export SETUP_OMNI_GIT=true
	  export OMNI_GIT=$(query_omni_git)
	  [[ $? -eq 0 ]] || exit 1
	fi

	clone_location=$(git_clone)
	[[ $? -eq 0 ]] || exit 1
	if [[ -z "$clone_location" ]]; then
		print_failed "weird error, clone_location is empty but shouldn't be"
		exit 1
	fi

	# Now call the install script from the newly cloned repo
	$clone_location/install.sh "$@"

	# And exit with the exit code of that command
	exit $?
fi

print_logo

# Switch directory to the omni repository
cd "$SCRIPT_DIR"

function install_dependencies_packages() {
	local expect=("rbenv" "uuidgen")
	local missing=()

	# Check that the expected commands are found
	for cmd in "${expect[@]}"; do
		if command -v "$cmd" >/dev/null; then
			print_ok "$cmd found"
		else
			print_issue "$cmd not found"
			missing+=("$cmd")
		fi
	done

	# If packages is empty, we can return early
	if [[ ${#missing[@]} -eq 0 ]]; then
		return
	fi

	if command -v brew >/dev/null; then
		print_ok "brew found"
		echo -e >&2 "\e[90m$ brew install ${missing[@]}\e[0m"
		brew install "${missing[@]}" || exit 1
	elif command -v apt-get >/dev/null; then
		print_ok "apt-get found"

		echo -e >&2 "\e[33m[sudo]\e[0m \e[90m$ apt-get update\e[0m"
		sudo DEBIAN_FRONTEND=noninteractive apt-get update || exit 1

		local rbenv_build=false
		local apt_packages=()
		if [[ " ${missing[@]} " =~ " rbenv " ]]; then
			if $(apt-cache search --names-only rbenv 2>/dev/null | grep -q ^rbenv); then
				apt_packages+=("rbenv")
			else
				rbenv_build=true
				apt_packages+=(
					"libssl-dev"
					"libreadline-dev"
					"zlib1g-dev"
					"autoconf"
					"bison"
					"build-essential"
					"libyaml-dev"
					"libreadline-dev"
					"libncurses5-dev"
					"libffi-dev"
					"libgdbm-dev"
				)
			fi
		fi
		if [[ " ${missing[@]} " =~ " uuidgen " ]]; then
			apt_packages+=("uuid-runtime")
		fi

		echo -e >&2 "\e[33m[sudo]\e[0m \e[90m$ apt-get --yes --no-install-recommends install ${apt_packages[@]}\e[0m"
		sudo DEBIAN_FRONTEND=noninteractive apt-get --yes install "${apt_packages[@]}" || exit 1

		if [[ "$rbenv_build" == "true" ]]; then
			echo -e >&2 "\e[90m$ curl -fsSL https://github.com/rbenv/rbenv-installer/raw/HEAD/bin/rbenv-installer | bash\e[0m"
			curl -fsSL https://github.com/rbenv/rbenv-installer/raw/HEAD/bin/rbenv-installer | bash || exit 1
		fi
	else
		print_issue "No package manager found"
		if [[ "$INTERACTIVE" == "true" ]]; then
			print_query "Please install the following dependencies manually:\n$(printf " - %s\n" "${packages[@]}")\nPress enter when ready to pursue."
			read
		fi
	fi

	# Check that the missing commands are now available
	for cmd in "${missing[@]}"; do
		if command -v "$cmd" >/dev/null; then
			print_ok "$cmd found"
		else
			print_failed "$cmd still not found"
			exit 1
		fi
	done
}

function install_dependencies_ruby() {
	# We then make sure the right ruby version is installed and being used from the repo
	local ruby_version=$(<"${SCRIPT_DIR}/.ruby-version")
	if (cd "$SCRIPT_DIR" && rbenv version | cut -d' ' -f1 | grep -q "$ruby_version"); then
		print_ok "ruby $ruby_version found"
	else
		if [[ ! -d "$HOME/.rbenv/plugins/rvm-download" ]]; then
			# Get rvm-download so that the installation can be faster
			print_pending "Installing rvm-download plugin for rbenv"
			git clone https://github.com/garnieretienne/rvm-download.git "$HOME/.rbenv/plugins/rvm-download" || exit 1
			print_ok "Installed rvm-download plugin for rbenv"
		fi

		{
			print_pending "Installing ruby $ruby_version from rvm sources"
			rbenv download $ruby_version && rbenv rehash
		} || {
			print_failed "Installing ruby $ruby_version from rvm sources"
			print_pending "Installing ruby $ruby_version from ruby-build"
			RUBY_CONFIGURE_OPTS=--disable-install-doc \
				rbenv install --verbose --skip-existing $ruby_version
		} || exit 1
		print_ok "Installed ruby $ruby_version"
	fi

	if ! (cd "$SCRIPT_DIR" && rbenv version | cut -d' ' -f1 | grep -q "$ruby_version"); then
		print_failed "ruby $ruby_version still not found"
		exit 1
	fi
}

function install_dependencies_bundler() {
	# We then check that bundler is installed, that should be automated, but just in case
	if command -v bundle >/dev/null; then
		print_ok "bundler found"
	else
		print_pending "Installing bundler"
		echo -e >&2 "\e[90m$ gem install bundler\e[0m"
		gem install bundler || exit 1
		print_ok "Installed bundler"
	fi

	if ! command -v bundle >/dev/null; then
		print_failed "bundler still not found"
		exit 1
	fi
}

function install_dependencies_gemfile() {
	# Finally, we can go into the repository and run the bundle install from there
	print_pending "Installing Gemfile dependencies"
	{
		cd "$SCRIPT_DIR"
		bundle config set path 'vendor/bundle'
		bundle install
	} || exit 1
	print_ok "Installed Gemfile dependencies"
}

function install_dependencies() {
	# rbenv might be installed in the user's home, so we add it to the path to make
	# sure that it's found even if it's not in the user's configured path
	if [[ ! ":$PATH:" =~ ":$HOME/.rbenv/bin:" ]]; then
		SETUP_RBENV_PATH=true
		export PATH="$HOME/.rbenv/bin:$PATH"
	fi

	install_dependencies_packages

	if ! command -v bundle >/dev/null; then
		SETUP_RBENV_INTEGRATION=true
	fi

	if [[ "$SETUP_RBENV_PATH" == "true" ]] && [[ ! -d "$HOME/.rbenv/bin" ]]; then
		SETUP_RBENV_PATH=false
	fi

	# Make sure rbenv is currently loaded, just in case
	eval "$(rbenv init - bash)" || exit 1

	install_dependencies_ruby
	install_dependencies_bundler
	install_dependencies_gemfile
}

install_dependencies

CURRENT_SHELL=$(basename -- "$(ps -p $PPID -o command=)" | sed 's/^-//')

function setup_shell_integration() {
	local shell="$1"
	local skip_confirmation=$([[ "$CURRENT_SHELL" == "$shell" ]] && echo "y" || echo "n")
	local setup_shell_var="$(echo SETUP_${shell}RC | tr '[:lower:]' '[:upper:]')"
	local setup_shell="${!setup_shell_var:-false}"
	local rc_file_var="$(echo ${shell}RC_PATH | tr '[:lower:]' '[:upper:]')"
	local rc_file="${!rc_file_var}"

	[[ "$skip_confirmation" =~ ^[yY]$ ]] && setup_shell="true"

	if [[ "$setup_shell" != "true" ]] && [[ "$INTERACTIVE" == "true" ]]; then
		print_query "Do you want to setup the $shell integration? \e[90m[y/N]\e[0m "
		read setup_shell
		[[ "$setup_shell" =~ ^[yY]$ ]] && setup_shell="true"
	fi

	if [[ "$setup_shell" != "true" ]]; then
		print_action "Skipping $shell integration"
		return 0
	fi

	if [[ -z "$rc_file" ]] && [[ "$INTERACTIVE" == "true" ]]; then
		print_query "Location of the .${shell}rc file to edit? \e[90m(default: ${HOME}/.${shell}rc)\e[0m "
		read rc_file
		rc_file="${rc_file:-${HOME}/.${shell}rc}"
		rc_file="$(eval "echo $rc_file")"
	fi

	[[ -z "$rc_file" ]] && rc_file="${HOME}/.${shell}rc"

	print_action "Setting up shell integration in $rc_file"

	if [[ "$SETUP_OMNI_GIT" == "true" ]] && [[ -n "${OMNI_GIT}" ]]; then
		echo 'export OMNI_GIT="'"${OMNI_GIT}"'"' >> "$rc_file"
		if [ $? -ne 0 ]; then
			print_failed "Setup OMNI_GIT in $rc_file"
		else
			print_ok "Setup OMNI_GIT in $rc_file"
		fi
	fi

	if [[ "$SETUP_RBENV_PATH" == "true" ]]; then
		echo 'export PATH="$HOME/.rbenv/bin:$PATH"' >> "$rc_file"
		if [ $? -ne 0 ]; then
			print_failed "Setup rbenv path in $rc_file"
		else
			print_ok "Setup rbenv path in $rc_file"
		fi
	fi

	if [[ "$SETUP_RBENV_INTEGRATION" == "true" ]]; then
		echo 'eval "$(rbenv init - '"${shell}"')"' >> "$rc_file"
		if [ $? -ne 0 ]; then
			print_failed "Setup rbenv integration in $rc_file"
		else
			print_ok "Setup rbenv integration in $rc_file"
		fi
	fi

	echo "[[ -x \"${SCRIPT_DIR}/shell_integration/omni.${shell}\" ]] && source \"${SCRIPT_DIR}/shell_integration/omni.${shell}\"" >> "$rc_file"
	if [ $? -ne 0 ]; then
		print_failed "Setup $shell integration in $rc_file"
	else
		print_ok "Setup $shell integration in $rc_file"
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
