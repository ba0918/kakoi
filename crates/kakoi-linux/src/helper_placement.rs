//! bwrap placement of caller-selected helper images.

use std::io;

use crate::bwrap_arguments::operations;
use crate::copies::FileContent;
use crate::plan::{Argument, LaunchLayout, Plan};

pub fn place(plan: &mut Plan, guard_image: Option<FileContent>) -> io::Result<()> {
    if plan.commands.is_some() {
        let root = crate::command_limits::FIRST_ROOT;
        let operations = operations(&plan.arguments)?;
        let start = operations
            .iter()
            .find_map(|(index, args)| {
                matches!(args, [Argument::Literal(option), Argument::Literal(path)]
                if option == "--tmpfs" && path == root)
                .then_some(*index)
            })
            .and_then(|index| index.checked_sub(2))
            .ok_or_else(|| io::Error::other("missing first-process placement"))?;
        let end = operations
            .iter()
            .find_map(|(index, args)| {
                matches!(args, [Argument::Literal(option), Argument::Literal(path)]
                if option == "--remount-ro" && path == root)
                .then_some(index + 2)
            })
            .ok_or_else(|| io::Error::other("missing first-process boundary"))?;
        plan.arguments.drain(start..end);
        shift_layout(&mut plan.launch_layout, end, -((end - start) as isize));
    }
    for entry in &plan.guards.table.entries {
        let image = guard_image
            .as_ref()
            .ok_or_else(|| io::Error::other("missing guard image"))?;
        let index = operations(&plan.arguments)?.into_iter().find_map(|(index, args)| {
            matches!(args, [Argument::Literal(option), _, Argument::Literal(path)]
                if option == "--ro-bind" && std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()) == entry.location).then_some(index)
        }).ok_or_else(|| io::Error::other("missing guard placement"))?;
        plan.arguments[index] = Argument::Literal("--ro-bind-data".into());
        plan.arguments[index + 1] = Argument::CopiedFile(image.clone());
        plan.arguments.splice(
            index..index,
            [
                Argument::Literal("--perms".into()),
                Argument::Literal("0555".into()),
            ],
        );
        shift_layout(&mut plan.launch_layout, index, 2);
    }
    Ok(())
}

pub fn shift_layout(layout: &mut LaunchLayout, from: usize, delta: isize) {
    for position in [
        &mut layout.argv0,
        &mut layout.command_separator,
        &mut layout.resolver_destination,
    ]
    .into_iter()
    .flatten()
    {
        if *position >= from {
            *position = position
                .checked_add_signed(delta)
                .expect("layout shift stays in bounds");
        }
    }
}

pub fn place_init(plan: &mut Plan, image: FileContent, path: &str) {
    plan.arguments
        .truncate(plan.launch_layout.command_separator.unwrap());
    if let Some(index) = plan.launch_layout.argv0 {
        plan.arguments[index] = Argument::Literal(path.into());
    }
    plan.arguments.extend([
        Argument::Literal("--dir".into()),
        Argument::Literal("/dev/kakoi-runtime".into()),
        Argument::Literal("--perms".into()),
        Argument::Literal("0700".into()),
        Argument::Literal("--ro-bind-data".into()),
        Argument::CopiedFile(image),
        Argument::Literal(path.into()),
        Argument::Literal("--as-pid-1".into()),
        Argument::Literal("--".into()),
        Argument::Literal(path.into()),
    ]);
}
