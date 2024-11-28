#!/usr/bin/env bash

asdf_tool_path() {
  local tool=$1
  local version=$2
  if [[ -z "${tool}" ]] || [[ -z "${version}" ]]; then
    echo "Tool and version should be specified as the first and second arguments when calling asdf_tool_bin_path"
    return 1
  fi

  echo "${HOME}/.local/share/omni/asdf/installs/${tool}/${version}"
}

asdf_tool_latest_version() {
  local tool=$1
  if [[ -z "${tool}" ]]; then
    echo "Tool should be specified as the first argument when calling asdf_tool_latest_version"
    return 1
  fi

  # Try to grab the latest version from the versions file
  latest_version=$(perl -ne '$last = $_ if /^\d+(\.\d+)*$/; END{print $last if $last}' \
                   "${PROJECT_DIR}/tests/fixtures/${tool}-versions.txt")

  if [ -z "${latest_version}" ]; then
    echo "Latest version for ${tool} could not be determined"
    return 1
  fi

  echo "${latest_version}"
}

add_asdf_tool_calls() {
  local tool=
  local latest_version=
  local version="latest"
  local fallback_version=
  local plugin_installed=false
  local installed=false
  local others_installed=false
  local venv=false
  local cache_versions=false
  local list_versions=true
  local upgrade=false
  local no_upgrade_installed=false

  for arg in "$@"; do
    case $arg in
      tool=*)
	tool="${arg#tool=}"
	shift
	;;
      latest_version=*)
	latest_version="${arg#latest_version=}"
	shift
	;;
      version=*)
        version="${arg#version=}"
        shift
        ;;
      fallback_version=*)
        fallback_version="${arg#fallback_version=}"
        shift
        ;;
      plugin_installed=*)
        plugin_installed="${arg#plugin_installed=}"
        shift
        ;;
      installed=*)
        installed="${arg#installed=}"
        shift
        ;;
      others_installed=*)
        others_installed="${arg#others_installed=}"
        shift
        ;;
      venv=*)
        venv="${arg#venv=}"
        shift
        ;;
      cache_versions=*)
        cache_versions="${arg#cache_versions=}"
        shift
        ;;
      list_versions=*)
        list_versions="${arg#list_versions=}"
        shift
        ;;
      upgrade=*)
        upgrade="${arg#upgrade=}"
        shift
        ;;
      no_upgrade_installed=*)
        no_upgrade_installed="${arg#no_upgrade_installed=}"
        shift
        ;;
      *)
        echo "Unknown argument: $arg"
        return 1
        ;;
    esac
  done

  if [ -z "${tool}" ]; then
    echo "Tool should be specified using tool={tool_name} when calling add_asdf_tool_calls"
    return 1
  fi

  if [ "$version" = "latest" ] || [ "$version" = "*" ]; then
    if [[ -z "${latest_version}" ]]; then
      latest_version=$(asdf_tool_latest_version "${tool}")
    fi
    version="${latest_version}"
  fi

  if [ "$cache_versions" = "true" ] || [ "$cache_versions" = "expired" ]; then
    if [ "$cache_versions" = "true" ]; then
      date=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    else
      # This allows to support GNU date but also BSD date
      date=$(date -u +"%Y-%m-%dT%H:%M:%SZ" -d "300 days ago" 2>/dev/null ||
             date -u -v-300d +"%Y-%m-%dT%H:%M:%SZ")
    fi
    mkdir -p "${HOME}/.cache/omni"
    perl -pe 's/{{ UPDATED_AT }}/'"${date}"'/g' "${PROJECT_DIR}/tests/fixtures/asdf_operation_cache.json" > "${HOME}/.cache/omni/asdf_operation.json"
  fi

  add_command asdf update

  if [ "$plugin_installed" = "true" ]; then
    add_command asdf plugin list
    add_command asdf plugin add ${tool}
  else
    add_command asdf plugin list <<EOF
${tool}
EOF
  fi

  if [ "$upgrade" = "false" ]; then
    if [ "$no_upgrade_installed" = "false" ]; then
      add_command asdf list ${tool}
    else
      add_command asdf list ${tool} <<EOF
$(for v in $(echo "${no_upgrade_installed}" | perl -pe 's/,+/,/g' | sort -u); do echo "  ${v}"; done)
EOF
    fi
  fi

  if [ "$list_versions" = "true" ]; then
    add_command asdf plugin update ${tool}
    add_command asdf list all ${tool} <"${PROJECT_DIR}/tests/fixtures/${tool}-versions.txt"
  elif [ "$list_versions" = "fail-update" ]; then
    add_command asdf plugin update ${tool} exit=1
  elif [ "$list_versions" = "fail" ]; then
    add_command asdf plugin update ${tool}
    add_command asdf list all ${tool} exit=1
  fi

  if [ "$installed" = "true" ]; then
    installed_versions=$(echo "${others_installed},${no_upgrade_installed},${version}" | \
      perl -pe 's/(^|,)false(?=,|$)/,/g' | \
      perl -pe 's/,+/\n/g' | \
      perl -pe '/^$/d' | \
      sort -u)
    add_command asdf list ${tool} "${version}" exit=0 <<EOF
$(for v in ${installed_versions}; do echo "  ${v}"; done)
EOF

  else
    if [ "$others_installed" = "false" ]; then
      installed_versions=""
      add_command asdf list ${tool} "${version}" exit=1
    else
      installed_versions=$(echo "${others_installed}" | tr ',' '\n' | sort -u)
      add_command asdf list ${tool} "${version}" exit=0 <<EOF
$(for v in ${installed_versions}; do echo "  ${v}"; done)
EOF
    fi
    if [ "$installed" = "fail" ]; then
      add_command asdf install ${tool} "${version}" exit=1 <<EOF
stderr:Error installing ${tool} ${version}
EOF
      add_command asdf list ${tool} <<EOF
$(for v in ${installed_versions}; do echo "  ${v}"; done)
EOF

      if [ -n "${fallback_version}" ]; then
        # Replace version by the fallback version here!
        version="${fallback_version}"
        add_command asdf list ${tool} "${version}" exit=0 <<EOF
  ${version}
EOF
      fi
    else
      add_command asdf install ${tool} "${version}"
    fi
  fi

  if [ "$venv" = "true" ]; then
    add_fakebin "${HOME}/.local/share/omni/asdf/installs/${tool}/${version}/bin/${tool}"
    add_command ${tool} -m venv "regex:${HOME}/\.local/share/omni/wd/.*/${tool}/${version}/root"
  fi
}

add_asdf_tool_brew_calls() {
  local tool=$1
  if [[ -z "${tool}" ]]; then
    echo "Tool should be specified as the first argument when calling add_asdf_tool_brew_calls"
    return 1
  fi

  formulas=(autoconf coreutils curl libyaml openssl@3 readline)
  if [[ "${tool}" == "python" ]]; then
    formulas+=(pkg-config)
  fi

  local checked_prefix=false
  for formula in "${formulas[@]}"; do
    add_command brew list --formula "${formula}" exit=1
    add_command brew install --formula "${formula}"
    add_command brew --prefix --installed "${formula}"
    if [ "$checked_prefix" = false ]; then
      checked_prefix=true
      add_command brew --prefix
    fi
  done
}

