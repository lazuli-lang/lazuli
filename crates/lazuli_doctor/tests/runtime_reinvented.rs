//! TDD oracle for `VOCAB-RUNTIME-REINVENTED-001`.
//!
//! These tests are the audit oracle in miniature: each fixture is grounded
//! on a real handler the reinvention audit confirmed (or a real negative the
//! audit refuted). They lock the two detector families (import-signal,
//! shape-signal), the precision guards (vendor `crypto/hmac`, plain
//! `UPDATE`), the `# doctor:allow` waiver, and — crucially — the table
//! extensibility contract: a new `REINVENTION_TABLE` row catches a new
//! family without touching `check`.

use std::path::Path;

use lazuli_doctor::vocab::runtime_reinvented_001::{
    self as rule, Finding, REINVENTION_TABLE, ReinventionRule, Trigger, scan_handler,
};

fn write_handler(dir: &Path, stem: &str, src: &str) {
    let handlers = dir.join("handlers");
    std::fs::create_dir_all(&handlers).unwrap();
    std::fs::write(handlers.join(format!("{stem}.go")), src).unwrap();
}

// Grounded on pauta account/handlers/hash_password.go + hostpoint
// account/handlers/hash_password.go: argon2id encoded by hand.
const HASH_PASSWORD_GO: &str = r#"
package accounthandlers

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"fmt"

	"golang.org/x/crypto/argon2"

	"lazuli.dev/runtime/lazuli"
)

func HashPassword(ctx *lazuli.Ctx, input any) (any, error) {
	salt := make([]byte, 16)
	rand.Read(salt)
	key := argon2.IDKey([]byte("pw"), salt, 3, 64*1024, 2, 32)
	return fmt.Sprintf("$argon2id$%s", base64.RawStdEncoding.EncodeToString(key)), nil
}
"#;

// Grounded on hostpoint payments/handlers/mp_client.go: a MercadoPago
// webhook signature verified with crypto/hmac — a LEGITIMATE vendor
// signature with no runtime equivalent. Imports crypto/sha256 + encoding/hex
// (which the token-hash row keys on) but the crypto/hmac presence proves it
// is vendor-signature verification, not a reinvented session token.
const VENDOR_HMAC_GO: &str = r#"
package paymentshandlers

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
)

func VerifyMpWebhookSignature(payload []byte, signatureHeader, secret string) error {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(payload)
	expected := hex.EncodeToString(mac.Sum(nil))
	if !hmac.Equal([]byte(signatureHeader), []byte(expected)) {
		return errors.New("MP_WEBHOOK_SIGNATURE_INVALID")
	}
	return nil
}
"#;

// Grounded on hostpoint operations/handlers/accept_proposal.go: the
// UPDATE ... status IN (...) + RowsAffected()==0 lifecycle-transition shape.
const ACCEPT_PROPOSAL_GO: &str = r#"
package operationshandlers

func AcceptProposal(ctx *lazuli.Ctx, input AcceptInput) (struct{}, error) {
	res, err := db.Exec(ctx,
		`UPDATE "service_transaction"
		 SET status = 'accepted', accepted_at = $1
		 WHERE id = $2
		   AND status IN ('requested', 'proposal_sent')`,
		now, input.TransactionID,
	)
	if err != nil {
		return struct{}{}, err
	}
	if res.RowsAffected() == 0 {
		return struct{}{}, ErrTransactionNotFoundOrInvalidState
	}
	return struct{}{}, nil
}
"#;

// A plain UPDATE with no RowsAffected==0 sentinel: an ordinary imperative
// write, NOT a hand-rolled lifecycle transition. Must stay silent.
const PLAIN_UPDATE_GO: &str = r#"
package opshandlers

func TouchSeen(ctx *lazuli.Ctx, id string) error {
	_, err := db.Exec(ctx, `UPDATE rows SET seen = true WHERE id = $1`, id)
	return err
}
"#;

// Grounded on pauta agency/handlers/validate_hex_color.go: the
// ^#?[0-9A-Fa-f]{6}$ regex literal that reinvents @semantic.HexColor.
const VALIDATE_HEX_COLOR_GO: &str = r##"
package agencyhandlers

import (
	"errors"
	"regexp"
)

var hexColorRe = regexp.MustCompile(`^#[0-9A-Fa-f]{6}$`)

func ValidateHexColor(input any) error {
	if !hexColorRe.MatchString("#ffffff") {
		return errors.New("INVALID_HEX_COLOR")
	}
	return nil
}
"##;

#[test]
fn argon2_import_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "hash_password", HASH_PASSWORD_GO);
    let findings = rule::check_dir("account", dir.path());
    assert_eq!(findings.len(), 1, "argon2 handler must fire exactly once");
    assert_eq!(Finding::CODE, "VOCAB-RUNTIME-REINVENTED-001");
    assert_eq!(findings[0].family, "auth.password-hash");
    let msg = findings[0].message();
    assert!(
        msg.contains("auth.HashPassword"),
        "message must name the runtime symbol: {msg}"
    );
    assert!(
        msg.contains("delegate-to-runtime.md"),
        "message must link the teach doc: {msg}"
    );
    assert!(!findings[0].waived);
}

#[test]
fn vendor_hmac_does_not_fire() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "verify_mp_webhook", VENDOR_HMAC_GO);
    let findings = rule::check_dir("payments", dir.path());
    assert!(
        findings.is_empty(),
        "vendor crypto/hmac webhook signature has NO runtime equivalent — must stay silent, got {findings:?}"
    );
}

#[test]
fn lifecycle_shape_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "accept_proposal", ACCEPT_PROPOSAL_GO);
    let findings = rule::check_dir("operations", dir.path());
    assert_eq!(findings.len(), 1, "lifecycle-shape handler must fire");
    assert_eq!(findings[0].family, "lifecycle.transition");
    assert!(
        findings[0].message().contains("transition"),
        "message must name the `transition` primitive"
    );
}

#[test]
fn plain_update_does_not_fire() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "touch_seen", PLAIN_UPDATE_GO);
    let findings = rule::check_dir("operations", dir.path());
    assert!(
        findings.is_empty(),
        "a plain UPDATE without the RowsAffected==0 sentinel must stay silent, got {findings:?}"
    );
}

#[test]
fn hexcolor_regex_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "validate_hex_color", VALIDATE_HEX_COLOR_GO);
    let findings = rule::check_dir("agency", dir.path());
    assert_eq!(findings.len(), 1, "hex-color regex handler must fire");
    assert_eq!(findings[0].family, "scalar.hexcolor");
    assert!(
        findings[0].message().contains("HexColor"),
        "message must name @semantic.HexColor"
    );
}

#[test]
fn doctor_allow_suppresses() {
    // The waiver is honored: the finding records the silence as an explicit,
    // reasoned opt-out (the `waived` flag is set + the body says so).
    let waived = HASH_PASSWORD_GO.replace(
        "func HashPassword(ctx *lazuli.Ctx, input any) (any, error) {",
        "func HashPassword(ctx *lazuli.Ctx, input any) (any, error) {\n# doctor:allow VOCAB-RUNTIME-REINVENTED-001 — reason \"gated on auth.HashPassword wiring\"",
    );
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "hash_password", &waived);
    let findings = rule::check_dir("account", dir.path());
    assert_eq!(findings.len(), 1);
    assert!(findings[0].waived, "the allow comment must set the waived flag");
    assert!(
        findings[0].message().contains("doctor:allow"),
        "waived message must reference the explicit opt-out"
    );
}

#[test]
fn table_is_extensible() {
    // The parameterization is the deliverable: adding a row to a LOCAL copy
    // of the table catches a brand-new family WITHOUT touching `check`. We
    // drive the same engine `check` uses (`scan_handler`) with an extended
    // table to prove a new row is all it takes.
    const NEW_FAMILY_GO: &str = r#"
package fees

func FormatPct(v float64) string {
	if v >= 0 && v <= 100 {
		return "ok"
	}
	return "bad"
}
"#;
    // Baseline: the seed table does not yet claim this percentage shape via
    // this exact bespoke trigger, so prove the row is what flips it on.
    let extra = ReinventionRule {
        trigger: Trigger::BodyShape(&[">= 0", "<= 100"]),
        equivalent: "@semantic.Percentage",
        family: "scalar.percentage-extended",
    };
    let mut extended: Vec<ReinventionRule> = REINVENTION_TABLE.to_vec();
    extended.push(extra);

    let hits = scan_handler(NEW_FAMILY_GO, &extended);
    assert!(
        hits.iter().any(|h| h.family == "scalar.percentage-extended"),
        "adding one row must catch the new family with no change to check()"
    );
}
