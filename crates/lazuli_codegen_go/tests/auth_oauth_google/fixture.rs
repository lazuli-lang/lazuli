//! `.lzi` source templates for the OAuth Google smoke fixture. The
//! parent `main.rs` writes these to a tempdir before invoking the
//! Lazuli generator.

#![cfg(feature = "smoke_e2e")]

use std::fs;
use std::path::Path;

pub fn write_oauth_google_fixture(dir: &Path) {
    fs::create_dir_all(dir).unwrap_or_else(|err| panic!("creating {}: {err}", dir.display()));
    fs::write(
        dir.join("app.lzi"),
        r#"app OAuthGoogleSmoke
  uses
    account

  targets
    backend go

  environments
    local

  urls
    web local "http://127.0.0.1:8080"
    api local "http://127.0.0.1:8080"

  runtime
    unit api
      healthcheck "/healthz"
"#,
    )
    .unwrap_or_else(|err| panic!("writing app.lzi: {err}"));

    fs::write(
        dir.join("account.lzi"),
        r#"feature account
  purpose "OAuth Google redirect smoke fixture."

  domain
    resource User
      email: @semantic.Email required unique
      name: Text optional

    resource Session
      user: User required
      expires_at: DateTime required

  policies
    login: @scope.public

  auth
    identity User.email

    oauth google
      adapter @adapter.google_oauth

    sessions
      resource Session
      ttl "30 days"
      refresh true

  extensions
    adapter google_oauth: IntegrationAdapter[GoogleOAuth] at "./integrations/google_oauth.go"
"#,
    )
    .unwrap_or_else(|err| panic!("writing account.lzi: {err}"));
}
