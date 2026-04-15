//! Shared HTTP helpers for analytics API calls.
//! Uses reqwest blocking client (consistent with the rest of the codebase).

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE, USER_AGENT};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Build a client with sensible defaults.
pub fn build_client(timeout_secs: u64) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

/// Perform a GET request with bearer token auth.
pub fn authed_get<T: DeserializeOwned>(
    url: &str,
    bearer_token: &str,
    extra_headers: Option<HeaderMap>,
) -> Result<T, String> {
    let client = build_client(15)?;
    let mut req = client
        .get(url)
        .header(AUTHORIZATION, format!("Bearer {}", bearer_token))
        .header(ACCEPT, "application/json");

    if let Some(headers) = extra_headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }

    let resp = req.send().map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} from {}: {}", status, url, body));
    }
    resp.json::<T>().map_err(|e| format!("JSON parse error: {}", e))
}

/// Perform a POST request with bearer token auth and JSON body.
pub fn authed_post<T: DeserializeOwned>(
    url: &str,
    bearer_token: &str,
    body: &serde_json::Value,
    extra_headers: Option<HeaderMap>,
) -> Result<T, String> {
    let client = build_client(15)?;
    let mut req = client
        .post(url)
        .header(AUTHORIZATION, format!("Bearer {}", bearer_token))
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .json(body);

    if let Some(headers) = extra_headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }

    let resp = req.send().map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} from {}: {}", status, url, body));
    }
    resp.json::<T>().map_err(|e| format!("JSON parse error: {}", e))
}

/// Perform a GET request with cookie auth.
pub fn cookie_get<T: DeserializeOwned>(
    url: &str,
    cookie_header: &str,
    extra_headers: Option<HeaderMap>,
) -> Result<T, String> {
    let client = build_client(15)?;
    let mut req = client
        .get(url)
        .header(COOKIE, cookie_header)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36");

    if let Some(headers) = extra_headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }

    let resp = req.send().map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} from {}: {}", status, url, body));
    }
    resp.json::<T>().map_err(|e| format!("JSON parse error: {}", e))
}

/// Perform a GET request that returns raw text (for HTML scraping).
pub fn cookie_get_text(
    url: &str,
    cookie_header: &str,
) -> Result<String, String> {
    let client = build_client(15)?;
    let resp = client
        .get(url)
        .header(COOKIE, cookie_header)
        .header(USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .header(ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .send()
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {} from {}", status, url));
    }
    resp.text().map_err(|e| format!("Body read error: {}", e))
}

/// Perform a POST request with cookie auth and JSON body.
pub fn cookie_post<T: DeserializeOwned>(
    url: &str,
    cookie_header: &str,
    body: &serde_json::Value,
    extra_headers: Option<HeaderMap>,
) -> Result<T, String> {
    let client = build_client(15)?;
    let mut req = client
        .post(url)
        .header(COOKIE, cookie_header)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .json(body);

    if let Some(headers) = extra_headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }

    let resp = req.send().map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} from {}: {}", status, url, body));
    }
    resp.json::<T>().map_err(|e| format!("JSON parse error: {}", e))
}

/// Perform a POST request with JSON body (no auth).
pub fn post_json<T: DeserializeOwned>(
    url: &str,
    body: &serde_json::Value,
    extra_headers: Option<HeaderMap>,
) -> Result<T, String> {
    let client = build_client(15)?;
    let mut req = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .json(body);

    if let Some(headers) = extra_headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }

    let resp = req.send().map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} from {}: {}", status, url, body));
    }
    resp.json::<T>().map_err(|e| format!("JSON parse error: {}", e))
}

/// Perform a GET request with no auth, returning JSON.
pub fn get_json<T: DeserializeOwned>(
    url: &str,
    extra_headers: Option<HeaderMap>,
) -> Result<T, String> {
    let client = build_client(15)?;
    let mut req = client
        .get(url)
        .header(ACCEPT, "application/json");

    if let Some(headers) = extra_headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }

    let resp = req.send().map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} from {}: {}", status, url, body));
    }
    resp.json::<T>().map_err(|e| format!("JSON parse error: {}", e))
}

/// Build a HeaderMap from key-value pairs.
pub fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in pairs {
        if let Ok(val) = HeaderValue::from_str(v) {
            if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                map.insert(name, val);
            }
        }
    }
    map
}
