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

use reqwest;
use serde_json::Value;
use splinter::cli::{Action, Arguments, Error};

pub struct StatusAction;

impl Action for StatusAction {
    fn run<'a>(&mut self, args: &dyn Arguments) -> Result<(), Error> {
        let url = args.value_of("url").unwrap_or("http://localhost:8085");

        let status: Value = reqwest::get(&format!("{}/health/status", url))
            .and_then(|mut res| res.json())
            .map_err(|err| Error(format!("Status request failed: {:?}", err)))?;

        println!(
            "{}",
            serde_json::to_string_pretty(&status)
                .map_err(|err| Error(format!("Failed to deserialize response: {}", err)))?
        );
        Ok(())
    }
}
