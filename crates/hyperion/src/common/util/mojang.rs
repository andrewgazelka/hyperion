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

/// The API provider to use for Minecraft profile lookups
#[derive(Clone, Copy)]
pub struct ApiProvider {
    username_base_url: &'static str,
    uuid_base_url: &'static str,
    max_requests: usize,
    interval: Duration,
}

impl ApiProvider {
    /// The matdoes.dev API mirror provider with higher rate limits
    pub const MAT_DOES_DEV: Self = Self {
        username_base_url: "https://mowojang.matdoes.dev/users/profiles/minecraft",
        uuid_base_url: "https://mowojang.matdoes.dev/session/minecraft/profile",
        max_requests: 10_000,
        interval: Duration::from_secs(1),
    };
    /// The official Mojang API provider
    pub const MOJANG: Self = Self {
        username_base_url: "https://api.mojang.com/users/profiles/minecraft",
        uuid_base_url: "https://sessionserver.mojang.com/session/minecraft/profile",
        max_requests: 600,
        interval: Duration::from_mins(10),
    };

    fn username_url(&self, username: &str) -> String {
        format!("{}/{username}", self.username_base_url)
    }

    fn uuid_url(&self, uuid: &Uuid) -> String {
        format!("{}/{uuid}?unsigned=false", self.uuid_base_url)
    }

    const fn max_requests(&self) -> usize {
        self.max_requests
    }

    const fn interval(&self) -> Duration {
        self.interval
    }
}

/// A client to interface with the Minecraft profile API.
///
/// Can use either the official Mojang API or [matdoes/mowojang](https://matdoes.dev/minecraft-uuids) as a data source.
/// This does not include caching, this should be done separately probably using [`crate::storage::Db`].
#[derive(Component, Clone)]
pub struct MojangClient {
    req: reqwest::Client,
    rate_limit: Arc<Semaphore>,
    provider: ApiProvider,
}

impl MojangClient {
    #[must_use]
    pub fn new(tasks: &AsyncRuntime, provider: ApiProvider) -> Self {
        let rate_limit = Arc::new(Semaphore::new(provider.max_requests()));
        let interval_duration = provider.interval();

        tokio::task::Builder::new()
            .name("reset_rate_limit")
            .spawn_on(
                {
                    let rate_limit = Arc::downgrade(&rate_limit);
                    let max_requests = provider.max_requests();
                    async move {
                        let mut interval = interval(interval_duration);
                        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

                        loop {
                            interval.tick().await;

                            let Some(rate_limit) = rate_limit.upgrade() else {
                                return;
                            };

                            let available = rate_limit.available_permits();
                            rate_limit.add_permits(max_requests - available);
                        }
                    }
                },
                tasks.handle(),
            )
            .unwrap();

        Self {
            req: reqwest::Client::new(),
            rate_limit,
            provider,
        }
    }

    /// Gets a player's UUID from their username.
    pub async fn get_uuid(&self, username: &str) -> anyhow::Result<Uuid> {
        let url = self.provider.username_url(username);
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
        let url = self.provider.uuid_url(&uuid);
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
        let url = self.provider.uuid_url(uuid);
        self.response_raw(&url).await
    }

    /// Gets player data from their username.
    pub async fn data_from_username(&self, username: &str) -> anyhow::Result<Value> {
        let url = self.provider.username_url(username);
        self.response_raw(&url).await
    }

    async fn response_raw(&self, url: &str) -> anyhow::Result<Value> {
        self.rate_limit
            .acquire()
            .await
            .expect("semaphore is never closed")
            .forget();

        if self.rate_limit.available_permits() == 0 {
            warn!(
                "rate limiting will be applied: {} requests have been sent in the past {:?} \
                 interval",
                self.provider.max_requests(),
                self.provider.interval()
            );
        }

        let response = self.req.get(url).send().await?;

        if response.status().is_success() {
            let body = response.text().await?;
            let json_object = serde_json::from_str::<Value>(&body)
                .with_context(|| format!("failed to parse json from response: {body:?}"))?;

            if let Some(error) = json_object.get("error") {
                bail!("API Error: {}", error.as_str().unwrap_or("Unknown error"));
            };
            Ok(json_object)
        } else {
            bail!("Failed to retrieve data from API");
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "these are tests")]
mod tests {
    use std::str::FromStr;

    use crate::{
        runtime::AsyncRuntime,
        util::mojang::{ApiProvider, MojangClient},
    };

    #[test]
    fn test_get_uuid() {
        let tasks = AsyncRuntime::default();
        let mojang = MojangClient::new(&tasks, ApiProvider::MAT_DOES_DEV);

        let uuid = tasks.block_on(mojang.get_uuid("Emerald_Explorer")).unwrap();
        let expected = uuid::Uuid::from_str("86271406-1188-44a5-8496-7af10c906204").unwrap();
        assert_eq!(uuid, expected);
    }

    #[test]
    fn test_get_username() {
        let tasks = AsyncRuntime::default();
        let mojang = MojangClient::new(&tasks, ApiProvider::MAT_DOES_DEV);

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
        let mojang = MojangClient::new(&tasks, ApiProvider::MAT_DOES_DEV);

        let res = tasks
            .block_on(mojang.data_from_uuid(
                &uuid::Uuid::from_str("86271406-1188-44a5-8496-7af10c906204").unwrap(),
            ))
            .unwrap();

        let pretty = serde_json::to_string_pretty(&res).unwrap();
        println!("{pretty}");
    }
}
