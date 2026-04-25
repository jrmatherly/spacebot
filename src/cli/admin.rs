//! Admin CLI wrappers that POST to `/api/admin/*` endpoints via the
//! `AuthedClient` (STORE-D: local JSON token cache + Bearer header).

use crate::cli::http::AuthedClient;
use crate::cli::store::CliTokenStore;

pub async fn claim_resource(
    resource_type: &str,
    resource_id: &str,
    owner: &str,
    visibility: &str,
    team: Option<&str>,
) -> anyhow::Result<()> {
    // Default to the local daemon. Override with SPACEBOT_DAEMON_URL to
    // point at a remote daemon (e.g. when the operator is claiming on a
    // deployed instance).
    let base = std::env::var("SPACEBOT_DAEMON_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:19898".into());
    let tenant_id = std::env::var("SPACEBOT_TENANT_ID")
        .map_err(|_| anyhow::anyhow!("set SPACEBOT_TENANT_ID"))?;
    let client_id = std::env::var("SPACEBOT_CLI_CLIENT_ID")
        .map_err(|_| anyhow::anyhow!("set SPACEBOT_CLI_CLIENT_ID"))?;

    let store = CliTokenStore::load()?;
    let client = AuthedClient::new(store, base.clone(), tenant_id, client_id);

    let body = serde_json::json!({
        "resource_type": resource_type,
        "resource_id": resource_id,
        "owner_principal_key": owner,
        "visibility": visibility,
        "shared_with_team_id": team,
    });
    let req = client
        .http()
        .post(format!("{base}/api/admin/claim-resource"))
        .json(&body);
    let res = client.send(req).await?;
    let status = res.status();
    if !status.is_success() {
        let msg = res.text().await.unwrap_or_default();
        anyhow::bail!("claim failed: {status} {msg}");
    }
    Ok(())
}
