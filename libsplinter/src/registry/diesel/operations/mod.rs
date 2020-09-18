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

//! Provides database operations for the `DieselRegistry`.

pub(super) mod count_nodes;
pub(super) mod delete_node;
pub(super) mod fetch_node;
pub(super) mod has_node;
pub(super) mod insert_node;
pub(super) mod list_nodes;

use diesel::{
    backend::Backend,
    dsl::{exists, not},
    prelude::*,
    query_builder::BoxedSelectStatement,
};

use crate::registry::MetadataPredicate;

use super::schema::{splinter_nodes, splinter_nodes_metadata};

pub struct RegistryOperations<'a, C> {
    conn: &'a C,
}

impl<'a, C: diesel::Connection> RegistryOperations<'a, C> {
    pub fn new(conn: &'a C) -> Self {
        RegistryOperations { conn }
    }
}

/// Produces a Diesel `SELECT` statement on the `splinter_nodes` table, filtered by the given
/// metadata predicates.
fn select_nodes_by_metadata_predicate<'a, DB: Backend + 'a>(
    predicates: &'a [MetadataPredicate],
) -> BoxedSelectStatement<
    'a,
    // These two `Text` types refers to the columns of the `splinter_nodes` table
    (diesel::sql_types::Text, diesel::sql_types::Text),
    splinter_nodes::table,
    DB,
> {
    let mut query = splinter_nodes::table.into_boxed();

    for predicate in predicates {
        match predicate {
            MetadataPredicate::Eq(key, val) => {
                query = query.filter(exists(
                    splinter_nodes_metadata::table.filter(
                        splinter_nodes_metadata::identity
                            .eq(splinter_nodes::identity)
                            .and(splinter_nodes_metadata::key.eq(key))
                            .and(splinter_nodes_metadata::value.eq(val)),
                    ),
                ));
            }
            MetadataPredicate::Ne(key, val) => {
                query = query.filter(
                    // If the metadata key is not set for a node, the predicate is
                    // satisfied
                    not(exists(
                        splinter_nodes_metadata::table.filter(
                            splinter_nodes_metadata::identity
                                .eq(splinter_nodes::identity)
                                .and(splinter_nodes_metadata::key.eq(key)),
                        ),
                    ))
                    .or(exists(
                        splinter_nodes_metadata::table.filter(
                            splinter_nodes_metadata::identity
                                .eq(splinter_nodes::identity)
                                .and(splinter_nodes_metadata::key.eq(key))
                                .and(splinter_nodes_metadata::value.ne(val)),
                        ),
                    )),
                );
            }
            MetadataPredicate::Gt(key, val) => {
                query = query.filter(exists(
                    splinter_nodes_metadata::table.filter(
                        splinter_nodes_metadata::identity
                            .eq(splinter_nodes::identity)
                            .and(splinter_nodes_metadata::key.eq(key))
                            .and(splinter_nodes_metadata::value.gt(val)),
                    ),
                ));
            }
            MetadataPredicate::Ge(key, val) => {
                query = query.filter(exists(
                    splinter_nodes_metadata::table.filter(
                        splinter_nodes_metadata::identity
                            .eq(splinter_nodes::identity)
                            .and(splinter_nodes_metadata::key.eq(key))
                            .and(splinter_nodes_metadata::value.ge(val)),
                    ),
                ));
            }
            MetadataPredicate::Lt(key, val) => {
                query = query.filter(exists(
                    splinter_nodes_metadata::table.filter(
                        splinter_nodes_metadata::identity
                            .eq(splinter_nodes::identity)
                            .and(splinter_nodes_metadata::key.eq(key))
                            .and(splinter_nodes_metadata::value.lt(val)),
                    ),
                ));
            }
            MetadataPredicate::Le(key, val) => {
                query = query.filter(exists(
                    splinter_nodes_metadata::table.filter(
                        splinter_nodes_metadata::identity
                            .eq(splinter_nodes::identity)
                            .and(splinter_nodes_metadata::key.eq(key))
                            .and(splinter_nodes_metadata::value.le(val)),
                    ),
                ));
            }
        }
    }

    query
}
