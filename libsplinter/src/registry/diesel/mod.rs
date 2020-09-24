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
//! [`DieselRegistry`]: ../struct.DieselRegistry.html
//! [`RwRegistry`]: ../trait.RwRegistry.html

pub mod migrations;
mod models;
mod operations;
mod schema;

use diesel::r2d2::{ConnectionManager, Pool};

use super::{
    MetadataPredicate, Node, NodeIter, RegistryError, RegistryReader, RegistryWriter, RwRegistry,
};

use operations::count_nodes::RegistryCountNodesOperation as _;
use operations::delete_node::RegistryDeleteNodeOperation as _;
use operations::fetch_node::RegistryFetchNodeOperation as _;
use operations::has_node::RegistryHasNodeOperation as _;
use operations::insert_node::RegistryInsertNodeOperation as _;
use operations::list_nodes::RegistryListNodesOperation as _;
use operations::RegistryOperations;

/// A database-backed registry, powered by [`Diesel`](https://crates.io/crates/diesel).
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

#[cfg(feature = "postgres")]
impl Clone for DieselRegistry<diesel::pg::PgConnection> {
    fn clone(&self) -> Self {
        Self {
            connection_pool: self.connection_pool.clone(),
        }
    }
}

#[cfg(feature = "sqlite")]
impl Clone for DieselRegistry<diesel::sqlite::SqliteConnection> {
    fn clone(&self) -> Self {
        Self {
            connection_pool: self.connection_pool.clone(),
        }
    }
}

impl<C> RegistryReader for DieselRegistry<C>
where
    C: diesel::Connection,
    i64: diesel::deserialize::FromSql<diesel::sql_types::BigInt, C::Backend>,
    String: diesel::deserialize::FromSql<diesel::sql_types::Text, C::Backend>,
{
    fn list_nodes<'a, 'b: 'a>(
        &'b self,
        predicates: &'a [MetadataPredicate],
    ) -> Result<NodeIter<'a>, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?)
            .list_nodes(predicates)
            .map(|nodes| Box::new(nodes.into_iter()) as NodeIter<'a>)
    }

    fn count_nodes(&self, predicates: &[MetadataPredicate]) -> Result<u32, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).count_nodes(predicates)
    }

    fn fetch_node(&self, identity: &str) -> Result<Option<Node>, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).fetch_node(identity)
    }

    fn has_node(&self, identity: &str) -> Result<bool, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).has_node(identity)
    }
}

#[cfg(feature = "postgres")]
impl RegistryWriter for DieselRegistry<diesel::pg::PgConnection> {
    fn insert_node(&self, node: Node) -> Result<(), RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).insert_node(node)
    }

    fn delete_node(&self, identity: &str) -> Result<Option<Node>, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).delete_node(identity)
    }
}

#[cfg(feature = "sqlite")]
impl RegistryWriter for DieselRegistry<diesel::sqlite::SqliteConnection> {
    fn insert_node(&self, node: Node) -> Result<(), RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).insert_node(node)
    }

    fn delete_node(&self, identity: &str) -> Result<Option<Node>, RegistryError> {
        RegistryOperations::new(&*self.connection_pool.get()?).delete_node(identity)
    }
}

#[cfg(feature = "postgres")]
impl RwRegistry for DieselRegistry<diesel::pg::PgConnection>
where
    String: diesel::deserialize::FromSql<diesel::sql_types::Text, diesel::pg::Pg>,
{
    fn clone_box(&self) -> Box<dyn RwRegistry> {
        Box::new(self.clone())
    }

    fn clone_box_as_reader(&self) -> Box<dyn RegistryReader> {
        Box::new(self.clone())
    }

    fn clone_box_as_writer(&self) -> Box<dyn RegistryWriter> {
        Box::new(self.clone())
    }
}

#[cfg(feature = "sqlite")]
impl RwRegistry for DieselRegistry<diesel::sqlite::SqliteConnection>
where
    String: diesel::deserialize::FromSql<diesel::sql_types::Text, diesel::sqlite::Sqlite>,
{
    fn clone_box(&self) -> Box<dyn RwRegistry> {
        Box::new(self.clone())
    }

    fn clone_box_as_reader(&self) -> Box<dyn RegistryReader> {
        Box::new(self.clone())
    }

    fn clone_box_as_writer(&self) -> Box<dyn RegistryWriter> {
        Box::new(self.clone())
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;

    use crate::registry::{diesel::migrations::run_sqlite_migrations, tests::*};

    use diesel::sqlite::SqliteConnection;

    /// Verifies the correct functionality of the `fetch_node` method for
    /// `DieselRegistry<SqliteConnection>`.
    #[test]
    fn fetch_node() {
        let registry = DieselRegistry::new(create_connection_pool_and_migrate());

        registry
            .insert_node(get_node_1())
            .expect("Failed to insert node");

        test_fetch_node(&registry, &get_node_1())
    }

    /// Verifies the correct functionality of the `has_node` method for
    /// `DieselRegistry<SqliteConnection>`.
    #[test]
    fn has_node() {
        let registry = DieselRegistry::new(create_connection_pool_and_migrate());

        registry
            .insert_node(get_node_1())
            .expect("Failed to insert node");

        test_has_node(&registry, &get_node_1().identity)
    }

    /// Verifies the correct functionality of the `list_nodes` method without metadata predicates
    /// for `DieselRegistry<SqliteConnection>`.
    #[test]
    fn list_nodes_without_predicates() {
        let registry = DieselRegistry::new(create_connection_pool_and_migrate());

        registry
            .insert_node(get_node_1())
            .expect("Failed to insert node1");
        registry
            .insert_node(get_node_2())
            .expect("Failed to insert node2");

        test_list_nodes_without_predicates(&registry, &[get_node_1(), get_node_2()])
    }

    /// Verifies the correct functionality of the `list_nodes` method when there are no nodes in the
    /// `DieselRegistry<SqliteConnection>`.
    #[test]
    fn list_nodes_empty() {
        let registry = DieselRegistry::new(create_connection_pool_and_migrate());

        test_list_nodes_empty(&registry)
    }

    /// Creates a conneciton pool for an in-memory SQLite database with only a single connection
    /// available. Each connection is backed by a different in-memory SQLite database, so limiting
    /// the pool to a single connection insures that the same DB is used for all operations.
    fn create_connection_pool_and_migrate() -> Pool<ConnectionManager<SqliteConnection>> {
        let connection_manager = ConnectionManager::<SqliteConnection>::new(":memory:");
        let pool = Pool::builder()
            .max_size(1)
            .build(connection_manager)
            .expect("Failed to build connection pool");

        run_sqlite_migrations(&*pool.get().expect("Failed to get connection for migrations"))
            .expect("Failed to run migrations");

        pool
    }

    fn get_node_1() -> Node {
        Node::builder("Node-123")
            .with_endpoint("tcps://12.0.0.123:8431")
            .with_display_name("Bitwise IO - Node 1")
            .with_key("abcd")
            .with_metadata("company", "Bitwise IO")
            .with_metadata("admin", "Bob")
            .build()
            .expect("Failed to build node1")
    }

    fn get_node_2() -> Node {
        Node::builder("Node-456")
            .with_endpoint("tcps://12.0.0.123:8434")
            .with_display_name("Cargill - Node 1")
            .with_key("0123")
            .with_metadata("company", "Cargill")
            .with_metadata("admin", "Carol")
            .build()
            .expect("Failed to build node2")
    }

    fn get_node_3() -> Node {
        Node::builder("Node-789")
            .with_endpoint("tcps://12.0.0.123:8435")
            .with_display_name("Cargill - Node 2")
            .with_key("4567")
            .with_metadata("company", "Cargill")
            .with_metadata("admin", "Charlie")
            .build()
            .expect("Failed to build node3")
    }
}
