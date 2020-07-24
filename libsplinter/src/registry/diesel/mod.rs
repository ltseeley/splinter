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

//! A database-backed registry, powered by [`Diesel`](https://crates.io/crates/diesel).
//!
//! This module contains the [`DieselRegistry`], which provides an implementation of the
//! [`RwRegistry`] trait.
//!
//! [`RwRegistry`]: ../../trait.RwRegistry.html

pub mod migrations;
mod models;
mod operations;
mod schema;

use diesel::r2d2::{ConnectionManager, Pool};

use super::{
    validate_nodes, MetadataPredicate, Node, NodeIter, RegistryError, RegistryReader,
    RegistryWriter, RwRegistry,
};

use operations::delete_node::RegistryDeleteNodeOperation as _;
use operations::insert_node::RegistryInsertNodeOperation as _;
// use operations::fetch_user::UserStoreFetchUserOperation as _;
// use operations::list_users::UserStoreListUsersOperation as _;
// use operations::update_user::UserStoreUpdateUserOperation as _;
use operations::RegistryOperations;

/// A database-backed registry, powered by [`Diesel`][diesel].
///
/// TODO
#[derive(Clone)]
pub struct DieselRegistry<C: diesel::Connection + 'static> {
    connection_pool: Pool<ConnectionManager<C>>,
}

impl<C: diesel::Connection> DieselRegistry<C> {
    /// Creates a new `DieselRegistry`.
    ///
    /// # Arguments
    ///
    ///  * `connection_pool`: connection pool for the database
    pub fn new(connection_pool: Pool<ConnectionManager<C>>) -> Self {
        DieselRegistry { connection_pool }
    }
}

// impl RegistryReader for DieselRegistry {
//     fn fetch_node(&self, identity: &str) -> Result<Option<Node>, RegistryError> {
//         Ok(self
//             .get_nodes()?
//             .iter()
//             .find(|node| node.identity == identity)
//             .cloned())
//     }
//
//     fn list_nodes<'a, 'b: 'a>(
//         &'b self,
//         predicates: &'a [MetadataPredicate],
//     ) -> Result<NodeIter<'a>, RegistryError> {
//         let mut nodes = self.get_nodes()?;
//         nodes.retain(|node| predicates.iter().all(|predicate| predicate.apply(node)));
//         Ok(Box::new(nodes.into_iter()))
//     }
//
//     fn count_nodes(&self, predicates: &[MetadataPredicate]) -> Result<u32, RegistryError> {
//         Ok(self
//             .get_nodes()?
//             .iter()
//             .filter(move |node| predicates.iter().all(|predicate| predicate.apply(node)))
//             .count() as u32)
//     }
// }

#[cfg(feature = "postgres")]
impl RegistryWriter for DieselRegistry<diesel::pg::PgConnection> {
    fn insert_node(&self, node: Node) -> Result<(), RegistryError> {
        // TODO: validate node, or in query?
        RegistryOperations::new(&*self.connection_pool.get()?).insert_node(node)
    }

    fn delete_node(&self, identity: &str) -> Result<Option<Node>, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).delete_node(identity)
    }
}

#[cfg(feature = "sqlite")]
impl RegistryWriter for DieselRegistry<diesel::sqlite::SqliteConnection> {
    fn insert_node(&self, node: Node) -> Result<(), RegistryError> {
        // TODO: validate node, or in query?
        RegistryOperations::new(&*self.connection_pool.get()?).insert_node(node)
    }

    fn delete_node(&self, identity: &str) -> Result<Option<Node>, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).delete_node(identity)
    }
}

// impl RwRegistry for DieselRegistry {
//     fn clone_box(&self) -> Box<dyn RwRegistry> {
//         Box::new(self.clone())
//     }
//
//     fn clone_box_as_reader(&self) -> Box<dyn RegistryReader> {
//         Box::new(Clone::clone(self))
//     }
//
//     fn clone_box_as_writer(&self) -> Box<dyn RegistryWriter> {
//         Box::new(Clone::clone(self))
//     }
// }
