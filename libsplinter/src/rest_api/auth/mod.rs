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

use actix_web::dev::*;
use actix_web::{
    error::ErrorBadRequest, http::header::LOCATION, web, Error as ActixError, FromRequest,
    HttpMessage, HttpRequest, HttpResponse,
};
use futures::{
    future::{ok, FutureResult},
    Future, IntoFuture, Poll,
};
use oauth2::{
    basic::{BasicClient, BasicTokenResponse},
    reqwest::http_client,
    AuthorizationCode, CsrfToken, Scope, TokenResponse,
};
use reqwest::blocking::Client;

use super::AppState;

// TODO: use PKCE
fn user_auth_request(client: &BasicClient, scopes: Vec<Scope>) -> HttpResponse {
    let mut auth_request = client.authorize_url(CsrfToken::new_random);
    for scope in scopes.into_iter() {
        auth_request = auth_request.add_scope(scope);
    }
    let (authorize_url, _csrf_state) = auth_request.url();

    HttpResponse::Found()
        .header(LOCATION, authorize_url.to_string())
        .finish()
}

pub fn github_login(data: web::Data<AppState>) -> HttpResponse {
    user_auth_request(&data.github_oauth, vec![Scope::new("user:email".into())])
}

pub fn google_login(data: web::Data<AppState>, query: web::Query<AuthTypeQuery>) -> HttpResponse {
    let auth_type: AuthType = query.auth_type.parse().expect("Invalid auth type");
    match auth_type {
        AuthType::Client => {
            let token = data
                .google_oauth
                .exchange_client_credentials()
                .request(http_client)
                .expect("Token request failed");

            info!("Got new access token: {}", token.access_token().secret());

            HttpResponse::Ok().body(token.access_token().secret())
        }
        // TBD: these scopes (and the response) are OpenID Connect standards; we may be able to
        // generalize this further for other auth providers
        AuthType::User => user_auth_request(
            &data.google_oauth,
            vec![
                Scope::new("openid".into()),
                Scope::new("profile".into()),
                Scope::new("email".into()),
            ],
        ),
    }
}

#[derive(Deserialize)]
pub struct AuthTypeQuery {
    auth_type: String,
}

#[derive(PartialEq)]
pub enum AuthType {
    Client,
    User,
}

impl std::str::FromStr for AuthType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "client" => Ok(Self::Client),
            "user" => Ok(Self::User),
            _ => Err("invalid auth type".into()),
        }
    }
}

fn get_user_token(client: &BasicClient, query: web::Query<AuthRequest>) -> BasicTokenResponse {
    // TODO: Verify state (for CSRF security)

    // Exchange the auth code for an access token
    let code = AuthorizationCode::new(query.code.clone());
    client
        .exchange_code(code)
        .request(http_client)
        .expect("Token request failed")
}

pub fn github_auth(data: web::Data<AppState>, query: web::Query<AuthRequest>) -> HttpResponse {
    let token = get_user_token(&data.github_oauth, query);

    let email = get_github_email(&format!(
        "{} {}",
        token.token_type().as_ref(),
        token.access_token().secret()
    ));

    // TODO: store token and/or email in Biome (if enabled)

    info!("Got user's email: {:?}", email);

    // TBD: should this response keep the OAuth response structure (token, expires_in, refresh token, etc.)? Should this not be returned when using Biome?
    HttpResponse::Ok().body(token.access_token().secret())
}

pub fn google_auth(data: web::Data<AppState>, query: web::Query<AuthRequest>) -> HttpResponse {
    let token = get_user_token(&data.google_oauth, query);

    let email = get_github_email(&format!(
        "{} {}",
        token.token_type().as_ref(),
        token.access_token().secret()
    ));

    // TODO: store token and/or email in Biome (if enabled)

    // TBD: how to refresh tokens?

    info!("Got user's email: {:?}", email);

    // TBD: should this response keep the OAuth response structure (token, expires_in, refresh token, etc.)? Should this not be returned when using Biome?
    HttpResponse::Ok().body(token.access_token().secret())
}

#[derive(Deserialize)]
pub struct AuthRequest {
    code: String,
}

pub fn test(identity: Identity) -> HttpResponse {
    // Do endpoint-specific access control
    match identity {
        Identity::User(email) if email == "ltseeley@gmail.com" => {
            HttpResponse::Ok().body("You are authenticated")
        }
        _ => HttpResponse::Unauthorized().finish(),
    }
}

pub struct Authorization;

impl<S, B> Transform<S> for Authorization
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = S::Error;
    type InitError = ();
    type Transform = AuthorizationMiddleware<S>;
    type Future = FutureResult<Self::Transform, Self::InitError>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuthorizationMiddleware { service })
    }
}

pub struct AuthorizationMiddleware<S> {
    service: S,
}

impl<S, B> Service for AuthorizationMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = S::Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        // TODO: If biome is enabled, check for authorization there

        // If this call is being made to an authentication endpoint, there's no need to be testing
        // authentication yet
        if !req.path().starts_with("/oauth") {
            // Get the authorization string or redirect to login
            let auth_str = match req.headers().get("Authorization") {
                Some(auth) => auth
                    .to_str()
                    .expect("Authorization header has non-string value"),
                None => {
                    return Box::new(
                        req.into_response(
                            HttpResponse::Found()
                                .header(LOCATION, "/login".to_string())
                                .finish()
                                .into_body(),
                        )
                        .into_future(),
                    )
                }
            };

            // Get the requester's identity
            let identity = Identity::User(get_github_email(auth_str));

            // TODO: REST API-wide access control can be added here

            // Add the identity to the request for endpoint-specific access control
            req.extensions_mut().insert(identity);
        }

        Box::new(self.service.call(req))
    }
}

fn get_github_email(auth_str: &str) -> String {
    Client::builder()
        .build()
        .expect("Failed to build reqwest client")
        .get("https://api.github.com/user/emails")
        .header("Authorization", auth_str)
        // NOTE: according to github docs, the user agent should be set to the name of the app (https://developer.github.com/v3/#user-agent-required)
        .header("User-Agent", "splinter demo")
        .send()
        .expect("Failed to make org request")
        .error_for_status()
        .expect("Org request got err response code")
        .json::<Vec<EmailResponse>>()
        .expect("Failed to parse org response")
        .into_iter()
        .find(|email| email.primary)
        .expect("No primary email for user")
        .email
}

#[derive(Debug, Deserialize)]
struct EmailResponse {
    email: String,
    primary: bool,
}

#[derive(Clone)]
pub enum Identity {
    User(String),
}

impl FromRequest for Identity {
    type Config = ();
    type Error = ActixError;
    type Future = Box<dyn Future<Item = Self, Error = Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        Box::new(
            req.extensions()
                .get::<Identity>()
                .cloned()
                .ok_or_else(|| ErrorBadRequest("missing identity extension"))
                .into_future(),
        )
    }
}

// TBD: could be generic on identity type, would cover key perimission manager
// pub trait AuthorizationStore {
//     fn is_authorized(identity: &Identity, role: &str) -> bool;
// }
//
// struct MockAuthorizationStore {
//     is_authorized: bool,
// }
//
// impl AuthorizationStore for MockAuthorizationStore {
//     fn is_authorized(_identity: &Identity, _role: &str) -> bool {
//         self.is_authorized
//     }
// }
