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

//! An identity provider that looks up GitHub usernames

use reqwest::{blocking::Client, StatusCode};

use super::{AuthCredentials, IdentityProvider, IdentityProviderError};

/// Retrieves a GitHub username from the GitHub servers
///
/// The `AuthCredentials` accepted by this provider must have the type `Bearer` and the value must
/// be a valid GitHub OAuth2 access token.
#[derive(Clone)]
pub struct GithubUserIdentityProvider;

impl IdentityProvider for GithubUserIdentityProvider {
    fn get_identity(&self, credentials: &AuthCredentials) -> Result<String, IdentityProviderError> {
        if credentials.auth_type() != "Bearer" {
            return Err(IdentityProviderError::UnsupportedAuthType);
        }

        let response = Client::builder()
            .build()
            .map_err(|err| IdentityProviderError::InternalError(err.to_string()))?
            .get("https://api.github.com/user")
            .header(
                "Authorization",
                format!("{} {}", credentials.auth_type(), credentials.value()),
            )
            .header("User-Agent", "splinter")
            .send()
            .map_err(|err| IdentityProviderError::InternalError(err.to_string()))?;

        if !response.status().is_success() {
            match response.status() {
                StatusCode::UNAUTHORIZED => return Err(IdentityProviderError::Unauthorized),
                status_code => {
                    return Err(IdentityProviderError::InternalError(format!(
                        "Received unexpected response code: {}",
                        status_code
                    )))
                }
            }
        }

        let username = response
            .json::<UserResponse>()
            .map_err(|_| {
                IdentityProviderError::InternalError("Received unexpected response body".into())
            })?
            .login;

        Ok(username)
    }

    fn clone_box(&self) -> Box<dyn IdentityProvider> {
        Box::new(self.clone())
    }
}

/// Deserializes the GitHub response
#[derive(Debug, Deserialize)]
struct UserResponse {
    login: String,
}
