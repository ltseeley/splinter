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

//! Actix implemenation of the splinter REST API.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use actix_http::Payload;
use actix_web::{web::Query, FromRequest, HttpRequest, ResponseError};

use super::{Request, RequestBuilder};

impl FromRequest for Request {
    type Error = RequestConversionError;
    type Future = Result<Self, Self::Error>;
    type Config = ();

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let path = req.path().to_string();

        let headers = req
            .headers()
            .iter()
            .map(|(name, value)| {
                let header_name = name.as_str().to_string();
                let header_value = value
                    .to_str()
                    .map_err(|err| {
                        RequestConversionError::new_with_source(
                            &format!("Value of header '{}' is not valid", header_name),
                            err.into(),
                        )
                    })?
                    .to_string();
                Ok((header_name, header_value))
            })
            .collect::<Result<_, _>>()?;

        let path_parameters = req
            .match_info()
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();

        let query_parameters = Query::<BTreeMap<String, String>>::from_query(req.query_string())
            .map(Query::into_inner)
            .map_err(|err| RequestConversionError::new(&format!("Query is invalid: {}", err)))?;

        RequestBuilder::new()
            .with_path(path)
            .with_headers(headers)
            .with_path_parameters(path_parameters)
            .with_query_parameters(query_parameters)
            .with_actix_request(req.clone())
            .build()
            .map_err(|err| {
                RequestConversionError::new_with_source("Failed to convert request", err.into())
            })
    }
}

#[derive(Debug)]
pub struct RequestConversionError {
    context: String,
    source: Option<Box<dyn Error>>,
}

impl RequestConversionError {
    pub fn new(context: &str) -> Self {
        Self {
            context: context.into(),
            source: None,
        }
    }

    pub fn new_with_source(context: &str, err: Box<dyn Error>) -> Self {
        Self {
            context: context.into(),
            source: Some(err),
        }
    }
}

impl Error for RequestConversionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|err| &**err)
    }
}

impl fmt::Display for RequestConversionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.source {
            Some(ref err) => write!(f, "{}: {}", self.context, err),
            None => f.write_str(&self.context),
        }
    }
}

impl ResponseError for RequestConversionError {}