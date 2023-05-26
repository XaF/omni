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

function print_msg() { printf >&2 "\033[96momni:\033[0m \033[93minstall:\033[0m $@\n"; }
function print_ok() { print_msg "\033[92m[OK]\033[0m      $@"; }
function print_failed() { print_msg "\033[91m[FAILED]\033[0m  $@"; }
function print_pending() { print_msg "\033[90m[--]\033[0m      $@"; }
function print_query_nl() { print_msg "\033[90m[??]\033[0m      $@"; }
function print_query() { printf >&2 "$(print_query_nl "$*" 2>&1) "; }
function print_action() { print_msg "\033[90m[!!]\033[0m      $@"; }
function print_issue() { print_msg "\033[93m[~~]\033[0m      $@"; }

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
INTERACTIVE=${INTERACTIVE:-$([ -t 0 ] && echo true || echo false)}
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
		fishrc*)
			eval "$(parse_long_option "fishrc" "optional" "${OPTARG}" "${!OPTIND}")"
			SETUP_FISHRC=true
			FISHRC_PATH="${val}"
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

	local config_home=${XDG_CONFIG_HOME}
	if [[ -z "$config_home" ]] || ! [[ "$config_home" =~ ^/ ]]; then
		config_home="${HOME}/.config"
	fi

	local config_files=(
		"${OMNI_CONFIG}"
		"${config_home}/omni.yaml"
		"${config_home}/omni"
		"${HOME}/.omni.yaml"
		"${HOME}/.omni"
	)
	for file in "${config_files[@]}"; do
		# If file does not exist or is not readable, go to next file
		([[ -n "$file" ]] && [[ -f "$file" ]] && [[ -r "$file" ]]) || continue

		# If there is no param to search, just return the first file that exists, if any
		if [[ -z "$param" ]]; then
			echo $file
			return
		fi

		# Try and find if there is a line following the format 'param: value'
		# in the file, we want the lookup to be compatible both for macos and linux, so
		# we choose the right command line for that research, knowing that even on macos
		# someone might have installed gnu grep
		local matching_line=$(grep -E "^${param}:" "$file")
		[[ -n "$matching_line" ]] || continue

		# Use awk to extract the parameter value, and remove the potential quotes, single or double, around it
		local matching_value=$(echo "$matching_line" | \
			sed -E "s/^${param}: *//" | \
			sed -E 's/^"([^"]*)".*/\1/' | \
			sed -E "s/^'([^']*)'.*/\1/")
		[[ -n "$matching_value" ]] || continue

		# If we found a value, just return it
		echo $matching_value
		return
	done

	# If we reach here and there is no param, returns the first file of the list that is writeable
	if [[ -n "$param" ]]; then
		return
	fi

	for file in "${config_files[@]}"; do
		([[ -n "$file" ]] && [[ -w "$file" ]]) || continue
		echo $file
		return
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

	print_query_nl "Omni clones and looks for git repositories in a git base directory. For "
	print_query_nl "that to work, it needs to know where your git repositories are located."
	print_query_nl "It will also use this directory to clone itself during this installation."
	print_query_nl "Some people call that their workspace, worktree, or git base directory."
	print_query "What is your git base directory? \033[90m(default: ${default_git_dir})\033[0m "
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

		print_query_nl "Omni will clone repositories in a standardized path format."
		print_query_nl "Which repository path format do you wish to use?"
		echo -e >&2 "\033[90m  %{host}   registry (e.g. github.com)\033[0m"
		echo -e >&2 "\033[90m  %{org}    repository owner (e.g. XaF)\033[0m"
		echo -e >&2 "\033[90m  %{repo}   repository name (e.g. omni)\033[0m"

		local PS3="Format index: "
		select REPO_PATH_FORMAT in "%{host}/%{org}/%{repo}" "%{org}/%{repo}" "%{repo}" "other (custom)"; do
			if [[ "$REPO_PATH_FORMAT" == "other (custom)" ]]; then
				print_msg "Enter the format to use for repositories"
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
	local config_file="$(search_config)"
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
		echo -e >&2 "\033[90m$ git clone \"${repo_location}\" \"${clone_location}\" --depth 1\033[0m"
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

OMNI_INSTALL_CURRENT_SHELL=${OMNI_INSTALL_CURRENT_SHELL:-$(basename -- "$(ps -p $PPID -o command=)" | sed 's/^-//')}

# We need the base location for git repositories
if [[ -z "$OMNI_GIT" ]]; then
	export SETUP_OMNI_GIT=true
	export OMNI_GIT=$(query_omni_git)
	[[ $? -eq 0 ]] && [[ -n "${OMNI_GIT}" ]] || exit 1
fi

if ! $in_omni_dir; then
	clone_location=$(git_clone)
	[[ $? -eq 0 ]] || exit 1
	if [[ -z "$clone_location" ]]; then
		print_failed "weird error, clone_location is empty but shouldn't be"
		exit 1
	fi

	# Update SCRIPT_DIR to the location of the cloned repository
	SCRIPT_DIR="$clone_location"
fi

print_logo

# Find ruby version to install
ruby_version=$(grep "^ *- *ruby:" "${SCRIPT_DIR}/.omni.yaml" | \
	sed -E 's/^ *- ruby: *//' | \
	sed -E 's/^"([^"]*)"/\1/' | \
	sed -E "s/'([^']*)'/\1/" | \
	sed -E 's/ .*$//')
export ASDF_RUBY_VERSION="$ruby_version"

# Compute the OMNI_DATA_HOME directory location
if [[ -z "$OMNI_DATA_HOME" ]]; then
	if [[ -z "$XDG_DATA_HOME" ]] || ! [[ "$XDG_DATA_HOME" =~ ^/ ]]; then
		XDG_DATA_HOME="$HOME/.local/share"
	fi
	OMNI_DATA_HOME="${XDG_DATA_HOME}/omni"
fi

# Prepare the location of the ASDF data directory
export ASDF_DATA_DIR="$OMNI_DATA_HOME/asdf"

# Prepare versions required for shadowenv
rust_version="1.69.0"
shadowenv_version="2.1.0"

function install_dependencies_packages() {
	local expect_bin=(
		"autoconf"
		"bison"
		"curl"
		"gcc"
		"git"
		"make"
		"uuidgen"
	)
	local missing=()

	# Check that the expected commands are found
	for bin in "${expect_bin[@]}"; do
		if command -v "$bin" >/dev/null; then
			print_ok "$bin found"
		else
			print_issue "$bin not found"
			missing+=("$bin")
		fi
	done

	# If missing is empty, and omni's ruby version is already installed, we can return early
	local omni_ruby_installed=$([[ -d "${ASDF_DATA_DIR}/installs/ruby/${ruby_version}" ]] && echo true || echo false)
	if [[ "$omni_ruby_installed" == "true" ]] && [[ ${#missing[@]} -eq 0 ]]; then
		return
	fi

	if command -v brew >/dev/null; then
		print_ok "brew found"

		if [[ ${#missing[@]} -gt 0 ]]; then
			echo -e >&2 "\033[90m$ brew install ${missing[@]}\033[0m"
			brew install "${missing[@]}" || exit 1
		fi
	elif command -v apt-get >/dev/null; then
		print_ok "apt-get found"

		echo -e >&2 "${CAN_USE_SUDO:+\033[33m[sudo]\033[0m }\033[90m$ apt-get update\033[0m"
		(
			export DEBIAN_FRONTEND=noninteractive
			${CAN_USE_SUDO:+sudo }apt update
		) || exit 1

		local apt_packages=()
		if [[ "$omni_ruby_installed" == "false" ]]; then
			apt_packages+=(
				"libffi-dev"
				"libgdbm-dev"
				"libncurses5-dev"
				"libreadline-dev"
				"libssl-dev"
				"libyaml-dev"
				"zlib1g-dev"
			)
		fi
		for pkg in autoconf bison curl git; do
			if [[ " ${missing[@]} " =~ " $pkg " ]]; then
				apt_packages+=("$pkg")
			fi
		done
		for pkg in gcc make; do
			if [[ " ${missing[@]} " =~ " $pkg " ]]; then
				apt_packages+=("build-essential")
				break
			fi
		done
		if [[ " ${missing[@]} " =~ " uuidgen " ]]; then
			apt_packages+=("uuid-runtime")
		fi

		echo -e >&2 "${CAN_USE_SUDO:+\033[33m[sudo]\033[0m }\033[90m$ apt-get --yes --no-install-recommends install ${apt_packages[@]}\033[0m"
		(
			export DEBIAN_FRONTEND=noninteractive
			${CAN_USE_SUDO:+sudo }apt-get --yes install "${apt_packages[@]}"
		) || exit 1
	elif command -v dnf >/dev/null; then
		print_ok "dnf found"

		local dnf_packages=()
		if [[ "$omni_ruby_installed" == "false" ]]; then
			dnf_packages+=(
				"gdbm-devel"
				"libffi-devel"
				"libyaml-devel"
				"ncurses-devel"
				"openssl-devel"
				"readline-devel"
				"zlib-devel"
			)
		fi
		for pkg in autoconf bison curl gcc git make; do
			if [[ " ${missing[@]} " =~ " $pkg " ]]; then
				dnf_packages+=("$pkg")
			fi
		done
		if [[ " ${missing[@]} " =~ " uuidgen " ]]; then
			dnf_packages+=("util-linux")
		fi

		echo -e >&2 "\033[33m[sudo]\033[0m \033[90m$ dnf install -y ${dnf_packages[@]}\033[0m"
		sudo dnf install -y "${dnf_packages[@]}" || exit 1
	elif command -v pacman >/dev/null; then
		print_ok "pacman found"

		local pacman_packages=()
		if [[ "$omni_ruby_installed" == "false" ]]; then
			pacman_packages+=(
				"libffi"
				"libyaml"
				"openssl"
				"readline"
				"zlib"
			)
		fi
		for pkg in curl git; do
			if [[ " ${missing[@]} " =~ " $pkg " ]]; then
				pacman_packages+=("$pkg")
			fi
		done
		for pkg in autoconf bison gcc make; do
			if [[ " ${missing[@]} " =~ " $pkg " ]]; then
				pacman_packages+=("base-devel")
				break
			fi
		done
		if [[ " ${missing[@]} " =~ " uuidgen " ]]; then
			pacman_packages+=("util-linux")
		fi

		echo -e >&2 "\033[33m[sudo]\033[0m \033[90m$ yes | pacman -S --noconfirm ${pacman_packages[@]}\033[0m"
		yes | sudo pacman -S --noconfirm "${pacman_packages[@]}" || exit 1
	else
		print_issue "No package manager found"
		if [[ "$INTERACTIVE" == "true" ]]; then
			print_query "Please install the following dependencies manually:\n$(printf " - %s\n" "${missing[@]}")\nPress enter when ready to pursue."
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

function install_dependencies_asdf() {
	if ! [[ -d "$ASDF_DATA_DIR" ]]; then
		echo -e >&2 "\033[90m$ git clone https://github.com/asdf-vm/asdf.git $ASDF_DATA_DIR --branch v0.11.3\033[0m"
		git clone https://github.com/asdf-vm/asdf.git $ASDF_DATA_DIR --branch v0.11.3 || exit 1
	fi

	# Load asdf bash
	source "$ASDF_DATA_DIR/asdf.sh"

	# Make sure asdf is up to date
	echo -e >&2 "\033[90m$ asdf update\033[0m"
	asdf update || exit 1
}

function install_dependencies_rust() {
	echo -e >&2 "\033[90m$ asdf plugin add rust\033[0m"
	asdf plugin add rust || {
		if [[ $? == 2 ]]; then
			asdf plugin update rust || exit 1
		else
			exit 1
		fi
	}

	echo -e >&2 "\033[90m$ asdf install rust $rust_version\033[0m"
	asdf install rust $rust_version || exit 1
}

function install_dependencies_shadowenv() {
	# Make sure that omni bin path is in the PATH
	export PATH="${SCRIPT_DIR}/bin:$PATH"

	function check_shadowenv() {
		if command -v shadowenv >/dev/null; then
			# Check the version of shadowenv is the expected one
			local current_shadowenv_version="$(shadowenv --version 2>/dev/null | cut -d' ' -f2)"
			if [[ "$current_shadowenv_version" == "$shadowenv_version" ]]; then
				print_ok "shadowenv ${shadowenv_version} found"
				return 0
			fi

			print_issue "shadowenv found but version is ${current_shadowenv_version:-unreadable}, expected $shadowenv_version"
		fi
		return 1
	}

	# Check if we already have shadowenv, and in which case, which version
	if check_shadowenv; then
		return
	fi

	# Prepare to have to build shadowenv locally
	local shadowenv_bin="${SCRIPT_DIR}/bin/shadowenv"

	if [[ -e "$shadowenv_bin" ]]; then
		print_issue "moving ${shadowenv_bin} to ${shadowenv_bin}.old"
		mv "$shadowenv_bin" "${shadowenv_bin}.old"
	fi

	# If on MacOS, we can try and brew install it
	if command -v brew >/dev/null && [[ "$OSTYPE" == "darwin"* ]]; then
		echo -e >&2 "\033[33m[brew]\033[0m \033[90m$ brew install shadowenv\033[0m"
		brew install shadowenv || exit 1

		if ! command -v shadowenv >/dev/null; then
			print_issue "shadowenv not found after brew install, verify that brew is correctly installed and in the PATH"
			exit 1
		fi
		return
	fi

	# Install rust
	install_dependencies_rust

	# Clone shadowenv to a temp directory that we'll remove after
	local clone_tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/omni-shadowenv.XXXXXX")"

	echo -e >&2 "\033[90m$ git clone https://github.com/Shopify/shadowenv --branch ${shadowenv_version} --depth 1 $clone_tmp_dir\033[0m"
	git clone https://github.com/Shopify/shadowenv --branch "${shadowenv_version}" --depth 1 "$clone_tmp_dir" || exit 1

	(
		# Go to shadowenv's directory
		cd "$clone_tmp_dir"

		# Set the rust version
		export ASDF_RUST_VERSION="$rust_version"

		# Try to save on memory by using git cli
		mkdir -p .cargo
		printf "[net]\ngit-fetch-with-cli = true\n" > .cargo/config

		# Build shadowenv
		echo -e >&2 "\033[90m$ cargo build --release\033[0m"
		cargo build --release || exit 1

		# Move the binary to the bin directory
		mv "target/release/shadowenv" "$shadowenv_bin"
	) || {
		# If something went wrong, remove the temp directory
		rm -rf "$clone_tmp_dir"

		# And exit
		exit 1
	}

	# Remove the temp directory
	rm -rf "$clone_tmp_dir"

	# Check that shadowenv is now available
	if ! check_shadowenv; then
		print_failed "shadowenv ${shadowenv_version} still not found"
		exit 1
	fi
}

function install_dependencies_ruby() {
	# Check that asdf current returns the right version
	function check_ruby() {
		(
			cd "$SCRIPT_DIR" &&
			asdf current ruby 2>/dev/null | grep -q "$ruby_version"
		) && return 0 || return 1
	}

	if check_ruby; then
		print_ok "ruby $ruby_version found"
		return
	fi

	echo -e >&2 "\033[90m$ asdf plugin add ruby\033[0m"
	asdf plugin add ruby || {
		if [[ $? == 2 ]]; then
			asdf plugin update ruby || exit 1
		else
			exit 1
		fi
	}

	echo -e >&2 "\033[90m$ asdf install ruby $ruby_version\033[0m"
	asdf install ruby $ruby_version || exit 1

	if ! check_ruby; then
		print_failed "ruby $ruby_version still not found"
		exit 1
	fi

	unset -f check_ruby
}

function install_dependencies_bundler() {
	function check_bundler() {
		(
			command -v bundle >/dev/null &&
			cd "$SCRIPT_DIR" &&
			bundle --version 2>/dev/null | grep -q "\b2\."
		) && return 0 || return 1
	}

	# We then check that bundler is installed, that should be automated, but just in case
	if check_bundler; then
		print_ok "bundler 2.x found"
	elif ! command -v gem >/dev/null; then
		print_failed "gem command not found - something might be wrong with your setup!"
		exit 1
	else
		print_pending "Installing bundler"
		echo -e >&2 "\033[90m$ gem install bundler\033[0m"
		(cd "$SCRIPT_DIR" && gem install bundler) || exit 1
		print_ok "Installed bundler"
	fi

	if ! check_bundler; then
		print_failed "bundler 2.x still not found"
		exit 1
	fi

	unset -f check_bundler
}

function install_dependencies_gemfile() {
	# Finally, we can go into the repository and run the bundle install from there
	print_pending "Installing Gemfile dependencies"
	(
		cd "$SCRIPT_DIR"
		bundle config set path 'vendor/bundle'
		bundle install
	) || exit 1
	print_ok "Installed Gemfile dependencies"
}

function install_dependencies() {
	CAN_USE_SUDO="$(command -v sudo >/dev/null && echo sudo)"

	install_dependencies_packages
	install_dependencies_asdf
	install_dependencies_shadowenv
	install_dependencies_ruby
	install_dependencies_bundler
	install_dependencies_gemfile
}

install_dependencies

function setup_shell_integration() {
	local shell="$1"
	local default_value=$([[ "$OMNI_INSTALL_CURRENT_SHELL" == "$shell" ]] && echo "y" || echo "n")
	local setup_shell_var="$(echo SETUP_${shell}RC | tr '[:lower:]' '[:upper:]')"
	local setup_shell="${!setup_shell_var:-false}"
	local rc_file_var="$(echo ${shell}RC_PATH | tr '[:lower:]' '[:upper:]')"
	local rc_file="${!rc_file_var}"

	[[ "$skip_confirmation" =~ ^[yY]$ ]] && setup_shell="true"

	if [[ "$setup_shell" != "true" ]] && ! command -v "$shell" >/dev/null; then
		# If the shell is not installed, and the integration was not
		# specifically requested to be setup, we skip it
		return 0
	fi

	if [[ "$setup_shell" != "true" ]] && [[ "$INTERACTIVE" == "true" ]]; then
		local default_show="y/N"
		[[ "$default_value" =~ ^[yY]$ ]] && default_show="Y/n"
		print_query "Do you want to setup the $shell integration? \033[90m[${default_show}]\033[0m "
		read setup_shell
		[[ -z "$setup_shell" ]] && setup_shell="$default_value"
		[[ "$setup_shell" =~ ^[yY]$ ]] && setup_shell="true"
	fi

	if [[ "$setup_shell" != "true" ]]; then
		print_action "Skipping $shell integration"
		return 0
	fi

	if [[ -z "$rc_file" ]] && [[ "$INTERACTIVE" == "true" ]]; then
		local default_rc_file="${HOME}/.${shell}rc"
		if [[ "$shell" == "fish" ]]; then
			default_rc_file="${HOME}/.config/fish/conf.d/omni.fish"
		fi

		print_query "Location of the .${shell}rc file to edit? \033[90m(default: ${default_rc_file})\033[0m "
		read rc_file
		rc_file="${rc_file:-${default_rc_file}}"
		rc_file="$(eval "echo $rc_file")"
	fi

	[[ -z "$rc_file" ]] && rc_file="${HOME}/.${shell}rc"

	print_action "Setting up $shell integration in $rc_file"

	# Make sure directory exists
	mkdir -p "$(dirname "$rc_file")"

	if [[ "$SETUP_OMNI_GIT" == "true" ]] && [[ -n "${OMNI_GIT}" ]]; then
		echo 'export OMNI_GIT="'"${OMNI_GIT}"'"' >> "$rc_file"
		if [ $? -ne 0 ]; then
			print_failed "Setup OMNI_GIT in $rc_file"
		else
			print_ok "Setup OMNI_GIT in $rc_file"
		fi
	fi

	local conditional_load="[[ -f \"${SCRIPT_DIR}/shell_integration/omni.${shell}\" ]] && source \"${SCRIPT_DIR}/shell_integration/omni.${shell}\""
	if [[ "$shell" == "fish" ]]; then
		conditional_load="test -f \"${SCRIPT_DIR}/shell_integration/omni.fish\"; and source \"${SCRIPT_DIR}/shell_integration/omni.fish\""
	fi

	if [[ -f "$rc_file" ]] &&  grep -qF "$conditional_load" "$rc_file"; then
		print_pending "$shell integration already in $rc_file"
		return
	fi

	echo "${conditional_load}" >> "$rc_file"
	if [ $? -ne 0 ]; then
		print_failed "Setup $shell integration in $rc_file"
	else
		print_ok "Setup $shell integration in $rc_file"
	fi
}

SUPPORTED_SHELLS=("bash" "zsh" "fish")
# Setup first the integration for the current shell
for shell in "${SUPPORTED_SHELLS[@]}"; do
	[[ "$OMNI_INSTALL_CURRENT_SHELL" == "$shell" ]] && setup_shell_integration "$shell"
done
# Then offer to setup the other shell integrations
for shell in "${SUPPORTED_SHELLS[@]}"; do
	[[ "$OMNI_INSTALL_CURRENT_SHELL" == "$shell" ]] || setup_shell_integration "$shell"
done

function call_omni_up() {
	print_pending "Running 'omni up' from the omni directory"
	local extra_params=$([[ "$INTERACTIVE" == "false" ]] && echo "yes --trust" || echo "")
	export PATH="${PATH:+${PATH}:}${SCRIPT_DIR}/bin"
	if (cd "$SCRIPT_DIR" && OMNI_SKIP_UPDATE=true bin/omni up --update-user-config $extra_params); then
		print_ok "Ran 'omni up' from the omni directory"
	else
		print_failed "'omni up' failed"
		exit 1
	fi
}

# Finish setting up omni by using omni
call_omni_up

# All set-up !
print_ok "All done! Omni is now installed 🎉"
print_action "You might need to reload your shell or use 'source ~/.${OMNI_INSTALL_CURRENT_SHELL}rc' to start using omni"
