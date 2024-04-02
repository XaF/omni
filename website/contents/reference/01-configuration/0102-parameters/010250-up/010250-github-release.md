---
description: Configuration of the `github-release` kind of `up` parameter
---

# `github-release` operation

Install a tool from a GitHub release.

For this to work properly for a GitHub release, it will need to:
- Be provided as a `.tar.gz` or `.zip` archive
- Have a file name that contains hints about the OS it was built for (e.g. `linux`, `darwin`, ...)
- Have a file name that contains hints about the architecture it was built for (e.g. `amd64`, `arm64`, ...)

Omni will download all the assets matching the current OS and architecture, extract them and move all the found binary files to a known location to be loaded in the repository environment.

:::note
This does not support using authentication yet, and thus will only work for public repositories for now.
:::

## Alternative names

- `ghrelease`
- `github_release`
- `githubrelease`

## Parameters

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `repository` | string | The name of the repository to download the release from, in the `<owner>/<name>` format; can also be provided as an object with the `owner` and `name` keys |
| `version` | string | The version of the tool to install; see [version handling](#version-handling) below for more details. |
| `prerelease` | boolean | Whether to download a prerelease version or only match stable releases |
| `api_url` | string | The URL of the GitHub API to use, useful to use GitHub Enterprise (e.g. `https://github.example.com/api/v3`); defaults to `https://api.github.com` |

### Version handling

The following strings can be used to specify the version:

| Version | Meaning |
|---------|---------|
| `1.2`     | Accepts `1.2` and any version prefixed by `1.2.*` |
| `1.2.3`   | Accepts `1.2.3` and any version prefixed by `1.2.3.*` |
| `~1.2.3`  | Accepts `1.2.3` and higher patch versions (`1.2.4`, `1.2.5`, etc. but not `1.3.0`) |
| `^1.2.3`  | Accepts `1.2.3` and higher minor and patch versions (`1.2.4`, `1.3.1`, `1.4.7`, etc. but not `2.0.0`) |
| `>1.2.3`  | Must be greater than `1.2.3` |
| `>=1.2.3` | Must be greater or equal to `1.2.3` |
| `<1.2.3`  | Must be lower than `1.2.3` |
| `<=1.2.3` | Must be lower or equal to `1.2.3` |
| `1.2.x`   | Accepts `1.2.0`, `1.2.1`, etc. but will not accept `1.3.0` |
| `*`       | Matches any version (will default to `latest`) |
| `latest`  | Latest release |
| `auto`    | Lookup for any version files in the project directory (`.tool-versions`, `.go-version`, `.golang-version` or `.go.mod`) and apply version parsing |

The version also supports the `||` operator to specify ranges. This operator is not compatible with the `latest` and `auto` keywords. For instance, `1.2.x || >1.3.5 <=1.4.0` will match any version between `1.2.0` included and `1.3.0` excluded, or between `1.3.5` excluded and `1.4.0` included.

The latest version satisfying the requirements will be installed.

## Examples

```yaml
up:
  # Will error out since no repository is provided
  - github-release

  # Will install the latest release of the `omni` tool
  # from the `XaF/omni` repository
  - github-release: XaF/omni

  # We can call it with any of the alternative names too
  - ghrelease: XaF/omni
  - github_release: XaF/omni
  - githubrelease: XaF/omni

  # Will also install the latest version
  - github-release:
      repository: XaF/omni
      version: latest

  # Will install any version starting with 1.20
  - github-release:
      repository: XaF/omni
      version: 1.2

  # Will install any version starting with 1
  - github-release:
      repository: XaF/omni
      version: 1

  # Full specification of the parameter to identify the version;
  # this will install any version starting with 1.2.3
  - github-release:
      repository: XaF/omni
      version: 1.2.3

  # Will install any version starting with 1, including
  # any pre-release versions
  - github-release:
      repository: XaF/omni
      version: 1
      prerelease: true
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `PATH` | prepend | Injects the path to the binaries of the installed tool |
