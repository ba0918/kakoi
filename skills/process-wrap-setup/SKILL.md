---
name: process-wrap-setup
description: "Adapt a process-wrap installation to this machine: propose profile entries for the commands that are installed, a copy of the shim template with its tool section filled in for the command being wrapped, path-prepend entries for replacement commands, and where a command that broke inside the isolation belongs. Use when the user asks to set up, adjust, or review their process-wrap profile, install a shim for a command, or work out why a command inside the isolation cannot see, write, or reach something."
---

# process-wrap setup

`process-wrap` runs a command inside a bubblewrap mount namespace shaped by a layered
policy. It ships with a built-in default written for WSL2, and `process-wrap init` writes
that default out as `<configuration directory>/profile/default.toml` so it can be edited.
The configuration directory is `$XDG_CONFIG_HOME/process-wrap` when that variable holds an
absolute path and `~/.config/process-wrap` otherwise — unset, empty and a relative path all
fall back the same way.

What is generic lives in the built-in default. What is specific to this machine — which
commands to wrap are installed, where their state lives, what goes in the tool section of a
shim for each of them, which host commands the isolation cuts off — is what this skill works
out with the user.

## Run outside the isolation

Run this skill outside the isolation: before the shim is on `PATH`, with
`PROCESS_WRAP_SHIM_OFF=1`, or from a CLI that was not started through `process-wrap`.
The configuration directory must not be inside a writable mount item, so `process-wrap`
refuses to start when it is (specification section 5.6) and an isolated agent cannot edit
its own profile.

That also means the agent is not isolated while this skill runs. Every write is therefore
shown as a diff and waits for the user's approval, and the user should run their CLI in a
mode that asks for approval before a write.

## What to do

1. **Find the commands that are installed and propose profile entries for them.** A command
   to wrap is any command the user wants to run inside the isolation; the agent CLIs they
   run — codex, claude, opencode and the rest — are the usual ones. Check which are on
   `PATH`, and where each keeps its state and the settings and instructions it reads.
   Propose `rw` for the state directory (so a session can persist) and `ro` for the
   instruction and settings files inside it (so a command cannot rewrite the rules it runs
   under). Read the entries the built-in default already has before proposing more; say
   which of them do not apply to this machine.
2. **Propose a copy of the shim template with its tool section filled in.** The template is
   `examples/shim/codex` in the `process-wrap` source tree, which an installed skill does
   not carry beside it. Ask the user where that source tree is — the directory
   `cargo install --path` was run from, or a clone they made — and read the template from
   there. If they have none, say so and stop; do not write a shim from memory. The body
   below the tool section is the same for every command; what changes is the tool section
   at the top, and the values shipped in it are filled in for codex as an example. For the
   command the user wants to wrap, propose those values from the output of that command's
   own `--help` and from nothing else: its executable name, the flag that turns its own
   sandbox off (empty if it has none), and the options whose values name a directory it
   works in, which become `--workspace` and `--rw`. Say plainly that these come from
   reading the help text and that you have not measured them; measuring them is the user's
   part. Leave both lists empty. Then propose a directory on `PATH` that comes before the
   real command, ask the user to confirm it, and show the copy as a diff before writing.
3. **Propose `path-prepend` entries for replacement commands.** Some host commands stop
   working inside the isolation because the profile hides the socket or the drive they
   need. Where a stand-in exists, propose a directory holding it in
   `env.path-prepend`, which is added to the front of `PATH` inside the isolation. Name
   the command, what breaks it, and what the stand-in does.
4. **When the user brings a command that broke inside the isolation, propose where it
   goes.** The shim's header carries a table of what broke against where the name goes.
   Follow it: a command that refuses the inserted flag, or takes it without effect, goes in
   the no-flag list; one that cannot reach the host from inside the isolation and runs no
   model while it executes goes in the allow list, which passes it straight through; one
   that runs a model and still breaks inside the isolation goes in neither list and is
   fixed in the profile instead. Say which row the case falls in and why, present the
   change as a diff, and for an allow-list entry say what the user has to establish before
   approving it — that while that subcommand executes the command asks no model and cannot
   run a command a model produced. The user decides; do not add a name on your own.
5. **Put a `--print-plan` from before the change next to one from after.** Run
   `process-wrap --print-plan -- true` before proposing a change and again after the user
   has accepted it, and show the two side by side. The plan lists the mount items applied
   and skipped with the reason, the four variables, the final environment, and the bwrap
   arguments, so the user can see exactly what the change opened or closed.

## What to keep to

1. **Show a diff and get approval before writing anything.** This includes the profile,
   any other policy file, and the shim. Never write first and report afterwards.
2. **Write only to two places: the configuration directory, and the directory the user
   approved for the shim.** Nothing else on the machine is yours to change. In
   particular, do not edit the user's shell configuration or their `PATH` for them;
   propose the line and let them add it.
3. **Do not read or write anything inside `secrets/`.** Say where a secret file goes
   (`<configuration directory>/secrets/<name>`), that the directory is created with mode
   0700 and is always hidden inside the isolation, and that the profile refers to it as
   `${config_dir}/secrets/<name>`. The user puts the value there; you never see it, and
   you never repeat a secret value back.
4. **Do not edit the specification or the README of `process-wrap`.** They describe the
   product, not this machine. If something on this machine cannot be expressed in the
   policy format, say so and stop; do not work around it by changing the documents.
