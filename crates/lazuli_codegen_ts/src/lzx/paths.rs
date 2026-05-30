//! File-path + view-naming canon for `.lzx` emitters.
//!
//! Centralises the L0 #1 §4 file-layout convention
//! (`dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`) and
//! the spec-const / hook-name string shapes shared between the three
//! view-kind emitters.

use super::casing::{lower_camel, pascal_case};
use super::ir::SurfaceTarget;

/// File path for the emitted view hook per L0 #1 §4:
/// `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`.
pub(crate) fn view_file_path(
    target: SurfaceTarget,
    feature: &str,
    audience: &str,
    view_name: &str,
) -> String {
    let prefix = match target {
        SurfaceTarget::Web => "ts-web",
        SurfaceTarget::Mobile => "ts-mobile",
    };
    format!(
        "dist/{}/{}/views/{}/{}.gen.ts",
        prefix, feature, audience, view_name
    )
}

/// Spec const name: `<audience><View>View`. E.g. `adminSlugListView`.
pub(crate) fn view_spec_const(audience: &str, view_name: &str) -> String {
    let aud = lower_camel(audience);
    let view = pascal_case(view_name);
    format!("{}{}View", aud, view)
}

/// Hook name: `use<PascalAudience><PascalView>View`. E.g.
/// `useAdminSlugListView`. Note `slug` is not in the hook name — the
/// dist path already scopes by feature.
pub(crate) fn view_hook_name(audience: &str, view_name: &str) -> String {
    format!("use{}{}View", pascal_case(audience), pascal_case(view_name))
}

/// Pascal name for an `<Audience><View>` prefix (used for slot
/// interfaces and section types). E.g. `AdminSlugList`.
pub(crate) fn audience_view_pascal(audience: &str, view_name: &str) -> String {
    format!("{}{}", pascal_case(audience), pascal_case(view_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_file_path_matches_l0_1_canon() {
        let p = view_file_path(SurfaceTarget::Web, "slug", "admin", "slug_list");
        assert_eq!(p, "dist/ts-web/slug/views/admin/slug_list.gen.ts");

        let mobile = view_file_path(SurfaceTarget::Mobile, "item", "kiosk", "item_create");
        assert_eq!(mobile, "dist/ts-mobile/item/views/kiosk/item_create.gen.ts");
    }

    #[test]
    fn view_spec_const_and_hook_naming() {
        assert_eq!(view_spec_const("admin", "slug_list"), "adminSlugListView");
        assert_eq!(view_hook_name("admin", "slug_list"), "useAdminSlugListView");
        assert_eq!(
            view_spec_const("workspace-admin", "slug_detail"),
            "workspaceAdminSlugDetailView"
        );
        assert_eq!(
            view_hook_name("workspace-admin", "slug_detail"),
            "useWorkspaceAdminSlugDetailView"
        );
    }
}
