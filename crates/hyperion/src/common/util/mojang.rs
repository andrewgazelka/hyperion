//! See [`MojangClient`].

use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context};
use flecs_ecs::macros::Component;
use serde_json::Value;
use tokio::{
    sync::Semaphore,
    time::{interval, MissedTickBehavior},
};
use tracing::warn;
use uuid::Uuid;

use crate::runtime::AsyncRuntime;

/// Maximum number of requests that can be made during [`MAX_REQUESTS_INTERVAL`]. The value for this rate limit is from [wiki.vg](https://wiki.vg/Mojang_API).
pub const MAX_REQUESTS_PER_INTERVAL: usize = 600;

/// Maximum number of requests that can be made during this interval. The value for this rate limit is from [wiki.vg](https://wiki.vg/Mojang_API).
pub const MAX_REQUESTS_INTERVAL: Duration = Duration::from_secs(600);

fn username_url(username: &str) -> String {
    format!("https://api.mojang.com/users/profiles/minecraft/{username}")
    // format!("https://mowojang.matdoes.dev/users/profiles/minecraft/{username}")
}

fn uuid_url(uuid: &Uuid) -> String {
    format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false")
    // format!("https://mowojang.matdoes.dev/session/minecraft/profile/{uuid}?unsigned=false")
}

/// A client to interface with the Mojang API.
///
/// This uses [matdoes/mowojang](https://matdoes.dev/minecraft-uuids) as a primary source of data.
/// This does not include caching, this should be done separately probably using [`crate::storage::Db`].
///
/// todo: add Mojang API backup
#[derive(Component, Clone)]
pub struct MojangClient {
    req: reqwest::Client,
    rate_limit: Arc<Semaphore>,
}

// todo: add cache for MojangUtils
impl MojangClient {
    #[must_use]
    pub fn new(tasks: &AsyncRuntime) -> Self {
        let rate_limit = Arc::new(Semaphore::new(MAX_REQUESTS_PER_INTERVAL));

        tokio::task::Builder::new()
            .name("reset_rate_limit")
            .spawn_on(
                {
                    let rate_limit = Arc::downgrade(&rate_limit);
                    async move {
                        let mut interval = interval(MAX_REQUESTS_INTERVAL);
                        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

                        loop {
                            interval.tick().await;

                            let Some(rate_limit) = rate_limit.upgrade() else {
                                return;
                            };

                            let available = rate_limit.available_permits();

                            // Reset the number of available permits to MAX_REQUEST_PER_INTERVAL.
                            rate_limit.add_permits(MAX_REQUESTS_PER_INTERVAL - available);
                        }
                    }
                },
                tasks.handle(),
            )
            .unwrap();

        Self {
            req: reqwest::Client::new(),
            rate_limit,
        }
    }

    /// Gets a player's UUID from their username.
    pub async fn get_uuid(&self, username: &str) -> anyhow::Result<Uuid> {
        let url = username_url(username);
        let json_object = self.response_raw(&url).await?;

        let id = json_object
            .get("id")
            .context("no id in json")?
            .as_str()
            .context("id is not a string")?;

        Uuid::parse_str(id).map_err(Into::into)
    }

    /// Gets a player's username from their UUID.
    pub async fn get_username(&self, uuid: Uuid) -> anyhow::Result<String> {
        let url = uuid_url(&uuid);
        let json_object = self.response_raw(&url).await?;

        json_object
            .get("name")
            .context("no name in json")?
            .as_str()
            .map(String::from)
            .context("Username not found")
    }

    /// Gets player data from their UUID.
    pub async fn data_from_uuid(&self, uuid: &Uuid) -> anyhow::Result<Value> {
        let url = uuid_url(uuid);
        self.response_raw(&url).await
    }

    /// Gets player data from their username.
    pub async fn data_from_username(&self, username: &str) -> anyhow::Result<Value> {
        let url = username_url(username);
        self.response_raw(&url).await
    }

    async fn response_raw(&self, url: &str) -> anyhow::Result<Value> {
        // Counts this request towards the current MAX_REQUESTS_PER_INTERVAL. If the max
        // requests per interval have already been reached, this will wait until the next
        // interval to do the request.
        self.rate_limit
            .acquire()
            .await
            .expect("semaphore is never closed")
            .forget();

        if self.rate_limit.available_permits() == 0 {
            warn!(
                "rate limiting will be applied: {MAX_REQUESTS_PER_INTERVAL} requests have been \
                 sent in the past {MAX_REQUESTS_INTERVAL:?} interval; new requests will be \
                 delayed until the next {MAX_REQUESTS_INTERVAL:?} interval"
            );
        }

        let response = self.req.get(url).send().await?;

        if response.status().is_success() {
            let body = response.text().await?;
            let json_object = serde_json::from_str::<Value>(&body)
                .with_context(|| format!("failed to parse json from mojang response: {body:?}"))?;

            if let Some(error) = json_object.get("error") {
                bail!(
                    "Mojang API Error: {}",
                    error.as_str().unwrap_or("Unknown error")
                );
            };
            Ok(json_object)
        } else {
            bail!("Failed to retrieve data from Mojang API");
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "these are tests")]
mod tests {
    use std::str::FromStr;

    use crate::{runtime::AsyncRuntime, util::mojang::MojangClient};

    #[test]
    fn test_get_uuid() {
        let tasks = AsyncRuntime::default();
        let mojang = MojangClient::new(&tasks);

        let uuid = tasks.block_on(mojang.get_uuid("Emerald_Explorer")).unwrap();
        let expected = uuid::Uuid::from_str("86271406-1188-44a5-8496-7af10c906204").unwrap();
        assert_eq!(uuid, expected);
    }

    #[test]
    fn test_get_username() {
        let tasks = AsyncRuntime::default();
        let mojang = MojangClient::new(&tasks);

        let username = tasks
            .block_on(mojang.get_username(
                uuid::Uuid::from_str("86271406-1188-44a5-8496-7af10c906204").unwrap(),
            ))
            .unwrap();
        assert_eq!(username, "Emerald_Explorer");
    }

    #[test]
    fn test_retrieve_username() {
        let tasks = AsyncRuntime::default();
        let mojang = MojangClient::new(&tasks);

        let res = tasks
            .block_on(mojang.data_from_uuid(
                &uuid::Uuid::from_str("86271406-1188-44a5-8496-7af10c906204").unwrap(),
            ))
            .unwrap();

        let pretty = serde_json::to_string_pretty(&res).unwrap();
        println!("{pretty}");
    }
}
