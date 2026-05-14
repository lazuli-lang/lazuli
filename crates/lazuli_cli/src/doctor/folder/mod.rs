//! Lazurite frontend folder canon Doctor rules.
//!
//! Per `docs/proposals/lazurite-frontend-folder-canon.md` §5.1, this
//! sub-namespace hosts rules that enforce the canonical Lazurite
//! frontend file layout:
//!
//! - [`feature_orphan`] — flags `.tsx`/`.ts` files outside the
//!   canonical `app/features/<feat>/{web,mobile}/{cells,views/<audience>/}`
//!   or `frontends/<target>/{shell,theme,ui,lib,hooks}/` layout.
//! - [`pages_bypass`] — flags filesystem routing markers (Next.js
//!   Pages Router `pages/`, App Router route groups `app/(<group>)/`)
//!   when Lazurite routes come from `.lzx` `view ... at "<route>"`
//!   declarations.
//! - [`type_duplicate`] — flags user-authored `.ts(x)` declaring an
//!   `interface` or `type` whose name collides with a generated
//!   `dist/ts-web/<feat>/<feat>.gen.ts` type.
//! - [`cross_feature_import`] — flags imports of
//!   `app/features/B/{web,mobile}/<...>` from inside `app/features/A/<...>`
//!   when `A != B`; cross-feature visual dependencies must go via
//!   slot bindings or shared `frontends/<target>/ui/` primitives.

pub mod cross_feature_import;
pub mod feature_orphan;
pub mod pages_bypass;
pub mod type_duplicate;
