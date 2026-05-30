//! Tests for the Wave-W6 surface UX doctor rules (`ux_rules.rs`).

use lazuli_ir::{
    AudienceUx, Board, BuiltinType, CommandRef, Defaults, EnumDecl, EnumVariant, FieldConstraints,
    InlineTable, ListQuery, ListRender, Policies, PolicyRef, QualifiedName, QueryKind, QueryRef,
    RepeatableField, RepeatableGroup, Resource, SpanRef, Surface, SurfaceTarget, TabEntry,
    TabGroup, TabGroupCase, Tabs, TypeRef, ViewDetail, ViewList, ViewUx, Wizard, WizardStep,
    WizardSteps,
};

use super::*;

include!("ux_rules_tests_p1.rs");
include!("ux_rules_tests_p2.rs");
