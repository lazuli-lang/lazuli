//! `.lzi` source templates for the auth_protected_endpoint smoke test.
//! `write_fixture` writes `app.lzi` + `account.lzi` into a tempdir
//! before the Lazuli generator is invoked.

#![cfg(feature = "smoke_e2e")]

use std::fs;
use std::path::Path;

pub fn write_fixture(dir: &Path) {
    fs::write(
        dir.join("app.lzi"),
        r#"app AuthProtectedSmoke
  uses
    account

  targets
    backend go

  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"
"#,
    )
    .unwrap_or_else(|err| panic!("writing app.lzi: {err}"));

    fs::write(
        dir.join("account.lzi"),
        r#"feature account
  purpose "Auth protected endpoint smoke fixture."

  domain
    enum UserRole
      user
      admin

    resource User
      email: @semantic.Email required unique
      name: Text required
      role: UserRole = user
      password_hash: @cap.Hashed(algorithm:argon2id) optional

      timestamps

    resource Session
      user: User required on_delete cascade
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

      timestamps

    record AuthSession
      user_id: User.ID required
      session_id: Session.ID required
      access_token: Text required
      expires_at: DateTime required

    record Profile
      user_id: User.ID required
      email: @semantic.Email required

    query.lookup by_email by email: @semantic.Email

  policies
    login: @scope.public
    read: @scope.authenticated

  auth
    identity User.email

    password
      algorithm argon2id
      hash @fn.hash_password
      verify @fn.verify_password
      rate_limit "5 per 10 minutes"

    sessions
      resource Session
      ttl "30 days"
      refresh false

  command login
    input
      email: @semantic.Email required
      password: Text required
    policy @policy.login
    returns AuthSession

  api profile
    method GET
    path "/api/auth-smoke/me"
    output Profile
    policy @policy.read

  extensions
    fn hash_password: Function[Text, @cap.Hashed(algorithm:argon2id)] at "./auth/hash_password.go"
    fn verify_password: Function[PasswordVerifyInput, PasswordVerifyResult] at "./auth/verify_password.go"
"#,
    )
    .unwrap_or_else(|err| panic!("writing account.lzi: {err}"));
}
