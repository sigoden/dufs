use std::{collections::HashMap, str::FromStr};

use crate::{auth::get_auth_user, server::Request};

pub const DEFAULT_LOG_FORMAT: &str = r#"$remote_addr "$request" $status"#;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpLogger {
    elements: Vec<LogElement>,
}

impl Default for HttpLogger {
    fn default() -> Self {
        DEFAULT_LOG_FORMAT.parse().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum LogElement {
    Variable(String),
    Header(String),
    Literal(String),
}

impl HttpLogger {
    pub fn data(&self, req: &Request) -> HashMap<String, String> {
        let mut data = HashMap::default();
        for element in self.elements.iter() {
            match element {
                LogElement::Variable(name) => match name.as_str() {
                    "request" => {
                        data.insert(name.to_string(), format!("{} {}", req.method(), req.uri()));
                    }
                    "remote_user" => {
                        if let Some(user) =
                            req.headers().get("authorization").and_then(get_auth_user)
                        {
                            data.insert(name.to_string(), user);
                        }
                    }
                    _ => {}
                },
                LogElement::Header(name) => {
                    if let Some(value) = req.headers().get(name).and_then(|v| v.to_str().ok()) {
                        data.insert(name.to_string(), value.to_string());
                    }
                }
                LogElement::Literal(_) => {}
            }
        }
        data
    }
    pub fn log(&self, data: &HashMap<String, String>, err: Option<String>) {
        if self.elements.is_empty() {
            return;
        }
        let mut output = String::new();
        for element in self.elements.iter() {
            match element {
                LogElement::Literal(value) => output.push_str(value.as_str()),
                LogElement::Header(name) | LogElement::Variable(name) => {
                    output.push_str(data.get(name).map(|v| v.as_str()).unwrap_or("-"))
                }
            }
        }
        match err {
            Some(err) => error!("{} {}", output, err),
            None => info!("{}", output),
        }
    }
}

impl FromStr for HttpLogger {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut elements = vec![];
        let mut is_var = false;
        let mut cache = String::new();
        for c in format!("{s} ").chars() {
            if c == '$' {
                if !cache.is_empty() {
                    elements.push(LogElement::Literal(cache.to_string()));
                }
                cache.clear();
                is_var = true;
            } else if is_var && !(c.is_alphanumeric() || c == '_') {
                if let Some(value) = cache.strip_prefix("$http_") {
                    elements.push(LogElement::Header(value.replace('_', "-").to_string()));
                } else if let Some(value) = cache.strip_prefix('$') {
                    elements.push(LogElement::Variable(value.to_string()));
                }
                cache.clear();
                is_var = false;
            }
            cache.push(c);
        }
        let cache = cache.trim();
        if !cache.is_empty() {
            elements.push(LogElement::Literal(cache.to_string()));
        }
        Ok(Self { elements })
    }
}
