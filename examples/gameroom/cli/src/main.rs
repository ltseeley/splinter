// Copyright 2020 Cargill Incorporated
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

#[macro_use]
extern crate log;

mod action;
mod error;

use clap::{App, AppSettings, Arg};
use flexi_logger::{LogSpecBuilder, Logger};

use error::CliError;

const APP_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = "Cargill";
const ABOUT: &str = "Command line for Gameroom";

const VERBOSE_ARG: &str = "verbose";
const VERBOSE_ARG_HELP: &str = "log verbosely";
const VERBOSE_ARG_SHORT: &str = "v";

fn main() {
    if let Err(e) = run() {
        error!("ERROR: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), CliError> {
    let app = App::new(APP_NAME)
        .version(VERSION)
        .author(AUTHOR)
        .about(ABOUT)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(
            Arg::with_name(VERBOSE_ARG)
                .help(VERBOSE_ARG_HELP)
                .short(VERBOSE_ARG_SHORT)
                .multiple(true),
        );

    let matches = app.get_matches();

    let log_level = match matches.occurrences_of(VERBOSE_ARG) {
        0 => log::LevelFilter::Info,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    setup_logging(log_level);

    Ok(())
}

fn setup_logging(log_level: log::LevelFilter) {
    let mut log_spec_builder = LogSpecBuilder::new();
    log_spec_builder.default(log_level);

    Logger::with(log_spec_builder.build())
        .format(log_format)
        .start()
        .expect("Failed to create logger");
}

// log format for cli that will only show the log message
pub fn log_format(
    w: &mut dyn std::io::Write,
    _now: &mut flexi_logger::DeferredNow,
    record: &log::Record,
) -> Result<(), std::io::Error> {
    write!(w, "{}", record.args(),)
}
