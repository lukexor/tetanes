use std::cell::RefCell;

#[cfg(not(target_arch = "wasm32"))]
mod fetcher {
    use reqwest::blocking::Client;
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone)]
    #[must_use]
    pub struct Fetcher {
        client: Option<Client>,
        rate_limit: Duration,
        last_request_time: Cell<Instant>,
    }

    impl Default for Fetcher {
        fn default() -> Self {
            Self {
                client: Self::create_client(),
                rate_limit: Duration::from_secs(1),
                last_request_time: Cell::new(Instant::now()),
            }
        }
    }

    impl Fetcher {
        fn create_client() -> Option<Client> {
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

        pub fn update_available(&self, version: &'static str) -> anyhow::Result<Option<String>> {
            #[derive(Debug, serde::Deserialize)]
            #[must_use]
            struct ApiError {
                detail: Option<String>,
            }

            #[derive(Debug, serde::Deserialize)]
            #[must_use]
            struct ApiErrors {
                errors: Vec<ApiError>,
            }

            // Partial deserialization of the full response
            #[derive(Debug, serde::Deserialize)]
            #[must_use]
            struct Crate {
                newest_version: String,
            }

            // Partial deserialization of the full response
            #[derive(Debug, serde::Deserialize)]
            #[must_use]
            struct CrateResponse {
                #[serde(rename = "crate")]
                cr: Crate,
            }

            if self.last_request_time.get().elapsed() < self.rate_limit {
                std::thread::sleep(
                    (self.last_request_time.get() + self.rate_limit) - Instant::now(),
                );
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
                    if Self::is_newer(&newest_version, version) {
                        Ok(Some(newest_version))
                    } else {
                        Ok(None)
                    }
                }
                Err(err) => anyhow::bail!("failed to deserialize crates.io response: {err:?}"),
            }
        }

        fn is_newer(new: &str, old: &str) -> bool {
            match (semver::Version::parse(old), semver::Version::parse(new)) {
                (Ok(old), Ok(new)) => new > old,
                _ => false,
            }
        }
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct Version {
    current: &'static str,
    latest: RefCell<String>,
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
    pub fn check_for_updates(
        &mut self,
        _tx: &crate::nes::event::NesEventProxy,
        _notify_latest: bool,
    ) {
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn check_for_updates(
        &mut self,
        tx: &crate::nes::event::NesEventProxy,
        notify_latest: bool,
    ) {
        use crate::nes::{event::UiEvent, renderer::gui::MessageType};

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let spawn_update = std::thread::Builder::new()
            .name("check_updates".into())
            .spawn({
                let current_version = self.current;
                let fetcher = fetcher::Fetcher::default();
                let tx = tx.clone();
                move || {
                    let newest_version = fetcher.update_available(current_version);
                    match newest_version {
                        Ok(Some(version)) => tx.event(UiEvent::UpdateAvailable(version)),
                        Ok(None) => {
                            if notify_latest {
                                tx.event(UiEvent::Message((
                                    MessageType::Info,
                                    format!("TetaNES v{current_version} is up to date!"),
                                )));
                            }
                        }
                        Err(err) => {
                            tx.event(UiEvent::Message((MessageType::Error, err.to_string())));
                        }
                    }
                }
            });
        if let Err(err) = spawn_update {
            tx.event(UiEvent::Message((
                MessageType::Error,
                format!("Failed to check for updates: {err}"),
            )));
        }
    }

    pub fn install_update_and_restart(&mut self) -> anyhow::Result<()> {
        // TODO: Implement install/restart for each platform
        anyhow::bail!("not yet implemented");
    }
}
