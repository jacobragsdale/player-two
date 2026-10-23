mod common;

use common::Hub;
use serde_json::json;

#[tokio::test]
async fn health_and_version_need_no_token() {
    let hub = Hub::start().await;
    let health = hub.get("", "/healthz").await;
    assert_eq!((health.status, health.body), (200, json!("ok")));
    assert_eq!(
        hub.get("", "/version").await.body,
        json!({"version": env!("CARGO_PKG_VERSION")})
    );
}

#[tokio::test]
async fn the_installer_and_its_manifest_are_served_when_the_image_carries_them() {
    let hub = Hub::start().await;
    assert_eq!(hub.get("", "/download").await.status, 404);
    assert_eq!(hub.get("", "/update").await.status, 404);

    let release = hub.state.config.release.clone();
    std::fs::create_dir_all(&release).expect("a folder");
    std::fs::write(release.join("player-two-setup.exe"), b"MZ installer").expect("written");
    std::fs::write(release.join("player-two-setup.exe.sig"), b"c2lnbmF0dXJl\n").expect("written");

    let download = hub
        .client
        .get(format!("{}/download", hub.base))
        .send()
        .await
        .expect("answers");
    assert_eq!(download.status().as_u16(), 200);
    assert!(
        download.headers()["content-disposition"]
            .to_str()
            .is_ok_and(|value| value.contains("player-two-setup.exe"))
    );
    assert_eq!(
        download.bytes().await.expect("a body").as_ref(),
        b"MZ installer"
    );

    let manifest = hub.get("", "/update").await.body;
    assert_eq!(
        manifest,
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "platforms": {"windows-x86_64": {"signature": "c2lnbmF0dXJl", "url": "https://hub.example/download"}}
        })
    );
}
