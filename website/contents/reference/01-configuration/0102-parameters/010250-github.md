---
description: Configuration of the `github` parameter
---

# `github`

## Parameters

Configuration related to the GitHub API.

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `auth` | list of [`Auth`](#auth-object) objects | How to handle authentication with the GitHub API; the first matching object is used, or the default authentication process is used (pulling an auth token from `gh` if available) |

### `Auth` object

This must contain one of the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `skip` | boolean | If `true`, this authentication object will be skipped |
| `token` | string | The GitHub API token to use for authentication |
| `token_env_var` | string | The environment variable containing the GitHub API token to use for authentication |
| `gh` | [`Gh`](#gh-object) object | Configuration for using the `gh` CLI for authentication. Depends on the `gh` CLI being installed and logged in, but won't error if it isn't. |

If specifying multiple of those for a single object, only the first matching one will be used, in the order they are listed above. If none is specified, the default authentication process is used (pulling an auth token from `gh` if available).

If provided in a list, it can also contain any or all of the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `repo` | [`Filter`](#filter-object) object | A filter on the repository, if the filter does not match the repository, this authentication object will be skipped. If the filter is not specified, the authentication object will always match. The repository is defined as the `<owner>/<name>` format |
| `hostname` | [`Filter`](#filter-object) object | A filter on the hostname of the GitHub API, if the filter does not match the hostname of the repository, this authentication object will be skipped. If the filter is not specified, the authentication object will always match. The hostname is defined as only the hostname part of the URL, e.g. `github.com` |

### `Gh` object

Can contain any or all of the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `hostname` | string | The hostname of the GitHub API to fetch the token for, if different from the repository's API URL |
| `user` | string | The username of the `gh` logged-in session to fetch the token for, if different than the default one |

The parameters that are not specified will simply be resolved automatically when using the `gh` command.

### `Filter` object

Can contain one of the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `contains` | string | A substring to match against the value of the filter |
| `starts_with` | string | A prefix to match against the value of the filter |
| `ends_with` | string | A suffix to match against the value of the filter |
| `regex` | string | A regular expression to match against the value of the filter |
| `glob` | string | A glob pattern to match against the value of the filter |
| `exact` | string | An exact string to match against the value of the filter |

If specifying multiple of those for a single object, only the first matching one will be used, in the order they are listed above.

If not specifying any of those, the filter will always match.

## Example

```yaml
github:
  auth:
    - repo:
        starts_with: 'omnicli/some-'
      token: gho_1234567890abcdef
    - repo:
        glob: 'omni*/no-auth-repo'
      skip: true
    - repo:
        contains: 'secret-env'
      token_env_var: GITHUB_TOKEN
    - gh
```
