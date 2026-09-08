---
name: process-wrap-setup
description: "Adapt a process-wrap installation to this machine: ask first whether the profile and the shim directory are edited elsewhere (then propose lines instead of writing), propose profile entries for the commands that are installed, a copy of the shim template with its tool section filled in for the command being wrapped and a check that every route to the command finds the copy first, path-prepend entries for replacement commands, and where a command that broke inside the isolation belongs. Use when the user asks to set up, adjust, or review their process-wrap profile, install a shim for a command, or work out why a command inside the isolation cannot see, write, or reach something."
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

1. **Ask first whether the profile is edited somewhere else.** Before proposing anything for
   the profile, ask the user whether the profile in the configuration directory is the file
   they edit, or whether it is produced from a file kept elsewhere — generated or synced from
   a dotfiles repository or a template, or a symbolic link to such a file. The user's answer is
   the only source: do not decide it from a marker in the file or from its contents. If the
   answer is "kept elsewhere", the profile is not yours to write for the rest of the session;
   propose the lines for the user to add to the file they edit instead (item 2 of "What to keep
   to"), and take the after-change plan the way item 7 below says.
2. **Find the commands that are installed and propose profile entries for them.** A command
   to wrap is any command the user wants to run inside the isolation; the agent CLIs they
   run — codex, claude, opencode and the rest — are the usual ones. Check which are on
   `PATH`, and where each keeps its state and the settings and instructions it reads.
   Propose `rw` for the state directory (so a session can persist) and `ro` for the
   instruction and settings files inside it (so a command cannot rewrite the rules it runs
   under). Read the entries the built-in default already has before proposing more; say
   which of them do not apply to this machine.
3. **Propose a copy of the shim template with its tool section filled in.** The template is
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
   part. Leave both lists empty.

   While reading the help text, look for the shapes the template cannot copy, and say so
   for each one instead of passing over it, naming the option and why:

   - **The working directory is a positional argument** (`tool [project]`). The template
     looks for option names only, so nothing is copied and `--workspace` stays the current
     directory. The way around it: start the command from that directory.
   - **One option takes several values separated by spaces** (`--add-dir a b c`). The
     template reads one value per option, so `a` is copied and `b` and `c` are dropped; the
     command still receives all three, and inside the isolation the dropped ones are not
     writable. The way around it, for an option copied to `--rw`: repeat the option once per
     directory, if the command accepts that. For an option copied to `--workspace` only the
     last one survives, so treat it like the positional case and start from that directory.
   - **Several values joined into one word** (`--add-dir a,b`). The whole word is copied as
     one path that does not exist: for `--rw` it is skipped, for `--workspace` `process-wrap`
     stops with a `path` diagnostic.

   None of these is fixed by editing the template's body; the specification leaves them out
   on purpose (section 18).

   Then propose a directory on `PATH` that comes before the real command, name the copy
   after the command being wrapped so that every invocation of that name goes through it,
   and ask the user to confirm the directory. When they confirm it, ask about that directory
   what item 1 asked about the profile: is a shim placed there kept elsewhere — generated,
   synced, or linked from a file they edit? If it is, do not write there; propose the file's
   content for the user to put in place themselves (item 2 of "What to keep to"). Otherwise
   show the copy as a diff before writing, and say with it that the copy will be made
   executable, since a diff does not carry the file mode. After writing it, make it
   executable (`chmod +x`) and check that it is; a copy without the executable bit is
   skipped by the `PATH` search in silence, and the real command runs unwrapped.
4. **Check that the copy is found first, on every route the command is started from.** Once
   the copy is in place, ask the user where they start the command from: their interactive
   shell, the command execution of the agent CLI this skill runs in, a cron job, an editor,
   and so on. For each route, resolve the name with `command -v NAME` on that route and
   compare the real path of what it returns (`realpath`) with the real path of the copy;
   they must be the same file. Do not start the command to find out. The two routes you
   always check are the user's interactive shell and this CLI's own command execution — the
   latter you can run yourself. Routes you cannot reach are the user's to check, with the
   same command. If the check fails on this CLI's side, the likeliest reason is that the
   `PATH` change the user just made has not reached this CLI's process; ask them to restart
   the CLI and check again before concluding anything about the order. Warn about version
   managers: their shims directory (mise, asdf and the like) coming before the copy sends the
   name straight to the real command, and a directory that sits behind the copy in an
   interactive shell can sit in front of it on a non-interactive `PATH`.
5. **Propose `path-prepend` entries for replacement commands.** Some host commands stop
   working inside the isolation because the profile hides the socket or the drive they
   need. Where a stand-in exists, propose a directory holding it in
   `env.path-prepend`, which is added to the front of `PATH` inside the isolation. Name
   the command, what breaks it, and what the stand-in does. This is the `PATH` inside the
   isolation, not the host `PATH` whose order item 4 checks.
6. **When the user brings a command that broke inside the isolation, propose where it
   goes.** The shim's header carries a table of what broke against where the name goes.
   Follow it: a command that refuses the inserted flag, or takes it without effect, goes in
   the no-flag list; one that cannot reach the host from inside the isolation and runs no
   model while it executes goes in the allow list, which passes it straight through; one
   that runs a model and still breaks inside the isolation goes in neither list and is
   fixed in the profile instead. Say which row the case falls in and why, present the
   change as a diff, and for an allow-list entry say what the user has to establish before
   approving it — that while that subcommand executes the command asks no model and cannot
   run a command a model produced. The user decides; do not add a name on your own.
7. **Put a `--print-plan` from before the change next to one from after.** Run
   `process-wrap --print-plan -- true` before proposing a change and again after the user
   has accepted it, and show the two side by side. The plan lists the mount items applied
   and skipped with the reason, the four variables, and how the environment differs from
   the host's, so the user can see exactly what the change opened or closed. Use
   `process-wrap --print-plan=full -- true` when the comparison has to be exact: it adds
   the merged policy, the origin of every item, the whole environment, and the bwrap
   arguments. `process-wrap --print-plan=json -- true` is the same content as one line of
   JSON, made for you rather than for the user: compare the two plans by key, and show the
   user the summary. When the profile is kept elsewhere (item 1), you proposed lines rather
   than writing them: take the after-change plan only once the user says they have added the
   lines and the configuration directory reflects them, use `--print-plan=full` for both
   plans, check that the items you proposed appear in the merged policy, and say so if they
   do not.

## What to keep to

1. **Show a diff and get approval before writing anything.** This includes the profile,
   any other policy file, and the shim. Never write first and report afterwards. This holds
   when the user dictated the content themselves: show where it lands — which file, which
   place in it — as a diff, and wait for approval, before writing. That the user wrote the
   lines is not approval of the write. Nor is the approval prompt of the agent CLI you run
   in: it may or may not appear, and it does not replace yours.
2. **Write only to two places: the configuration directory, except `secrets/`, and the
   directory the user approved for the shim.** Nothing else on the machine is yours to
   change. A place the user said is kept elsewhere (items 1 and 3 of "What to do") is not
   yours either, even though it is one of the two: do not write there, and do not write to
   the file it is generated or linked from. Propose instead — for the profile, the lines to
   add to the file they edit; for the shim, the whole file's content — and let the user put
   it in place, including the executable bit. In particular, do not edit the user's shell
   configuration or their `PATH` for them; propose the line and let them add it.
3. **When you cannot read the configuration directory, read the policy from the plan and
   do not write.** Some CLI permission settings deny reading `~/.config`. Then
   `process-wrap --print-plan=full -- true` is your view of the policy: it carries the
   merged policy and the file each item came from. Tell the user that the comments in
   their profile are invisible to you. Never fill the gap from the built-in default or from
   memory. And since you cannot produce a diff of a file you cannot read, do not write to
   the profile even where writing is allowed; propose the lines as in item 2.
4. **Do not list, read, or write anything inside `secrets/`.** The names of the files there
   say which credentials the user has, so listing the directory is reading it. Say where a
   secret file goes (`<configuration directory>/secrets/<name>`), that the directory is
   created with mode 0700 and is always hidden inside the isolation, and that the profile
   refers to it as `${config_dir}/secrets/<name>`. The user puts the value there; you never
   see it, and you never repeat a secret value back. What this protects is the value and
   the set of names; the variable names and paths the profile and the plan print are not
   secret. Whether a secret file is in place you learn from the launch output, when the
   profile has a `secrets` entry for it: a `process-wrap: warning:` line means the file is
   missing, no warning and no diagnostic means it is there, and a diagnostic of kind
   `secret` means it is there but its value is unusable. When the profile has no `secrets`
   entry — the built-in default has none — the launch tells you nothing, so ask the user.
5. **Do not edit the specification or the public documentation of `process-wrap`** (the
   README and the pages under `docs/`). They describe the product, not this machine. If
   something on this machine cannot be expressed in the policy format, say so and stop; do
   not work around it by changing the documents.
6. **Do not edit the body of the shim template — the part below the tool section — neither
   in the bundled template nor in the user's copy.** What the body cannot copy (item 3 of
   "What to do") is worked around, not patched in.
