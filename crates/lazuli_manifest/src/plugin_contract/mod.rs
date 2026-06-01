//! 0022 — the SHARED adapter-contract classifier.
//!
//! Single source of truth for both surfaces that assert the declared
//! adapter contract:
//!
//! - `lazuli plugin verify`'s **L3 contract** link
//!   (`lazuli_cli::commands::plugin::verify`), and
//! - the **`PLUGIN-CONTRACT-001`** doctor diagnostic
//!   (`lazuli_doctor_run`'s `lazurite_manifest` aggregator, tagged with
//!   `lazuli_doctor::correctness::plugin_contract_001::CODE`).
//!
//! Both call [`classify_adapter_contract`] over the SAME typed 0021
//! `PluginManifest` fields, so a misdeclared adapter can never be flagged
//! by one surface and silently passed by the other. The
//! `verify_and_doctor_agree_drift_guard` test in `lazuli_cli` pins the
//! agreement.
//!
//! ## What is (and is not) checked
//!
//! This is a STATIC, declared-contract check. It verifies:
//!
//! 1. every interface the adapter declares (`implements` + `[binds]
//!    .interface`, normalised to the `<pkg>.<Interface>` short form) is a
//!    member of the closed [`KNOWN_BUCKET_INTERFACES`] catalog (mirrors
//!    `runtime/go/lazuli/`), and
//! 2. for each known interface whose capability the app's registry binds
//!    to a DIFFERENT plugin ref, the binding is flagged
//!    ([`ContractStatus::UnboundCapability`]).
//!
//! It does NOT verify that the plugin's Go `Adapter` type actually
//! satisfies the interface's method set — the Rust compiler cannot run
//! `go build`. That conformance proof stays the plugin's runtime
//! `var _ <Interface> = (*Adapter)(nil)` assertion in `adapter.go`, which
//! fires under `go build` / `go test`. Every surface that renders a
//! contract result states this limit (see [`HONEST_LIMIT_NOTE`]).

use std::collections::BTreeMap;

use crate::plugin_manifest::PluginManifest;

/// The closed catalog of framework bucket interfaces an adapter may
/// `implements`. Mirrors the Go bucket packages under
/// `runtime/go/lazuli/`.
///
/// **Runtime-verified** (the `package.Interface` exists today as a Go
/// `interface` in the runtime tree):
/// - `payments.PaymentGateway` — `runtime/go/lazuli/payments/contract.go`
/// - `storage.ObjectStore` — `runtime/go/lazuli/storage/upload.go`
/// - `maps.Geocoder` — `runtime/go/lazuli/maps/contract.go`
///
/// **Forward-declared** (the public bucket contract per the 0022 ADR; the
/// concrete Go `interface` is named here but the runtime bucket has not
/// yet shipped the exact symbol — kept so adapters authored against the
/// blessed contract verify clean once the bucket lands):
/// - `notifications.EmailSender`
/// - `auth/social.Provider`
///
/// Growing this list is a deliberate one-line edit, guarded by
/// `bucket_catalog_pinned`. The `[binds].interface` self-reference shapes
/// some legacy plugins ship (`<self-module>.EmailSender`, bare `Provider`)
/// are intentionally NOT members — they are legacy spellings that
/// legitimately FAIL the contract check until migrated to the blessed
/// `<bucket>.<Interface>` form.
pub const KNOWN_BUCKET_INTERFACES: &[&str] = &[
    "payments.PaymentGateway",
    "storage.ObjectStore",
    "maps.Geocoder",
    "notifications.EmailSender",
    "auth/social.Provider",
];

/// The honest static-limit note appended wherever a contract result is
/// rendered (verify L3 line, `PLUGIN-CONTRACT-001` message tail, docs).
pub const HONEST_LIMIT_NOTE: &str = "(method-set conformance is verified at runtime by 'var _ <Interface> = (*Adapter)(nil)' in the plugin's adapter.go under go build)";

/// The app's view of capability → adapter bindings, as the contract check
/// needs it. Keyed by bucket interface (`payments.PaymentGateway`); the
/// value is the plugin ref the app binds that capability to.
///
/// v1 reality: the framework does not yet express capability bindings in a
/// statically-readable manifest shape, so the common case is an EMPTY view
/// (no bindings). An empty view means [`classify_adapter_contract`] never
/// raises [`ContractStatus::UnboundCapability`] — consistent with
/// `PLUGIN-UNUSED-001` being a warning, never a false FAIL on a
/// pre-binding pilot. The unbound-capability FAIL fires ONLY when the view
/// DOES bind the interface, to a DIFFERENT ref.
#[derive(Debug, Clone, Default)]
pub struct RegistryView {
    /// interface (`<pkg>.<Interface>`) → bound plugin ref.
    bindings: BTreeMap<String, String>,
}

impl RegistryView {
    /// An empty view — no capability bindings expressed. The common v1
    /// case; never produces an `UnboundCapability`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a view from an iterator of `(interface, plugin_ref)` pairs.
    pub fn from_bindings<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            bindings: pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    /// The plugin ref the app binds `interface` to, if any.
    fn bound_ref(&self, interface: &str) -> Option<&str> {
        self.bindings.get(interface).map(String::as_str)
    }
}

/// Outcome of classifying an adapter's declared contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractStatus {
    /// The plugin declares no `implements` / `[binds]` interface — there
    /// is no adapter contract to check (semantic-only plugins, or
    /// legacy `[provides]`-only manifests 0021 yields no typed interface
    /// for). Callers render this as `n/a`, never a FAIL.
    NotAnAdapter,
    /// Every declared interface is known and (where bound) bound to this
    /// plugin. The contract holds as far as static checking can prove.
    Ok,
    /// A declared interface is not in [`KNOWN_BUCKET_INTERFACES`].
    /// `declared` is the offending normalised interface; `nearest` is the
    /// smallest-edit-distance known interface for a "did you mean" hint.
    UnknownInterface {
        declared: String,
        nearest: &'static str,
    },
    /// A known interface whose capability the app binds to a DIFFERENT
    /// plugin ref. `capability` is the interface; `bound_to` is the ref
    /// the app actually bound it to.
    UnboundCapability {
        capability: String,
        bound_to: String,
    },
}

impl ContractStatus {
    /// True when this status represents a contract FAILURE (the surfaces
    /// gate / exit non-zero on these). `NotAnAdapter` and `Ok` are not
    /// failures.
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            ContractStatus::UnknownInterface { .. } | ContractStatus::UnboundCapability { .. }
        )
    }
}

/// Normalise a declared interface string into the blessed
/// `<pkg>.<Interface>` short form the catalog is keyed by.
///
/// Handles the heterogeneous legacy spellings 0021's typed fields carry
/// verbatim:
/// - `lazuli.dev/runtime/lazuli/auth/social.Provider` →
///   `auth/social.Provider` (strip the runtime module prefix).
/// - `github.com/lazuli-lang/lazuli-plugin-smtp.EmailSender` →
///   `lazuli-plugin-smtp.EmailSender` (strip the host/org path; the
///   self-referential package name remains — these legacy self-refs are
///   intentionally NOT in the catalog and FAIL, prompting migration).
/// - `payments.PaymentGateway` → unchanged (already short form).
/// - bare `Provider` → unchanged (no `.` package qualifier; FAILs as
///   unknown, prompting the author to qualify it).
pub fn normalise_interface(raw: &str) -> String {
    let trimmed = raw.trim();
    // Strip the canonical runtime module prefix so an adapter authored
    // against the fully-qualified runtime path matches the short catalog
    // key. Two spellings seen in the wild.
    for prefix in [
        "lazuli.dev/runtime/lazuli/",
        "lazuli.dev/runtime/",
        "lazuli.dev/",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

/// Collect the plugin's declared bucket interfaces from the 0021 typed
/// fields (`implements` + `[binds].interface`), normalised to short form
/// and de-duplicated in declaration order.
pub fn declared_interfaces(manifest: &PluginManifest) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let norm = normalise_interface(raw);
        if !norm.is_empty() && !out.contains(&norm) {
            out.push(norm);
        }
    };
    for iface in &manifest.implements {
        push(iface);
    }
    if let Some(binds) = manifest.binds.as_ref()
        && let Some(iface) = binds.interface.as_deref()
    {
        push(iface);
    }
    out
}

/// Classify an adapter plugin's DECLARED contract against the known
/// bucket-interface catalog + the app's registry view.
///
/// This is the ONE function both `plugin verify` (L3) and the
/// `PLUGIN-CONTRACT-001` doctor rule call — they cannot diverge.
///
/// Returns the FIRST failure found (unknown interface takes precedence
/// over unbound capability, so a typo is reported before a binding gap),
/// or `Ok` / `NotAnAdapter`.
pub fn classify_adapter_contract(
    manifest: &PluginManifest,
    plugin_ref: &str,
    registry: &RegistryView,
) -> ContractStatus {
    let declared = declared_interfaces(manifest);
    if declared.is_empty() {
        return ContractStatus::NotAnAdapter;
    }

    // First pass: any unknown interface is the highest-priority failure.
    for iface in &declared {
        if !KNOWN_BUCKET_INTERFACES.contains(&iface.as_str()) {
            return ContractStatus::UnknownInterface {
                declared: iface.clone(),
                nearest: nearest_known_interface(iface),
            };
        }
    }

    // Second pass: a known interface bound by the app to a DIFFERENT ref.
    for iface in &declared {
        if let Some(bound) = registry.bound_ref(iface)
            && bound != plugin_ref
        {
            return ContractStatus::UnboundCapability {
                capability: iface.clone(),
                bound_to: bound.to_string(),
            };
        }
    }

    ContractStatus::Ok
}

/// Smallest Levenshtein-distance member of [`KNOWN_BUCKET_INTERFACES`] —
/// the "did you mean" hint for an `UnknownInterface`.
pub fn nearest_known_interface(declared: &str) -> &'static str {
    KNOWN_BUCKET_INTERFACES
        .iter()
        .copied()
        .min_by_key(|known| levenshtein(declared, known))
        .unwrap_or("payments.PaymentGateway")
}

/// Classic two-row Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{PluginBindsContract, PluginManifest};

    fn adapter_with_implements(impls: &[&str]) -> PluginManifest {
        PluginManifest {
            implements: impls.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn bucket_catalog_pinned() {
        // Drift reminder: when a runtime bucket ships a new interface,
        // add it here AND to KNOWN_BUCKET_INTERFACES in the same edit.
        assert_eq!(
            KNOWN_BUCKET_INTERFACES,
            &[
                "payments.PaymentGateway",
                "storage.ObjectStore",
                "maps.Geocoder",
                "notifications.EmailSender",
                "auth/social.Provider",
            ]
        );
    }

    #[test]
    fn classify_unknown_interface_flags_typo() {
        let m = adapter_with_implements(&["payments.PaymentGatway"]);
        let status = classify_adapter_contract(&m, "@lazuli/plugin-x", &RegistryView::empty());
        assert_eq!(
            status,
            ContractStatus::UnknownInterface {
                declared: "payments.PaymentGatway".to_string(),
                nearest: "payments.PaymentGateway",
            }
        );
        assert!(status.is_failure());
    }

    #[test]
    fn classify_known_interface_unbound_capability() {
        let m = adapter_with_implements(&["payments.PaymentGateway"]);
        let registry = RegistryView::from_bindings([(
            "payments.PaymentGateway",
            "@lazuli/plugin-other",
        )]);
        let status = classify_adapter_contract(&m, "@lazuli/plugin-x", &registry);
        assert_eq!(
            status,
            ContractStatus::UnboundCapability {
                capability: "payments.PaymentGateway".to_string(),
                bound_to: "@lazuli/plugin-other".to_string(),
            }
        );
    }

    #[test]
    fn classify_known_interface_bound_is_ok() {
        let m = adapter_with_implements(&["payments.PaymentGateway"]);
        let registry =
            RegistryView::from_bindings([("payments.PaymentGateway", "@lazuli/plugin-x")]);
        let status = classify_adapter_contract(&m, "@lazuli/plugin-x", &registry);
        assert_eq!(status, ContractStatus::Ok);
    }

    #[test]
    fn classify_no_registry_bindings_is_ok() {
        let m = adapter_with_implements(&["payments.PaymentGateway"]);
        let status = classify_adapter_contract(&m, "@lazuli/plugin-x", &RegistryView::empty());
        assert_eq!(status, ContractStatus::Ok);
    }

    #[test]
    fn classify_semantic_only_plugin_is_na() {
        let m = PluginManifest::default();
        let status = classify_adapter_contract(&m, "@lazuli/plugin-x", &RegistryView::empty());
        assert_eq!(status, ContractStatus::NotAnAdapter);
        assert!(!status.is_failure());
    }

    #[test]
    fn binds_interface_normalised_runtime_prefix_stripped() {
        let m = PluginManifest {
            binds: Some(PluginBindsContract {
                interface: Some("lazuli.dev/runtime/lazuli/auth/social.Provider".to_string()),
                methods: vec![],
            }),
            ..Default::default()
        };
        assert_eq!(declared_interfaces(&m), vec!["auth/social.Provider"]);
        assert_eq!(
            classify_adapter_contract(&m, "@lazuli/plugin-x", &RegistryView::empty()),
            ContractStatus::Ok
        );
    }

    #[test]
    fn legacy_self_referential_interface_fails_unknown() {
        // smtp's `[binds].interface = "<self-module>.EmailSender"` is a
        // legacy self-reference, NOT notifications.EmailSender — it FAILs.
        let m = PluginManifest {
            binds: Some(PluginBindsContract {
                interface: Some(
                    "github.com/lazuli-lang/lazuli-plugin-smtp.EmailSender".to_string(),
                ),
                methods: vec![],
            }),
            ..Default::default()
        };
        let status = classify_adapter_contract(&m, "@lazuli/plugin-smtp", &RegistryView::empty());
        assert!(matches!(status, ContractStatus::UnknownInterface { .. }));
    }
}
