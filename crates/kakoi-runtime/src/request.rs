use std::ffi::OsString;
use std::os::fd::OwnedFd;
use std::path::PathBuf;

use crate::{HostContext, Policy};

pub enum Io {
    Inherit,
    Pipe,
    Null,
    Fd(OwnedFd),
}

pub struct StdioSpec {
    pub stdin: Io,
    pub stdout: Io,
    pub stderr: Io,
}

pub struct CommandSpec {
    pub(crate) program: OsString,
    pub(crate) arguments: Vec<OsString>,
}

impl CommandSpec {
    pub fn new(program: OsString) -> Self {
        Self {
            program,
            arguments: Vec::new(),
        }
    }
    pub fn arg(mut self, argument: OsString) -> Self {
        self.arguments.push(argument);
        self
    }
}

pub struct RunRequest {
    pub(crate) policy: Policy,
    pub(crate) command: CommandSpec,
    pub(crate) context: HostContext,
    pub(crate) stdio: StdioSpec,
    pub(crate) workspace: Option<PathBuf>,
}

impl RunRequest {
    pub fn new(
        policy: Policy,
        command: CommandSpec,
        context: HostContext,
        stdio: StdioSpec,
    ) -> Self {
        Self {
            policy,
            command,
            context,
            stdio,
            workspace: None,
        }
    }
    pub fn workspace(mut self, path: PathBuf) -> Self {
        self.workspace = Some(path);
        self
    }
}
