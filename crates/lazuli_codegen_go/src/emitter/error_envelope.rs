//! Mapping from known bucket sentinel errors to typed-error wrapping shapes.
//! Codegen consults this when emitting handler return paths so bucket
//! sentinels surface as the right typed envelope at the runtime boundary.
//! Per docs/proposals/bucket-ai-debug-loop-cycle.md §7.2 (envelope discipline).

use std::collections::BTreeSet;

use lazuli_ir::ExternalCallRef;

use super::patterns::{PATTERN_ERROR_WRAP_HELPER, emit_pattern_header};
use super::printer::GoPrinter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapShape {
    /// Wrap into *lazuli.FieldError with given Field name + Reason.
    Field {
        field: &'static str,
        reason: FieldReason,
        input_type: &'static str,
    },
    /// Wrap into *lazuli.PolicyError with placeholder Rule (codegen fills from policy DSL).
    Policy,
    /// Wrap into *lazuli.TenantError with placeholder Axis (codegen fills from tenancy DSL).
    Tenant,
    /// Wrap into *lazuli.AdapterError (codegen fills Adapter from integration ref).
    Adapter,
    /// Generic envelope - wrap into *lazuli.Error with Surface=UserDSL, Code="internal".
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldReason {
    Required,
    InvalidFormat,
    OutOfRange,
    Mismatch,
    UnknownEnum,
}

impl FieldReason {
    pub fn go_const(self) -> &'static str {
        match self {
            FieldReason::Required => "lazuli.FieldReasonRequired",
            FieldReason::InvalidFormat => "lazuli.FieldReasonInvalidFormat",
            FieldReason::OutOfRange => "lazuli.FieldReasonOutOfRange",
            FieldReason::Mismatch => "lazuli.FieldReasonMismatch",
            FieldReason::UnknownEnum => "lazuli.FieldReasonUnknownEnum",
        }
    }
}

/// Sentinel Go identifier (fully qualified) -> wrap shape.
/// Populated from existing runtime/go/lazuli/<bucket>/ sentinels.
pub const SENTINEL_MAPPINGS: &[(&str, WrapShape)] = &[
    // auth bucket
    (
        "auth.ErrPasswordMismatch",
        WrapShape::Field {
            field: "password",
            reason: FieldReason::Mismatch,
            input_type: "string",
        },
    ),
    ("auth.ErrSessionExpired", WrapShape::Generic),
    (
        "auth.ErrTokenInvalid",
        WrapShape::Field {
            field: "token",
            reason: FieldReason::Mismatch,
            input_type: "@cap.Token",
        },
    ),
    (
        "auth.ErrMfaCodeInvalid",
        WrapShape::Field {
            field: "code",
            reason: FieldReason::Mismatch,
            input_type: "string",
        },
    ),
    (
        "auth.ErrOAuthStateMismatch",
        WrapShape::Field {
            field: "state",
            reason: FieldReason::Mismatch,
            input_type: "string",
        },
    ),
    (
        "auth.ErrPasswordResetTokenInvalid",
        WrapShape::Field {
            field: "token",
            reason: FieldReason::Mismatch,
            input_type: "@cap.Token",
        },
    ),
    ("auth.ErrPasswordResetTokenExpired", WrapShape::Generic),
    (
        "auth.ErrEmailVerifyTokenInvalid",
        WrapShape::Field {
            field: "token",
            reason: FieldReason::Mismatch,
            input_type: "@cap.Token",
        },
    ),
    ("auth.ErrEmailVerifyTokenExpired", WrapShape::Generic),
    (
        "auth.ErrJWTInvalid",
        WrapShape::Field {
            field: "token",
            reason: FieldReason::Mismatch,
            input_type: "@cap.Token",
        },
    ),
    ("auth.ErrJWTExpired", WrapShape::Generic),
    (
        "auth.ErrJWTSignature",
        WrapShape::Field {
            field: "token",
            reason: FieldReason::Mismatch,
            input_type: "@cap.Token",
        },
    ),
];

pub fn lookup_wrap_shape(sentinel_path: &str) -> WrapShape {
    SENTINEL_MAPPINGS
        .iter()
        .find(|(name, _)| *name == sentinel_path)
        .map(|(_, shape)| *shape)
        .unwrap_or(WrapShape::Generic)
}

pub fn bucket_names_for_external_calls(calls: &[ExternalCallRef]) -> BTreeSet<&str> {
    calls
        .iter()
        .filter_map(|call| bucket_name_for_slot(&call.slot))
        .collect()
}

pub fn sentinel_buckets(buckets: &BTreeSet<&str>) -> BTreeSet<&'static str> {
    SENTINEL_MAPPINGS
        .iter()
        .filter_map(|(sentinel, _)| {
            let (bucket, _) = sentinel.split_once('.')?;
            buckets.contains(bucket).then_some(bucket)
        })
        .collect()
}

pub fn emit_wrap_helper(p: &mut GoPrinter, buckets: &BTreeSet<&str>) {
    emit_wrap_helper_named(p, "wrapErrorForHandler", buckets);
}

pub fn emit_wrap_helper_named(p: &mut GoPrinter, helper_name: &str, buckets: &BTreeSet<&str>) {
    let mut cases: Vec<(&str, WrapShape)> = SENTINEL_MAPPINGS
        .iter()
        .filter_map(|(sentinel, shape)| {
            let (bucket, _) = sentinel.split_once('.')?;
            buckets.contains(bucket).then_some((*sentinel, *shape))
        })
        .collect();
    cases.sort_by(|a, b| a.0.cmp(b.0));

    if cases.is_empty() {
        return;
    }

    p.line(&format!(
        "// {helper_name} wraps known bucket sentinels into typed errors."
    ));
    p.line("// One-wrap boundary per bucket-ai-debug-loop-cycle.md §7.2.");
    emit_pattern_header(p, PATTERN_ERROR_WRAP_HELPER);
    p.line(&format!(
        "func {helper_name}(ctx context.Context, err error) error {{"
    ));
    p.indent();
    p.line("if err == nil {");
    p.indent();
    p.line("return nil");
    p.dedent();
    p.line("}");
    p.line("switch {");
    for (sentinel, shape) in cases {
        p.line(&format!("case errors.Is(err, {sentinel}):"));
        p.indent();
        emit_wrap_return(p, shape);
        p.dedent();
    }
    p.line("default:");
    p.indent();
    emit_generic_return(p);
    p.dedent();
    p.line("}");
    p.dedent();
    p.line("}");
}

fn bucket_name_for_slot(slot: &str) -> Option<&str> {
    match slot {
        "auth" => Some("auth"),
        _ => None,
    }
}

fn emit_wrap_return(p: &mut GoPrinter, shape: WrapShape) {
    match shape {
        WrapShape::Field {
            field,
            reason,
            input_type,
        } => emit_field_return(p, field, reason, input_type),
        WrapShape::Generic => emit_generic_return(p),
        WrapShape::Policy => emit_policy_return(p),
        WrapShape::Tenant => emit_tenant_return(p),
        WrapShape::Adapter => emit_adapter_return(p),
    }
}

fn emit_field_return(
    p: &mut GoPrinter,
    field: &'static str,
    reason: FieldReason,
    input_type: &'static str,
) {
    p.line("return &lazuli.FieldError{");
    p.indent();
    p.line("Base: lazuli.ErrorBaseFromContext(ctx, lazuli.ErrorBase{");
    p.indent();
    p.line("Code:    \"field_invalid\",");
    p.line("Surface: lazuli.SurfaceUserDSL,");
    p.line("Status:  400,");
    p.line("Cause:   err,");
    p.dedent();
    p.line("}),");
    p.line(&format!("Field:     \"{field}\","));
    p.line(&format!("Reason:    {},", reason.go_const()));
    p.line(&format!("InputType: \"{input_type}\","));
    p.dedent();
    p.line("}");
}

fn emit_generic_return(p: &mut GoPrinter) {
    p.line("return &lazuli.Error{");
    p.indent();
    p.line("Base: lazuli.ErrorBaseFromContext(ctx, lazuli.ErrorBase{");
    p.indent();
    p.line("Code:    \"internal\",");
    p.line("Surface: lazuli.SurfaceUserDSL,");
    p.line("Status:  500,");
    p.line("Cause:   err,");
    p.dedent();
    p.line("}),");
    p.dedent();
    p.line("}");
}

fn emit_policy_return(p: &mut GoPrinter) {
    p.line("return &lazuli.PolicyError{");
    p.indent();
    p.line("Base: lazuli.ErrorBaseFromContext(ctx, lazuli.ErrorBase{Code: \"policy_denied\", Surface: lazuli.SurfaceUserDSL, Status: 403, Cause: err}),");
    p.dedent();
    p.line("}");
}

fn emit_tenant_return(p: &mut GoPrinter) {
    p.line("return &lazuli.TenantError{");
    p.indent();
    p.line("Base: lazuli.ErrorBaseFromContext(ctx, lazuli.ErrorBase{Code: \"tenant_mismatch\", Surface: lazuli.SurfaceUserDSL, Status: 403, Cause: err}),");
    p.dedent();
    p.line("}");
}

fn emit_adapter_return(p: &mut GoPrinter) {
    p.line("return &lazuli.AdapterError{");
    p.indent();
    p.line("Base: lazuli.ErrorBaseFromContext(ctx, lazuli.ErrorBase{Code: \"integration_error\", Surface: lazuli.SurfaceAdapterRuntime, Status: 502, Cause: err}),");
    p.dedent();
    p.line("}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_known_sentinel() {
        assert_eq!(
            lookup_wrap_shape("auth.ErrPasswordMismatch"),
            WrapShape::Field {
                field: "password",
                reason: FieldReason::Mismatch,
                input_type: "string",
            }
        );
    }

    #[test]
    fn lookup_returns_generic_for_unknown() {
        assert_eq!(
            lookup_wrap_shape("auth.ErrDoesNotExist"),
            WrapShape::Generic
        );
    }

    #[test]
    fn field_reason_go_const_round_trip() {
        let reasons = [
            (FieldReason::Required, "lazuli.FieldReasonRequired"),
            (
                FieldReason::InvalidFormat,
                "lazuli.FieldReasonInvalidFormat",
            ),
            (FieldReason::OutOfRange, "lazuli.FieldReasonOutOfRange"),
            (FieldReason::Mismatch, "lazuli.FieldReasonMismatch"),
            (FieldReason::UnknownEnum, "lazuli.FieldReasonUnknownEnum"),
        ];

        for (reason, go_const) in reasons {
            assert_eq!(reason.go_const(), go_const);
        }
    }
}
