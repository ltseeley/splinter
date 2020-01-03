// Copyright 2019 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module provides a common interface that various Splinter components use to expose CLI
//! functionality.

mod error;

use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;

use libc;

pub use error::Error;

/// An `Action` represents a single CLI operation.
pub trait Action {
    /// Run the CLI Action with the given arguments.
    fn run(&mut self, args: &dyn Arguments) -> Result<(), Error>;
}

/// All arguments that are specified for the given `Action`.
pub trait Arguments {
    /// Get the value of an option or positional argument that takes a single value, if present; if
    /// not present, return `None`.
    fn value_of(&self, arg_name: &str) -> Option<&str>;

    /// Get the values of an option or positional argument that takes multiple values, if present;
    /// if not present, return `None`.
    fn values_of(&self, arg_name: &str) -> Option<Vec<&str>>;

    /// Determine if an option or positional argument is present.
    fn is_present(&self, arg_name: &str) -> bool;

    /// Get the name of the subcommand and the arguments for the subcommand if a subcommand was
    /// used; otherwise, return `None`.
    fn subcommand<'a, 'b: 'a>(&'b self) -> Option<(&str, Box<dyn Arguments + 'a>)>;
}

/// A collection of subcommands associated with a single parent command.
#[derive(Default)]
pub struct SubcommandActions<'a> {
    actions: HashMap<String, Box<dyn Action + 'a>>,
}

impl<'a> SubcommandActions<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a subcommand.
    pub fn with_command<'action: 'a, A: Action + 'action>(
        mut self,
        command: &str,
        action: A,
    ) -> Self {
        self.actions.insert(command.to_string(), Box::new(action));

        self
    }
}

impl<'s> Action for SubcommandActions<'s> {
    fn run<'a>(&mut self, args: &dyn Arguments) -> Result<(), Error> {
        let (subcommand, args) = args
            .subcommand()
            .ok_or_else(|| Error("no subcommands found".into()))?;
        let action = self
            .actions
            .get_mut(subcommand)
            .ok_or_else(|| Error("invalid subcommand".into()))?;
        action.run(&*args)
    }
}

/// Provides a convenient way for `Action`s to chown a file.
pub fn chown(path: &Path, uid: u32, gid: u32) -> Result<(), Error> {
    let pathstr = path
        .to_str()
        .ok_or_else(|| Error(format!("Cannot chown file; invalid path: {:?}", path)))?;
    let cpath = CString::new(pathstr).map_err(|err| Error(format!("{}", err)))?;
    let result = unsafe { libc::chown(cpath.as_ptr(), uid, gid) };
    match result {
        0 => Ok(()),
        code => Err(Error(format!("Error chowning file {}: {}", pathstr, code))),
    }
}
