use std::cell::RefCell;

#[derive(Debug, Clone)]
#[must_use]
pub struct Version {
    current: &'static str,
    latest: RefCell<String>,
    #[cfg(not(target_arch = "wasm32"))]
    client: Option<reqwest::blocking::Client>,
    #[cfg(not(target_arch = "wasm32"))]
    rate_limit: std::time::Duration,
    #[cfg(not(target_arch = "wasm32"))]
    last_request_time: std::cell::Cell<std::time::Instant>,
}

impl Default for Version {
    fn default() -> Self {
        Self::new()
    }
}

impl Version {
    pub fn new() -> Self {
        Self {
            current: env!("CARGO_PKG_VERSION"),
            latest: RefCell::new(env!("CARGO_PKG_VERSION").to_string()),
            #[cfg(not(target_arch = "wasm32"))]
            client: Self::create_client(),
            #[cfg(not(target_arch = "wasm32"))]
            rate_limit: std::time::Duration::from_secs(1),
            #[cfg(not(target_arch = "wasm32"))]
            last_request_time: std::cell::Cell::new(std::time::Instant::now()),
        }
    }

    pub const fn current(&self) -> &str {
        self.current
    }

    pub fn latest(&self) -> String {
        self.latest.borrow().clone()
    }

    pub fn set_latest(&mut self, version: String) {
        self.latest.replace(version);
    }

    pub const fn requires_updates(&self) -> bool {
        cfg!(not(target_arch = "wasm32"))
    }

    #[cfg(target_arch = "wasm32")]
    pub const fn update_available(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn update_available(&self) -> anyhow::Result<Option<String>> {
        use std::time::Instant;

        if self.last_request_time.get().elapsed() < self.rate_limit {
            std::thread::sleep((self.last_request_time.get() + self.rate_limit) - Instant::now());
        }

        self.last_request_time.set(Instant::now());
        let Some(client) = &self.client else {
            anyhow::bail!("failed to create http client");
        };
        let content = client
            .get("https://crates.io/api/v1/crates/tetanes")
            .send()
            .and_then(|res| res.text())?;
        if let Ok(res) = serde_json::from_str::<ApiErrors>(&content) {
            anyhow::bail!(
                "encountered crates.io API errors: {}",
                res.errors
                    .into_iter()
                    .filter_map(|error| error.detail)
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }

        match serde_json::from_str::<CrateResponse>(&content) {
            Ok(CrateResponse {
                cr: Crate { newest_version, .. },
            }) => {
                if Self::version_is_newer(&newest_version, self.current) {
                    self.latest.replace(newest_version.clone());
                    Ok(Some(newest_version))
                } else {
                    Ok(None)
                }
            }
            Err(err) => anyhow::bail!("failed to deserialize crates.io response: {err:?}"),
        }
    }

    pub fn install_update_and_restart(&mut self) -> anyhow::Result<()> {
        // TODO: Implement install/restart for each platform
        anyhow::bail!("not yet implemented");
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn version_is_newer(new: &str, old: &str) -> bool {
        match (semver::Version::parse(old), semver::Version::parse(new)) {
            (Ok(old), Ok(new)) => new > old,
            _ => false,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn create_client() -> Option<reqwest::blocking::Client> {
        use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str("tetanes (me@lukeworks.tech)").ok()?,
        );
        reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .ok()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, serde::Deserialize)]
#[must_use]
struct ApiError {
    detail: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, serde::Deserialize)]
#[must_use]
struct ApiErrors {
    errors: Vec<ApiError>,
}

// Partial deserialization of the full response
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, serde::Deserialize)]
#[must_use]
struct Crate {
    newest_version: String,
}

// Partial deserialization of the full response
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, serde::Deserialize)]
#[must_use]
struct CrateResponse {
    #[serde(rename = "crate")]
    cr: Crate,
}
