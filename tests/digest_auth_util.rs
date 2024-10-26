/// Refs https://github.dev/maoertel/diqwest/blob/main/src/blocking.rs
use anyhow::{anyhow, Result};
use digest_auth::{AuthContext, AuthorizationHeader, HttpMethod};
use hyper::{header::AUTHORIZATION, HeaderMap, StatusCode};
use reqwest::blocking::{RequestBuilder, Response};
use url::Position;

pub fn send_with_digest_auth(
    request_builder: RequestBuilder,
    username: &str,
    password: &str,
) -> Result<Response> {
    let first_response = try_clone_request_builder(&request_builder)?.send()?;
    match first_response.status() {
        StatusCode::UNAUTHORIZED => {
            try_digest_auth(request_builder, first_response, username, password)
        }
        _ => Ok(first_response),
    }
}

fn try_digest_auth(
    request_builder: RequestBuilder,
    first_response: Response,
    username: &str,
    password: &str,
) -> Result<Response> {
    if let Some(answer) = get_answer(
        &request_builder,
        first_response.headers(),
        username,
        password,
    )? {
        return Ok(request_builder
            .header(AUTHORIZATION, answer.to_header_string())
            .send()?);
    };

    Ok(first_response)
}

fn try_clone_request_builder(request_builder: &RequestBuilder) -> Result<RequestBuilder> {
    request_builder
        .try_clone()
        .ok_or_else(|| anyhow!("Request body must not be a stream"))
}

fn get_answer(
    request_builder: &RequestBuilder,
    first_response: &HeaderMap,
    username: &str,
    password: &str,
) -> Result<Option<AuthorizationHeader>> {
    let answer = calculate_answer(request_builder, first_response, username, password);
    match answer {
        Ok(answer) => Ok(Some(answer)),
        Err(error) => Err(error),
    }
}

fn calculate_answer(
    request_builder: &RequestBuilder,
    headers: &HeaderMap,
    username: &str,
    password: &str,
) -> Result<AuthorizationHeader> {
    let request = try_clone_request_builder(request_builder)?.build()?;
    let path = &request.url()[Position::AfterPort..];
    let method = HttpMethod::from(request.method().as_str());
    let body = request.body().and_then(|b| b.as_bytes());

    parse_digest_auth_header(headers, path, method, body, username, password)
}

fn parse_digest_auth_header(
    header: &HeaderMap,
    path: &str,
    method: HttpMethod,
    body: Option<&[u8]>,
    username: &str,
    password: &str,
) -> Result<AuthorizationHeader> {
    let www_auth = header
        .get("www-authenticate")
        .ok_or_else(|| anyhow!("The header 'www-authenticate' is missing."))?
        .to_str()?;
    let context = AuthContext::new_with_method(username, password, path, body, method);
    let mut prompt = digest_auth::parse(www_auth)?;

    Ok(prompt.respond(&context)?)
}
