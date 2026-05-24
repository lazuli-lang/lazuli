//! Doctor pass for refresh-token rotation invariants.
//!
//! LAZ-80 / ANALYZE-1 wires the nine `AUTH-REFRESH-*` diagnostics over
//! the typed `auth.sessions.rotation` IR. Cross-layer adoption checks use
//! app `capabilities` as explicit project markers until those surfaces are
//! promoted into `AppManifest`.

pub(super) mod rules;

use std::path::PathBuf;

use super::{
    AuthFacts, DoctorAppManifest, DoctorDiagnostic, DoctorFile, DoctorSeverity, ResourceFact,
};

pub(super) const DEFAULT_REFRESH_TTL: &str = "14d";
pub(super) const DEFAULT_ROTATION_GRACE: &str = "1m";
pub(super) const DEFAULT_THEFT_ACTION: &str = "revoke_session_family";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthRefreshDiagnostic {
    MissingSecretProvider,
    GraceExceedsRefreshTtl,
    MissingRotationColumns,
    RevokeUserWithoutUserFk,
    RefreshTtlLong,
    MissingRefreshFailureHandler,
    AutoPromotionApplied,
    AutoRefreshNotSurfaced,
    CookieDomainMissing,
}

impl AuthRefreshDiagnostic {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::MissingSecretProvider => "AUTH-REFRESH-001",
            Self::GraceExceedsRefreshTtl => "AUTH-REFRESH-002",
            Self::MissingRotationColumns => "AUTH-REFRESH-003",
            Self::RevokeUserWithoutUserFk => "AUTH-REFRESH-004",
            Self::RefreshTtlLong => "AUTH-REFRESH-005",
            Self::MissingRefreshFailureHandler => "AUTH-REFRESH-006",
            Self::AutoPromotionApplied => "AUTH-REFRESH-007",
            Self::AutoRefreshNotSurfaced => "AUTH-REFRESH-008",
            Self::CookieDomainMissing => "AUTH-REFRESH-009",
        }
    }

    pub(super) fn severity(self) -> DoctorSeverity {
        match self {
            Self::MissingSecretProvider
            | Self::GraceExceedsRefreshTtl
            | Self::MissingRotationColumns => DoctorSeverity::Error,
            Self::RevokeUserWithoutUserFk | Self::RefreshTtlLong => DoctorSeverity::Warning,
            Self::MissingRefreshFailureHandler
            | Self::AutoPromotionApplied
            | Self::AutoRefreshNotSurfaced
            | Self::CookieDomainMissing => DoctorSeverity::Info,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Finding {
    pub diagnostic: AuthRefreshDiagnostic,
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

impl Finding {
    fn into_doctor(self) -> DoctorDiagnostic {
        DoctorDiagnostic {
            path: self.path,
            line: self.line,
            column: 1,
            severity: self.diagnostic.severity(),
            code: self.diagnostic.code().to_owned(),
            message: self.message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }
    }
}

pub(super) struct AuthRefreshContext<'a> {
    pub auth_facts: &'a [AuthFacts],
    pub feature_resources:
        &'a std::collections::BTreeMap<String, std::collections::BTreeMap<String, ResourceFact>>,
    pub feature_uses: &'a std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    pub app: Option<&'a DoctorAppManifest>,
    pub files: &'a [DoctorFile],
}

pub(super) fn diagnostics(
    auth_facts: &[AuthFacts],
    feature_resources: &std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, ResourceFact>,
    >,
    feature_uses: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    app: Option<&DoctorAppManifest>,
    files: &[DoctorFile],
) -> Vec<DoctorDiagnostic> {
    let ctx = AuthRefreshContext {
        auth_facts,
        feature_resources,
        feature_uses,
        app,
        files,
    };
    rules::check(&ctx)
        .into_iter()
        .map(Finding::into_doctor)
        .collect()
}
