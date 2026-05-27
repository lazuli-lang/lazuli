//! LSP hover + completion backends for the `lifecycle <field>`
//! resource-child block vocabulary (proposal §3).
//!
//! This module is intentionally separate from
//! `diagnostics::lifecycle`, which targets the route-guard family
//! (`requires_lifecycle X = …`, `resume` arms,
//! `on_lifecycle_pending`). This module is about the **block proper**
//! — the lifecycle declaration that lives as a child of
//! `resource <Name>` and declares `state` / `transition` /
//! `invariant` / `invariant_handler` children.
//!
//! Two public entry points are surfaced to `lib.rs::server`:
//! [`lifecycle_block_hover`] and [`lifecycle_block_completions`]. Both
//! gate fires on `enclosing_lifecycle_block` so unrelated tokens
//! elsewhere in the document don't surface block-vocab content.
//!
//! | Sub-module | Concern |
//! |---|---|
//! | [`context`] | Block-scope facts (`enclosing_lifecycle_block`, `enclosing_transition_block`). |
//! | [`hover`] | `lifecycle_block_hover` + 12+ hover constants. |
//! | [`completion`] | `lifecycle_block_completions` + per-context item catalogs. |

pub(crate) mod completion;
pub(crate) mod context;
pub(crate) mod hover;

#[allow(unused_imports)]
pub use completion::lifecycle_block_completions;
#[allow(unused_imports)]
pub(crate) use completion::{
    invariant_catalog_completion_items, lifecycle_child_completion_items,
    transition_child_completion_items,
};
#[allow(unused_imports)]
pub(crate) use context::{
    enclosing_lifecycle_block, enclosing_transition_block, LifecycleBlock, TransitionBlock,
};
#[allow(unused_imports)]
pub use hover::lifecycle_block_hover;
#[allow(unused_imports)]
pub(crate) use hover::{
    LIFECYCLE_AUDIT_HOVER, LIFECYCLE_BLOCK_HEADER_HOVER, LIFECYCLE_FROM_HOVER,
    LIFECYCLE_INITIAL_HOVER, LIFECYCLE_INVARIANT_HANDLER_HOVER, LIFECYCLE_INVARIANT_HOVER,
    LIFECYCLE_NO_JUMP_HOVER, LIFECYCLE_SINGLE_PER_HOVER, LIFECYCLE_STATE_HOVER,
    LIFECYCLE_TERMINAL_HOVER, LIFECYCLE_TERMINAL_IMMUTABLE_HOVER, LIFECYCLE_TESTS_HOVER,
    LIFECYCLE_TIMESTAMPS_HOVER, LIFECYCLE_TO_HOVER, LIFECYCLE_TRANSITION_HOVER,
};
