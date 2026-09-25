# Installing pasta for filtered mode

The `filtered` network mode runs `pasta`, from the passt project, as found on `PATH`. The
`pasta` a distribution ships can be too old for the options `kakoi` passes: Ubuntu 24.04's, for
one, lacks `--host-lo-to-ns-lo` and `--map-host-loopback`. `kakoi` then stops with exit code 125
and names the options that are missing. `host` and `none` do not use `pasta`.

## Check the pasta you have

`kakoi` passes these options to `pasta` by their long names:

```
--foreground
--pid
--config-net
--quiet
--host-lo-to-ns-lo
--userns
--netns
--no-map-gw
--map-host-loopback
```

The `pasta` on `PATH` is new enough when its `--help` lists every one of them. This prints the
names it does not list, and nothing when it lists them all:

```sh
command -v pasta
for name in --foreground --pid --config-net --quiet --host-lo-to-ns-lo \
    --userns --netns --no-map-gw --map-host-loopback; do
  pasta --help 2>&1 | grep -Eq -- "(^|[[:blank:],])$name([[:blank:],]|\$)" ||
    echo "missing: $name"
done
```

A name counts as listed only as a whole word: `--netns-only` does not list `--netns`. This is
the same check `kakoi` makes when `pasta` ends during its start.

## Versions

passt names its releases by date and commit, as in the upstream tag `2026_07_28.f8df3f1`.
Distributions number the same releases their own way, so compare the check above rather than a
version string.

- Oldest release with every option: `2024_10_30`.
- Recommended: `2026_07_16` or later. The meaning of `--host-lo-to-ns-lo` changed in that
  release, and the releases before it have not been tested with `kakoi`.
- Tested: `2026_07_28.f8df3f1`.

## Build pasta from source

When the `pasta` you have lacks an option, build the tested release from the upstream
repository and install it under `~/.local`. The build needs `git`, `gcc`, and `make` (on Debian
and Ubuntu, `sudo apt install git gcc make`), and takes well under a minute:

```sh
git clone https://passt.top/passt
cd passt
git checkout 2026_07_28.f8df3f1
make
make install prefix="$HOME/.local"
```

This places `passt` and `pasta` (a link to `passt`) in `~/.local/bin`, and their manual pages
under `~/.local/share/man`. It leaves the distribution's `pasta` in `/usr/bin` in place.

`kakoi` takes the first `pasta` on `PATH`, so `~/.local/bin` must come before `/usr/bin`. If
`command -v pasta` still prints `/usr/bin/pasta`, put `~/.local/bin` first in your shell's
startup file, for example:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Then run the check above again: `command -v pasta` prints the one in `~/.local/bin`, and no name
is missing.

## Ubuntu 24.04 and later

On Ubuntu 24.04 and later, a new `pasta` is not enough: `filtered` also needs the restriction on
unprivileged user namespaces turned off. See
[Allowing the user namespace](getting-started.md#allowing-the-user-namespace-on-ubuntu-2404-and-later).
