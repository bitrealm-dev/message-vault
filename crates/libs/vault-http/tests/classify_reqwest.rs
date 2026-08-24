//! reqwest-status classification needs a real response: build an error via
//! `error_for_status()` against a local mock server.

use httpmock::prelude::*;
use vault_http::{RetryKind, classify_retry};

#[test]
fn reqwest_4xx_status_is_permanent_5xx_is_transient() {
    let server = MockServer::start();
    let mock_404 = server.mock(|when, then| {
        when.method(GET).path("/missing");
        then.status(404);
    });
    let mock_500 = server.mock(|when, then| {
        when.method(GET).path("/broken");
        then.status(500);
    });

    let client = reqwest::blocking::Client::new();
    let e404 = client
        .get(server.url("/missing"))
        .send()
        .and_then(|r| r.error_for_status())
        .unwrap_err();
    assert_eq!(
        classify_retry(&anyhow::Error::new(e404)),
        RetryKind::Permanent
    );

    let e500 = client
        .get(server.url("/broken"))
        .send()
        .and_then(|r| r.error_for_status())
        .unwrap_err();
    assert_eq!(
        classify_retry(&anyhow::Error::new(e500)),
        RetryKind::Transient
    );

    mock_404.assert();
    mock_500.assert();
}
