# Wrapping a command

`kakoi` knows nothing about the command it wraps. To make every invocation of a CLI go
through it, put a shim under the command's name on `PATH`. The repository ships a shim template,
[`examples/shim/codex`](../examples/shim/codex), and an Agent Skill that fills it in for the CLIs
on your machine.

## The shim template

Copy the template to a directory that comes before the real command on your `PATH`, name the
copy after that command, fill in the tool section at the top of the file, and make it
executable:

```sh
curl -fsSLo ~/.local/bin/codex \
  https://raw.githubusercontent.com/ba0918/kakoi/main/examples/shim/codex
chmod +x ~/.local/bin/codex
```

With the repository cloned, `cp examples/shim/codex ~/.local/bin/codex` does the same thing.

Every invocation of that name then goes through `kakoi`. The shim finds the real command
further down `PATH` (skipping itself), creates `/tmp/kakoi` when it is missing, and hands
the command's path to `kakoi` with the arguments unchanged.

The **tool section** is everything that depends on the command being wrapped:

| Value | Meaning |
| --- | --- |
| `REAL_COMMAND` | the name of the executable to look for on `PATH` |
| `BYPASS_FLAG` | the flag that turns the command's own sandbox off, inserted at the front of the argument list; empty if there is none |
| `WORKSPACE_OPTIONS`, `RW_OPTIONS` | the command's options whose values are copied to `--workspace` and `--rw` |
| `ALLOW_LIST` | subcommands passed straight to the real command, outside `kakoi` |
| `NO_FLAG_LIST` | subcommands that go through `kakoi` without `BYPASS_FLAG` |

The body under it depends on none of them. The values shipped in the tool section are filled in
for codex as an example of a command to wrap, not because `kakoi` has anything to do with
codex. Read the values for another command off its `--help`; the header of the template explains
how each is read and the one misreading that widens the boundary.

## Two ways out

Everything is isolated by default: `--help` and `--version` are no exception, and no subcommand
decides otherwise, so a subcommand a newer version of the command adds is isolated like the
rest. There are two ways out, and both are yours to open:

- A subcommand in the shim's **allow list** is passed straight to the real command, outside
  `kakoi`. It is a whitelist you approve and answer for, and it is empty as shipped.
- `KAKOI_SHIM_OFF=1` runs the real command with your arguments unchanged, outside
  `kakoi`, whatever the lists say.

Add a name to a list only when something breaks, and only the name that broke:

| What broke | Where the name goes |
|---|---|
| the command refuses the flag, or takes it and it has no effect | the no-flag list |
| it cannot reach the host from inside the isolation, and no model runs | the allow list |
| a model runs and it still breaks inside the isolation | neither list; fix the profile |

"No model runs" is what to check before approving an allow-list entry: while that subcommand
executes, the wrapped command asks no model and cannot run a command a model produced. Read the
command's documentation and watch a run of it. For codex, the header printed at start-up and
the output of `exec` are where it shows. The name most likely to be the first one in the list is
the one that authenticates, `login` in codex's case; that is a reading of the help text rather
than a measurement, and nothing is shipped in either list.

Both lists are matched against the first word of the argument list only. An argument list that
starts with an option never matches and is isolated as usual.

## Checking a copy

The template is not part of the product, so what is checked here about the bundled one and what
you check about your copy are two different things.

**What is checked here** runs with two directories at the front of `PATH`: the first holds the
copy under the wrapped command's name, the second holds stand-ins for the wrapped command and
for `kakoi` that print their arguments and exit. The order matters, since a stand-in
found before the copy would take the invocation instead of it. With both directories in place
the real command never runs, on the paths that go straight to it either. The conditions:

- an argument list starting with a subcommand, one starting with an option, and `--help` all
  reach `kakoi`;
- the values of the copied options appear in its arguments;
- a name put in a list temporarily takes effect only when it is the first word, an argument list
  starting with an option being isolated as usual;
- a copy whose flag is empty still reaches `kakoi`;
- with `KAKOI_SHIM_OFF=1` the `kakoi` stand-in is not started, and the stand-in
  for the wrapped command is reached with the arguments unchanged.

One condition needs the real command: that the flag put in front reaches it, which for codex is
a start-up header saying something other than `sandbox: read-only`.

**Yours to check about your copy**: that for every name in its allow list, no model runs while
that subcommand executes.

## The setup skill

[`skills/kakoi-setup`](../skills/kakoi-setup) is an Agent Skill that fits an
installation to the machine it is on. It asks first whether your profile, and later the shim in
the directory you pick, are kept elsewhere — generated, synced, or linked from a dotfiles
repository — and if so proposes the lines or the file for you to put in place instead of writing
there. It proposes:

- profile entries for the commands you want to wrap that are installed;
- a copy of the shim template with its tool section filled in for the command being wrapped,
  saying which options in the command's `--help` the template cannot copy (a positional working
  directory, several values after one option, several values joined into one word) and how to
  work around each;
- a check, on every route you start the command from, that the copy is found before the real
  command;
- `path-prepend` entries for replacement commands;
- for a command that broke inside the isolation, which of the two lists its name belongs in, or
  whether the profile is what to fix instead.

It puts a `--print-plan` from before a change next to one from after. Install it with your CLI's
own means, such as:

```sh
gh skill install ba0918/kakoi kakoi-setup
```

Run it **outside the isolation**: before the shim is on `PATH`, with `KAKOI_SHIM_OFF=1`,
or from a CLI not started through `kakoi`. The configuration directory may not sit inside
a writable mount item, so an isolated agent cannot edit its own profile. The agent is therefore
not isolated while the skill runs. Run your CLI in a mode that asks before writing, and check
the skill for yourself: that each of its "What to keep to" items is written there as an
instruction, and that a trial shows you a diff and asks for approval before every write.
