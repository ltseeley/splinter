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

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use diesel::{connection::Connection as _, pg::PgConnection};

use crate::error::CliError;

pub const SUBCMD: &str = "database";
const ABOUT: &str = "Manage Gameroom database";

const URL_ARG: &str = "database-url";
const URL_HELP: &str = "database connection URL";
const URL_DEFAULT: &str = "postgres://gameroom:gameroom_example@postgres:5432/gameroom";

const MIGRATE_SUBCMD: &str = "migrate";
const MIGRATE_ABOUT: &str = "Run database migrations for the Gameroom database";

embed_migrations!("../database/migrations");

pub fn get_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(SUBCMD)
        .about(ABOUT)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(
            Arg::with_name(URL_ARG)
                .help(URL_HELP)
                .long(URL_ARG)
                .global(true)
                .default_value(URL_DEFAULT)
                .takes_value(true),
        )
        .subcommand(SubCommand::with_name(MIGRATE_SUBCMD).about(MIGRATE_ABOUT))
}

pub fn handle_database_subcommand(matches: &ArgMatches) -> Result<(), CliError> {
    match matches.subcommand() {
        (MIGRATE_SUBCMD, Some(matches)) => migrate_database(matches),
        (unknown_subcmd, _) => Err(CliError::InvalidSubcommand(unknown_subcmd.into())),
    }
}

fn migrate_database(matches: &ArgMatches) -> Result<(), CliError> {
    let url = matches
        .value_of(URL_ARG)
        .ok_or_else(|| CliError::MissingArgument(URL_ARG.into()))?;

    let conn = PgConnection::establish(url).map_err(|err| {
        CliError::CommandFailed(format!("failed to establish database connection: {}", err))
    })?;

    embedded_migrations::run(&conn).map_err(|err| {
        CliError::CommandFailed(format!("failed to run database migrations: {}", err))
    })?;

    info!("Database migrations applied successfully");

    Ok(())
}
