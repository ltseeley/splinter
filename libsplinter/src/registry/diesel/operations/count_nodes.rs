// Copyright 2018-2020 Cargill Incorporated
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

//! Provides the "count nodes" operation for the `DieselRegistry`.

use diesel::prelude::*;

use crate::registry::{MetadataPredicate, RegistryError};

use super::{select_nodes_by_metadata_predicate, RegistryOperations};

pub(in crate::registry::diesel) trait RegistryCountNodesOperation {
    fn count_nodes(&self, predicates: &[MetadataPredicate]) -> Result<u32, RegistryError>;
}

impl<'a, C> RegistryCountNodesOperation for RegistryOperations<'a, C>
where
    C: diesel::Connection,
    i64: diesel::deserialize::FromSql<diesel::sql_types::BigInt, C::Backend>,
{
    fn count_nodes(&self, predicates: &[MetadataPredicate]) -> Result<u32, RegistryError> {
        select_nodes_by_metadata_predicate(predicates)
            .count()
            // Parse as an i64 here because Diesel knows how to convert a `BigInt` into an i64
            .get_result::<i64>(self.conn)
            .map(|count| count as u32)
            .map_err(|err| {
                RegistryError::general_error_with_source("Failed to count all nodes", Box::new(err))
            })
    }
}
