---
description: CI/CD
---

# CI/CD

Omni can be used in a Continuous Integration (CI) and/or Continuous Delivery (CD) pipeline to setup the environment and/or commands needed for your project, to be used in the same fashion as in your local development environment.

When planning to use omni in a CI/CD pipeline, we recommend pinning the versions of any dependency that will be installed by omni to ensure reproducibility. This can be done by avoiding `latest` or unspecific version numbers (e.g. `1` instead of `1.0.0`).

## GitHub Actions

If you use GitHub Actions, you can directly use the [omni setup action](https://github.com/marketplace/actions/omni-setup-action) that we provide. This wraps the installation of omni and the capability to directly run specific omni operations, such as `omni up`. This also handles caching for you, in order for your workflow to be as fast as possible.

Here is an example of a workflow that uses the omni setup action:

```yaml
name: Run tests


on:
  pull_request:
    branches:
      - main

  push:
    branches:
      - main


jobs:
  tests:
    name: Run tests
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install omni
        uses: omnicli/setup-action@v0
        with:
          up: true
          # Only cache when running on the main branch
          cache_write: ${{ github.ref == 'refs/heads/main' }}

      - name: Run tests
        # Assuming you created a 'tests' command in your omni configuration
        run: omni tests
```

## GitLab CI

On GitLab CI, you can use any docker image that has omni installed, and then run the `omni up` command to setup the environment. Here is an example of a `Dockerfile` that you can use in your GitLab CI pipeline:

```dockerfile
FROM debian:12-slim

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
       sudo curl git ca-certificates build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsLS get.omnicli.dev | sh
```

And here is an example of a `.gitlab-ci.yml` file that uses the above Docker image:

```yaml
build-job:
  stage: build
  image: omni-image # The name of the image you built with the above Dockerfile
  variables:
    OMNI_DATA_HOME: "$PWD/.omni/data"
    OMNI_CACHE_HOME: "$PWD/.omni/cache"
    PATH: "$PWD/.omni/data/shims:$PATH"
  cache:
    - key:
        prefix: omni-
        files: [".omni.yaml"] # Or any of the files you have used to put the omni configuration
      paths:
        - "$OMNI_DATA_HOME"
        - "$OMNI_CACHE_HOME"
  script:
    - omni up
```

## Other

CI/CD pipelines generally allow to run arbitrary commands in a shell. You can use the following command to setup the environment in your CI/CD pipeline:

```bash
run: |
  curl https://get.omnicli.dev | sh
  omni up

  # Export the path, might require some adjusting depending
  # on the CI/CD provider
  export PATH="$(omni hook init --print-shims-path):$PATH"
```
