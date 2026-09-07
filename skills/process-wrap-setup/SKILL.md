---
name: process-wrap-setup
description: "Adapt a process-wrap installation to this machine: propose profile entries for the agent CLIs that are installed, a place for the codex shim, path-prepend entries for replacement commands, and a review of the shim's subcommand classification. Use when the user asks to set up, adjust, or review their process-wrap profile, install the codex shim, or work out why a command inside the isolation cannot see or write something."
---

# process-wrap setup

`process-wrap` runs a command inside a bubblewrap mount namespace shaped by a layered
policy. It ships with a built-in default written for WSL2, and `process-wrap init` writes
that default out as `<configuration directory>/profile/default.toml` so it can be edited.
The configuration directory is `$XDG_CONFIG_HOME/process-wrap` or, when that variable is
unset, `~/.config/process-wrap`.

What is generic lives in the built-in default. What is specific to this machine — which
agent CLIs are installed, where their state lives, which host commands the isolation cuts
off — is what this skill works out with the user.

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

1. **Find the agent CLIs that are installed and propose profile entries for them.** Check
   which of codex, claude, opencode and any other agent CLI the user runs are on `PATH`,
   and where each keeps its state and its instructions. Propose `rw` for the state
   directory (so a session can persist) and `ro` for the instruction and settings files
   inside it (so an agent cannot rewrite the rules it runs under). Read the entries the
   built-in default already has before proposing more; say which of them do not apply to
   this machine.
2. **Propose where the codex shim goes.** The template is `examples/shim/codex` in the
   `process-wrap` source tree, which an installed skill does not carry beside it. Ask the
   user where that source tree is — the directory `cargo install --path` was run from, or
   a clone they made — and read the template from there. If they have none, say so and
   stop; do not write a shim from memory. Then propose a directory on `PATH` that comes
   before the real codex, ask the user to confirm it, and show the copy as a diff before
   writing.
3. **Propose `path-prepend` entries for replacement commands.** Some host commands stop
   working inside the isolation because the profile hides the socket or the drive they
   need. Where a stand-in exists, propose a directory holding it in
   `env.path-prepend`, which is added to the front of `PATH` inside the isolation. Name
   the command, what breaks it, and what the stand-in does.
4. **Compare `codex --help` with the shim's classification and ask the user about the
   rest.** Run `codex --help`, list its subcommands, and compare them with the two lists
   in the shim (`is_isolated_subcommand` and `is_passed_through_subcommand`). Report every
   subcommand that is in neither, say for each whether it looks like a form in which the
   agent may run commands, and ask the user which list it belongs in. The user decides;
   do not add one on your own.
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
