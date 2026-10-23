//! The desk's side of the hub's HTTP API.

use player_two_core::api::{ApiError, JoinRequest, Joined};
use player_two_core::names::PersonName;
use reqwest::{RequestBuilder, Response};
use serde::de::DeserializeOwned;

/// Sends a request and reads its JSON answer, turning a refusal into the hub's own words.
pub async fn call<T: DeserializeOwned>(base: &str, request: RequestBuilder) -> anyhow::Result<T> {
    let response = request
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("cannot reach the hub at {base}: {error}"))?;
    read(response).await
}

/// Reads a response: its JSON when it succeeded, the hub's reason when it did not.
pub async fn read<T: DeserializeOwned>(response: Response) -> anyhow::Result<T> {
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        let reason = serde_json::from_str::<ApiError>(&text).map_or_else(
            |_| format!("the hub answered {status}"),
            |error| error.error,
        );
        anyhow::bail!(reason);
    }
    Ok(serde_json::from_str(if text.is_empty() {
        "null"
    } else {
        &text
    })?)
}

/// Joins the hub at `base` as `name`, giving back the personal token.
pub async fn join(
    http: &reqwest::Client,
    base: &str,
    join_token: &str,
    name: &PersonName,
) -> anyhow::Result<String> {
    let request = http.post(format!("{base}/join")).json(&JoinRequest {
        join_token: join_token.to_owned(),
        name: name.clone(),
    });
    let joined: Joined = call(base, request).await?;
    Ok(joined.token)
}
