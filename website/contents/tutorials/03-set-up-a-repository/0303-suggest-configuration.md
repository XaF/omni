---
description: Suggesting user configuration changes from a git repository
---

# Configuration suggestions

If you've followed the previous sections, we now have a `.omni.yaml` in our repository that contains an `up` configuration and a `path` parameter to append our path to the `omnipath`. But at the end of that section, we required users wanting to access those commands throughout their systems to add the `path` configuration parameter to their global configuration **manually**.

In this section, we're going to talk about the `suggest_config` configuration parameter, which will allow you, as the repository provider, to suggest users with changes to their global configuration. This will make the process of your users using `omni` even smoothier.

## Suggest `omnipath` additions

Let's resume from the same repository configuration file, and let's add the `suggest_config` configuration parameter to append to the global `path` configuration:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

path:
  append:
    - python
    - go/bin

# highlight-start
suggest_config:
  path:
    append__toappend:
      - python
      - go/bin
# highlight-end
```

With this extra configuration:
- We're suggesting configuration changes
- Those configuration changes are related to the `path` key
- Those configuration changes are related to the `append` subkey, which is a list
- We wish to append (`__toappend`) the `python` and `go/bin` paths to that list, which are paths relative to the repository configuration file location

When cloning the repository, or running `omni up --update-user-config` in it, and after accepting to trust your repository (if not already configured in their organizations), the user will get a prompt looking like the following:

```
omni: up: The current repository is suggesting configuration changes.
omni: up: The following is going to be changed in your omni configuration:
  @@ -1,1 +1,4 @@
  -null
  +path:
  +  append:
  +  - /path/to/repo/python
  +  - /path/to/repo/go/bin
? Do you want to apply the changes? (Ynsh)
```

## Suggest an organization

Omni makes it simple to `cd` to and `clone` repositories without specifying the whole path for defined organizations. On top of that, known organizations can be automatically trusted to run `omni up` commands. If you provide more than one repositories for your users, you will want to suggest them to configure your organization.

This is easily done in the same way the path was suggested above:

```yaml showLineNumbers title="/path/to/repo/.omni.yaml"
up:
  - python
  - go
  - custom:
      name: "Compile sayhello go binary"
      met?: |
        bin="go/bin/gosayhello"
        [ -f "$bin" ] && \
          [ -x "$bin" ] && \
          [ "$bin" -nt go/src/gosayhello.go ]
      meet: "go build -o go/bin/gosayhello go/src/gosayhello.go"
      unmeet: "rm go/bin/gosayhello"

path:
  append:
    - python
    - go/bin

suggest_config:
  path:
    append__toappend:
      - python
      - go/bin
# highlight-start
  org__toprepend:
    - handle: github.com/omnicli
      trusted: true
# highlight-end
```

With this extra configuration:
- We're suggesting configuration changes
- We still suggest changes to the `path` key as previously
- We also suggest changes to the `org` key, which is a list
- We wish to prepend (`__toprepend`) the new organization to that list, which we also suggest to explicitly trust

When cloning the repository, or running `omni up --update-user-config` in it, and after accepting to trust your repository (if not already configured in their organizations), the user will get a prompt looking like the following:

```
omni: up: The current repository is suggesting configuration changes.
omni: up: The following is going to be changed in your omni configuration:
  @@ -1,1 +1,7 @@
  -null
  +org:
  +- handle: github.com/omnicli
  +  trusted: true
  +path:
  +  append:
  +  - /path/to/repo/python
  +  - /path/to/repo/go/bin
? Do you want to apply the changes? (Ynsh)
```

:::tip Users can accept all or only part of the suggestions
The `s` option from the above prompt means `split` - if selected, the user will be able to choose which configuration parameters they want to accept suggestions for, if any.
:::
