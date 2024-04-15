---
description: Template support
---

# Templates

Omni supports templating for some elements of the configuration. The documentation specific to the templating format can be found on the [Tera documentation](https://keats.github.io/tera/docs/#templates), and is very close to the Jinja2 templating syntax.


## Variables

### `id` variable

The `id` variable contains the current work directory identifier, which is the name of the directory in which the command is being run.

### `root` variable

The `root` variable contains the root directory of the current work directory.

### `repo` object

The `repo` object contains information about the current repository, if in a repository.

| Property | Type | Description |
|----------|------|-------------|
| `handle` | string | The handle of the repository, corresponding to the URL allowing to clone the repository |
| `host` | string | The hostname of the repository |
| `org` | string | The organization of the repository |
| `name` | string | The name of the repository |

### `env` object

The `env` map contains the environment variables of the current process.

### `prompts` object

When using prompts in your working directory, they are made available through the `prompts` object in a template, as soon as that prompt has been answered. This means that you can use that prompt's answer when asking any following prompt.

For instance, if you have a prompt asking for a team, and another asking for a subteam, you can use the answer to the team prompt to conditionally ask the subteam prompt:

```yaml
prompts:
- id: team
  prompt: What is your team?
  scope: org
  type: choice
  choices:
  - id: team1
    choice: team 1
  - team 2
  - team 3
- id: subteam
  prompt: What are your subteams in {{ prompts.team }}?
  scope: org
  type: multichoice
  choices: |
    - subteam 1
    - subteam 2
    {{% if prompts.team == "team1" %}}- subteam 3{{% endif %}}
  if: '{{ prompts.team == "team1" or prompts.team == "team 2" }}'
```

The type of each property in the `prompts` object is dependent on the type of the prompt. More details are available in the [prompts documentation](/reference/configuration/parameters/prompts#prompt-types).


## Helper functions

### `partial_resolve`

The `partial_resolve` function allows to resolve a repository partially from the context of the current repository. This is useful if you want to use the full repository path to another repository of the same organization, but do not want to hardcode the organization, hostname and protocol used.

```yaml
suggest_clone:
  template: |
    - {{ partial_resolve(handle="my-repo") }}
    - {{ partial_resolve(handle="other-org/other-repo") }}
```
