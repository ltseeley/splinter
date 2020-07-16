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

#[cfg(feature = "sqlite")]
pub mod sqlite;

pub trait StoreFactory {
    #[cfg(feature = "biome-credentials")]
    fn get_credentials_store(&self) -> Box<dyn crate::biome::credentials::store::CredentialsStore>;
}

pub fn create_store_factory(connection_string: &str) -> Box<dyn StoreFactory> {
    if connection_string == "mem" {
        unimplemented!();
    }

    #[cfg(feature = "diesel")]
    {
        use diesel::r2d2::{ConnectionManager, Pool};

        #[cfg(feature = "postgres")]
        if connection_string.starts_with("postgres://") {
            unimplemented!();
        }

        #[cfg(feature = "sqlite")]
        {
            let connection_manager =
                ConnectionManager::<diesel::sqlite::SqliteConnection>::new(connection_string);
            let pool = Pool::builder()
                .build(connection_manager)
                .expect("Failed to build connection pool");
            return Box::new(sqlite::SqliteStoreFactory::new(pool));
        }
    }

    panic!(
        "No supported impelementation provided for connection string '{}'",
        connection_string
    );
}
