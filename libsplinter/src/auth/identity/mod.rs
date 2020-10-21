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

//! Tools for identifying clients and users

mod error;
#[cfg(feature = "oauth-github")]
pub mod github;

pub use error::IdentityProviderError;

/// A service that fetches identities from a backing provider
pub trait IdentityProvider: Send {
    /// Attempts to get the identity that corresponds to the given credentials
    fn get_identity(&self, credentials: &AuthCredentials) -> Result<String, IdentityProviderError>;

    /// Clones implementation for `IdentityProvider`. The implementation of the `Clone` trait for
    /// `Box<dyn IdentityProvider>` calls this method.
    ///
    /// # Example
    ///
    ///```ignore
    ///  fn clone_box(&self) -> Box<dyn IdentityProvider> {
    ///     Box::new(self.clone())
    ///  }
    ///```
    fn clone_box(&self) -> Box<dyn IdentityProvider>;
}

impl Clone for Box<dyn IdentityProvider> {
    fn clone(&self) -> Box<dyn IdentityProvider> {
        self.clone_box()
    }
}

/// The authentication credentials that are passed to an `IdentityProvider`
pub struct AuthCredentials {
    /// The authentication type. Each identity provider may support only certain types.
    auth_type: String,
    /// The credentials of the client. The format of this string is specific to the authentication
    /// type.
    value: String,
}

impl AuthCredentials {
    /// Creates new
    pub fn new(auth_type: String, value: String) -> Self {
        Self { auth_type, value }
    }

    /// Gets the authentication type
    pub fn auth_type(&self) -> &str {
        &self.auth_type
    }

    /// Gets the value of the credentials
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the `AuthCredentials` struct returns the correct values.
    #[test]
    fn auth_credentials() {
        let credentials = AuthCredentials::new("auth_type".into(), "value".into());

        assert_eq!(credentials.auth_type(), "auth_type");
        assert_eq!(credentials.value(), "value");
    }
}
