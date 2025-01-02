---
sidebar_position: 2
description: Start to take advantage of omni in your own way
---

# Initial configuration

Omni offers [a number of configuration parameters](/reference/configuration/parameters). This page aims at guiding you through the very initial configuration to make omni work with your preferences.

## Create a configuration file

Omni can work without configuration file and will use the default parameters. However, we want to make it a bit more personal, so let's create the main global configuration file.

If you're not twiggling with the `XDG_CONFIG_HOME` environment variable, you can create this file at `~/.config/omni/config.yaml`.

## Setting up the repository path format

The first configuration option you will want to add is [`repo_path_format`](/reference/configuration/parameters/repo_path_format) since it will define the structure of your repositories inside of your worktree.

```yaml
repo_path_format: "%{org}/%{repo}"
```

Here are the three most common values for this variable:

| <div style={{width: 210 + 'px'}}>Value</div> | Description |
|----------------------------------------------|-------------|
| `%{host}/%{org}/%{repo}` | Full details on the repository in the path, used for instance by Go in the `GOPATH`. e.g. `WORKTREE/github.com/XaF/omni` |
| `%{org}/%{repo}`         | To split repositories by organization. e.g. `WORKTREE/XaF/omni` |
| `%{repo}`                | To put all the repositories directly in the worktree. e.g. `WORKTREE/omni` |

## Setting up your worktree

By default, omni will check if `~/git` exists, and will alternatively check if `$GOPATH/src` is defined and exists. If none, it will revert by default to consider `~/git` as your worktree.

However, if you have an habit or preference for a different path, for instance `~/workspace`, you can simply set a configuration parameter for this:

```yaml
worktree: ~/workspace
```

The `worktree` parameter accepts absolute, relative and home-prefixed (`~`) paths.

## Configuring a first organization

A number of omni's magic is made available through configured organizations. For instance, with the `github.com/XaF` organization configured, you'll be able to run `omni clone omni` to clone omni's repository.

:::note
Before manually handling this configuration, if your organization provides a main omni repository, you might want to `omni clone path.to/that/repo` before pursuing this section: this repository might suggest you to setup the organization you need!
:::

An organization is defined by a handle which provides the part of the URL that is always going to be used when cloning repositories for that organization. Organizations are evaluated in the order in which they are defined to look for repositories, so you will want to put first organizations that you clone repositories from most of the time.

If my organization is `omnicli` and I use `github.com`, I could use the following organization:

```yaml
org:
  - handle: github.com/omnicli
    trusted: true
```

Setting the organization as `trusted` means that `omni up` will be ran in repositories from that organization without requesting you to approve first. If you are not comfortable with this, you can set the value to `false` and handle decisions repository by repository.

If you want to add your own repositories to take advantage of omni's handling, this could look like this:

```yaml
org:
  - handle: github.com/omnicli
    trusted: true
  - handle: github.com/XaF
    trusted: true
    worktree: /Users/xaf/personal
```

In this case, we even decided to use an alternate worktree for the second organization to avoid potential conflicts when cloning repositories. This is not mandatory and really depends on your personal preference and usage.

Finally, if you want to take advantage of fast `org/repo` cloning for repositories of your favorite git provider, you could add an organization with only the hostname. For instance, with `github.com`, this would look like the following:

```yaml
org:
  - handle: github.com/omnicli
    trusted: true
  - handle: github.com/XaF
    trusted: true
  - handle: github.com
    trusted: false
```

We marked that organization as untrusted as if we were to trust it, we would trust every repository hosted in that organization, which could be dangerous.

