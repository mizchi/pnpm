---
id: fetch
title: pnpm fetch
---

Fetch packages from a lockfile into the virtual store. Dependency versions come from the lockfile.
A missing package manifest is not created.

`pnpm fetch` requires `pnpm-lock.yaml` and does not update it. When the project pins a pnpm version that commands would switch to, `pnpm fetch` also downloads that pnpm into the store, so a later `pnpm install --offline` finds it.

## Usage scenario

This command is specifically designed to improve building a docker image.

A typical Dockerfile for a project using pnpm looks like this (see also
[Working with Docker](../docker.md)):

```dockerfile title="Dockerfile"
FROM ghcr.io/pnpm/pnpm:12

RUN pnpm runtime set node 22 -g

WORKDIR /app

# Files required by pnpm install. Leave out .pnpmfile.mjs if the project has none,
# because COPY fails on a missing source.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .pnpmfile.mjs ./

# If you patched any package, include patches before install too
COPY patches patches

RUN pnpm install --frozen-lockfile --prod

# Bundle app source
COPY . .

EXPOSE 8080
CMD [ "node", "server.js" ]
```

As long as there are no changes to `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`,
`.pnpmfile.mjs`, docker build cache is still valid up to the layer of
`RUN pnpm install --frozen-lockfile --prod`, which cost most of the time
when building a docker image.

However, modification to `package.json` may happen much more frequently than
we expect, because it does not only contain dependencies, but may also
contain the version number, scripts, and arbitrary configuration for any other
tool.

It's also hard to maintain a Dockerfile that builds a monorepo project, it may
look like

```dockerfile title="Dockerfile"
FROM ghcr.io/pnpm/pnpm:12

RUN pnpm runtime set node 22 -g

WORKDIR /app

# Files required by pnpm install. Leave out .pnpmfile.mjs if the project has none,
# because COPY fails on a missing source.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .pnpmfile.mjs ./

# If you patched any package, include patches before install too
COPY patches patches

# for each sub-package, we have to add one extra step to copy its manifest
# to the right place, as docker have no way to filter out only package.json with
# single instruction
COPY packages/foo/package.json packages/foo/
COPY packages/bar/package.json packages/bar/

RUN pnpm install --frozen-lockfile --prod

# Bundle app source
COPY . .

EXPOSE 8080
CMD [ "node", "server.js" ]

```
As you can see, the Dockerfile has to be updated when you add or remove
sub-packages.

`pnpm fetch` solves the above problem perfectly by providing the ability
to load packages into the virtual store using only information from a lockfile and a configuration file (`pnpm-workspace.yaml`).

```dockerfile title="Dockerfile"
FROM ghcr.io/pnpm/pnpm:12

RUN pnpm runtime set node 22 -g

WORKDIR /app

# pnpm fetch does require only lockfile
COPY pnpm-lock.yaml pnpm-workspace.yaml ./

# If you patched any package, include patches before running pnpm fetch
COPY patches patches

RUN pnpm fetch --prod


COPY . ./
RUN pnpm install -r --offline --prod


EXPOSE 8080
CMD [ "node", "server.js" ]
```

It works for both simple and monorepo projects, `--offline` enforces
pnpm not to communicate with the package registry as all needed packages are
already present in the virtual store.

As long as the lockfile is not changed, the build cache is valid up to the
layer, so `RUN pnpm install -r --offline --prod`, will save you much
time.



:::note

Local `file:` protocol dependencies are skipped during `pnpm fetch`, since they reference directories that may not be available at fetch time (e.g. in Docker builds).

:::

## Options

### --filter &lt;selector\>, --filter-prod &lt;selector\>

Fetch only the dependencies of the selected projects. Without a filter, `pnpm fetch` fetches dependencies for every project in the lockfile.

Directory selectors work without project manifests, including for projects omitted by a sparse checkout or a Docker build. Name selectors read names from the available project manifests. Dependency versions and workspace dependency links still come from the lockfile.

Fetch the dependencies of an application and its workspace dependencies:

```sh
pnpm --filter '{apps/web}...' fetch
```

Add `--workspace-root` when the root project's dependencies are needed too. `--filter-prod` follows only production workspace dependencies when expanding a selector. Add `--prod` to also exclude development packages from the fetch.

Filtering reduces downloads, extraction, and virtual-store creation. To save a cache containing only those dependencies, use a separate `--store-dir`. Filtering does not remove other packages already in that store.

See [Filtering](../filtering.md) for selector syntax.

### --dev, -D

Only development packages will be fetched

### --prod, --production, -P

Development packages will not be fetched

### --ignore-pnpmfile

Don't run the hooks of the [pnpmfile](../pnpmfile.md).
