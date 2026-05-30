//! Struct + `Resource[T]` value emission tests. The wide-spread fan
//! of `emit_resource_file` checks for: empty-feature short-circuit,
//! field type lowering (builtins, capability refs, PostGIS,
//! `@cap.File`), retention / tenancy / timestamps / soft-delete
//! lifting, semantic-plugin validate-tag wiring, optional + derived
//! field shape, record carve-out, deterministic ordering, and
//! cross-feature user-defined refs.
//!
//! Anything that exercises the *struct shape* lives here. Encryption
//! helper bodies, json-skip sentinels, and the feature-level entry
//! point have their own siblings.

#![cfg(test)]

use super::CrossFeatureIndex;
use super::emit_resource_file;
use super::test_support::{
    base_feature, default_app_manifest, emit, simple_field, simple_resource,
};
use lazuli_ir::{
    BuiltinType, CapabilityRef, EncryptedCapability, Field, Module, Record, Resource,
    RetentionAction, RetentionSpec, Tenancy, TypeRef,
};

include!("struct_emit_p1_tests.rs");
include!("struct_emit_p2_tests.rs");
