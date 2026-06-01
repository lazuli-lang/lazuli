use super::*;
use super::emit::{DelegationCtx, StubDelegation, lookup_delegation};
use lazuli_ir::{
    Auth, AuthIdentity, AuthPassword, BuiltinType, CapabilityRef, Command, CommandEffect,
    CommandInput, CommandKind, Defaults, Expr, Extension, ExtensionContract, Feature,
    FieldConstraints, FieldRef, HashAlgorithm, HashedCapability, Path, PathRef, PathSource,
    Policies, PolicyRef, QualifiedName, TypeRef, TypedSlot,
};

include!("tests_p1.rs");
include!("tests_p2.rs");
include!("tests_smart_stub.rs");
