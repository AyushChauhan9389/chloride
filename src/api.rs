use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::config::{Config, UserInfo};

// --- Auth ---

pub struct AuthResult {
    pub token: String,
    pub refresh_token: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginUser {
    id: i64,
    email: String,
    role: Option<String>,
    plan: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
    #[serde(rename = "refreshToken", default)]
    refresh_token: String,
    user: LoginUser,
}

pub fn login(email: &str, password: &str, base_url: &str) -> Result<AuthResult> {
    let client = Client::new();
    let url = format!("{}/api/auth/login", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&LoginRequest {
            email: email.to_string(),
            password: password.to_string(),
        })
        .send()
        .context("Failed to connect to server")?;

    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Login failed");
        bail!("{msg}");
    }

    let login: LoginResponse = resp.json().context("Invalid response from server")?;

    Ok(AuthResult {
        token: login.token,
        refresh_token: login.refresh_token,
        user: UserInfo {
            id: login.user.id,
            email: login.user.email,
            role: login.user.role,
            plan: login.user.plan,
        },
    })
}

pub fn register(email: &str, password: &str, base_url: &str) -> Result<AuthResult> {
    let client = Client::new();
    let url = format!("{}/api/auth/signup", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&LoginRequest {
            email: email.to_string(),
            password: password.to_string(),
        })
        .send()
        .context("Failed to connect to server")?;

    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Registration failed");
        bail!("{msg}");
    }

    let signup: LoginResponse = resp.json().context("Invalid response from server")?;

    Ok(AuthResult {
        token: signup.token,
        refresh_token: signup.refresh_token,
        user: UserInfo {
            id: signup.user.id,
            email: signup.user.email,
            role: signup.user.role,
            plan: signup.user.plan,
        },
    })
}

/// Exchange a refresh token for a new access token. Returns the new token
/// and the (possibly updated) user info.
pub fn refresh_access_token(refresh_token: &str, base_url: &str) -> Result<(String, UserInfo)> {
    let client = Client::new();
    let url = format!("{}/api/auth/refresh", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .bearer_auth(refresh_token)
        .send()
        .context("Failed to connect to server")?;

    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Token refresh failed");
        bail!("{msg}");
    }

    #[derive(Deserialize)]
    struct RefreshResp {
        token: String,
        user: LoginUser,
    }
    let r: RefreshResp = resp.json().context("Invalid response from server")?;
    Ok((
        r.token,
        UserInfo {
            id: r.user.id,
            email: r.user.email,
            role: r.user.role,
            plan: r.user.plan,
        },
    ))
}

/// Ensure the config has a valid access token. If the token is missing or
/// known-expired (caller passes `force_refresh`), try to refresh using the
/// stored refresh token. Updates `config` in place and saves it.
///
/// Returns `Ok(token)` on success, or an error if no refresh path exists.
pub fn ensure_token(config: &mut Config) -> Result<String> {
    // Fast path: we still have an access token. Callers detect 401 and retry.
    if let Some(token) = config.token.clone() {
        return Ok(token);
    }

    // No access token — try the refresh token.
    if let Some(refresh) = config.refresh_token.clone() {
        let (new_token, user) = refresh_access_token(&refresh, &config.base_url)?;
        config.token = Some(new_token.clone());
        config.user = Some(user);
        let _ = config.save();
        return Ok(new_token);
    }

    bail!("Not logged in. Run `cl login`.");
}

/// Called by blocking helpers when they get a 401: try to refresh and update
/// config. Returns the new token on success.
fn refresh_on_401(config: &mut Config) -> Result<String> {
    config.token = None; // clear the bad token so ensure_token will refresh
    let Some(refresh) = config.refresh_token.clone() else {
        bail!("Session expired. Please log in again.");
    };
    let (new_token, user) = refresh_access_token(&refresh, &config.base_url)?;
    config.token = Some(new_token.clone());
    config.user = Some(user);
    let _ = config.save();
    Ok(new_token)
}

// --- Storage ---

const UNLIMITED_SENTINEL: i64 = -1;

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(non_snake_case, dead_code)]
pub struct StorageInfo {
    pub used: i64,
    pub left: i64,
    pub limit: i64,
    pub percentageUsed: f64,
    #[serde(rename = "usedFormatted")]
    pub used_formatted: Option<String>,
    #[serde(rename = "leftFormatted")]
    pub left_formatted: Option<String>,
    #[serde(rename = "limitFormatted")]
    pub limit_formatted: Option<String>,
}

impl StorageInfo {
    pub fn is_unlimited(&self) -> bool {
        self.limit == UNLIMITED_SENTINEL
    }
}

pub fn get_storage(config: &mut Config) -> Result<StorageInfo> {
    let token = ensure_token(config)?;
    let url = format!("{}/api/plans/user/storage", config.base_url.trim_end_matches('/'));

    let resp = Client::new()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .context("Failed to connect to server")?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        // Try refreshing once, then retry.
        let new_token = refresh_on_401(config)?;
        let resp = Client::new()
            .get(&url)
            .bearer_auth(&new_token)
            .send()
            .context("Failed to connect to server")?;
        return parse_storage(resp);
    }
    parse_storage(resp)
}

fn parse_storage(resp: reqwest::blocking::Response) -> Result<StorageInfo> {
    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Failed to fetch storage");
        bail!("{msg}");
    }
    #[derive(Deserialize)]
    struct Resp {
        storage: StorageInfo,
    }
    let r: Resp = resp.json().context("Invalid response from server")?;
    Ok(r.storage)
}

// --- Files ---

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct RemoteFile {
    pub id: i64,
    pub name: String,
    pub size: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "OriginalViewUrl", default)]
    pub original_view_url: Option<String>,
    #[serde(rename = "OriginalDownloadUrl", default)]
    pub original_download_url: Option<String>,
    #[serde(rename = "ShortViewUrl", default)]
    pub short_view_url: Option<String>,
    #[serde(rename = "ShortDownloadUrl", default)]
    pub short_download_url: Option<String>,
}

pub fn list_files(config: &mut Config) -> Result<Vec<RemoteFile>> {
    let token = ensure_token(config)?;
    let url = format!("{}/api/files/my-files", config.base_url.trim_end_matches('/'));

    let resp = Client::new()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .context("Failed to connect to server")?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let new_token = refresh_on_401(config)?;
        let resp = Client::new()
            .get(&url)
            .bearer_auth(&new_token)
            .send()
            .context("Failed to connect to server")?;
        return parse_files(resp);
    }
    parse_files(resp)
}

fn parse_files(resp: reqwest::blocking::Response) -> Result<Vec<RemoteFile>> {
    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Failed to fetch files");
        bail!("{msg}");
    }
    resp.json().context("Invalid response from server")
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct RegenerateUrlsResult {
    pub fileId: i64,
    #[serde(rename = "ViewUrl")]
    pub view_url: Option<String>,
    #[serde(rename = "DownloadUrl")]
    pub download_url: Option<String>,
    #[serde(rename = "shortViewUrl")]
    pub short_view_url: Option<String>,
    #[serde(rename = "shortDownloadUrl")]
    pub short_download_url: Option<String>,
}

pub fn regenerate_urls(config: &mut Config, file_id: i64) -> Result<RegenerateUrlsResult> {
    let token = ensure_token(config)?;
    let url = format!(
        "{}/api/files/{}/regenerate-urls",
        config.base_url.trim_end_matches('/'),
        file_id
    );

    let resp = Client::new()
        .post(&url)
        .bearer_auth(&token)
        .send()
        .context("Failed to connect to server")?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let new_token = refresh_on_401(config)?;
        let resp = Client::new()
            .post(&url)
            .bearer_auth(&new_token)
            .send()
            .context("Failed to connect to server")?;
        return parse_regenerate(resp);
    }
    parse_regenerate(resp)
}

pub fn delete_file(config: &mut Config, file_id: i64) -> Result<()> {
    let token = ensure_token(config)?;
    let url = format!(
        "{}/api/files/{}",
        config.base_url.trim_end_matches('/'),
        file_id
    );

    let resp = Client::new()
        .delete(&url)
        .bearer_auth(&token)
        .send()
        .context("Failed to connect to server")?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let new_token = refresh_on_401(config)?;
        let resp = Client::new()
            .delete(&url)
            .bearer_auth(&new_token)
            .send()
            .context("Failed to connect to server")?;
        return parse_delete_file(resp);
    }
    parse_delete_file(resp)
}

fn parse_delete_file(resp: reqwest::blocking::Response) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Failed to delete file");
        bail!("{msg}");
    }
    Ok(())
}

fn parse_regenerate(resp: reqwest::blocking::Response) -> Result<RegenerateUrlsResult> {
    let status = resp.status();
    if !status.is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Failed to regenerate URLs");
        bail!("{msg}");
    }
    resp.json().context("Invalid response from server")
}

// --- Upload (async, with progress) ---

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct UploadResult {
    pub fileId: i64,
    #[serde(rename = "ViewUrl")]
    pub view_url: String,
    #[serde(rename = "DownloadUrl")]
    pub download_url: String,
    #[serde(rename = "shortViewUrl")]
    pub short_view_url: String,
    #[serde(rename = "shortDownloadUrl")]
    pub short_download_url: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: i64,
}

/// A progress reporter called periodically during upload.
/// Arguments: `(bytes_sent, total_bytes)`.
pub type ProgressFn = std::sync::Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Upload a file asynchronously, calling `on_progress` as bytes are sent.
/// Auto-refreshes the access token on 401 before failing.
pub async fn upload_file(
    config: &std::sync::Arc<std::sync::Mutex<Config>>,
    file_path: &std::path::Path,
    file_name: &str,
    content_type: &str,
    expires_in: i64,
    on_progress: ProgressFn,
) -> Result<UploadResult> {
    let bytes = tokio::fs::read(file_path).await?;
    let total = bytes.len() as u64;
    let bytes = std::sync::Arc::new(bytes);

    // Resolve a token (refreshing if needed) before starting.
    let token = {
        let mut cfg = config.lock().unwrap();
        ensure_token(&mut cfg)?
    };
    let base_url = config.lock().unwrap().base_url.clone();

    let result = do_upload(&token, &base_url, &bytes, total, file_name, content_type, expires_in, on_progress.clone()).await;

    match result {
        Ok(r) => Ok(r),
        Err(e) if is_unauthorized(&e) => {
            // 401 — refresh and retry once.
            let new_token = {
                let mut cfg = config.lock().unwrap();
                refresh_on_401(&mut cfg)?
            };
            do_upload(&new_token, &base_url, &bytes, total, file_name, content_type, expires_in, on_progress).await
        }
        Err(e) => Err(e),
    }
}

fn is_unauthorized(e: &anyhow::Error) -> bool {
    let msg = e.to_string();
    msg.contains("Unauthorized") || msg.contains("401")
}

async fn do_upload(
    token: &str,
    base_url: &str,
    bytes: &[u8],
    total: u64,
    file_name: &str,
    content_type: &str,
    expires_in: i64,
    on_progress: ProgressFn,
) -> Result<UploadResult> {
    let bytes = std::sync::Arc::new(bytes.to_vec());

    let stream = futures_util::stream::unfold(0u64, move |mut sent| {
        let on_progress = on_progress.clone();
        let bytes = bytes.clone();
        async move {
            if sent >= total {
                None
            } else {
                let chunk_size = std::cmp::min(8192, (total - sent) as usize);
                let end = (sent + chunk_size as u64) as usize;
                let chunk = bytes[sent as usize..end].to_vec();
                sent += chunk_size as u64;
                on_progress(sent, total);
                Some((Ok::<_, std::io::Error>(chunk), sent))
            }
        }
    });

    let body = reqwest::Body::wrap_stream(stream);

    let part = reqwest::multipart::Part::stream_with_length(body, total)
        .file_name(file_name.to_string())
        .mime_str(content_type)?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("expiresIn", expires_in.to_string());

    let url = format!("{}/api/upload/single", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await
        .context("Failed to connect to server")?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        bail!("Unauthorized");
    }
    if !status.is_success() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Upload failed");
        bail!("{msg}");
    }

    resp.json().await.context("Invalid response from server")
}
