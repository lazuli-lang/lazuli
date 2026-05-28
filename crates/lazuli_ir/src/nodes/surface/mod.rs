//! Lzx ViewModel surface IR — `<feat>.<target>.lzx` lowered shape.
//!
//! A *Surface* is what an audience sees on one platform target. The
//! author writes one `<feature>.web.lzx` and one `<feature>.mobile.lzx`
//! (or fewer); the analyzer lowers each into one [`Surface`] carried on
//! `Feature.surfaces`. Codegen projects the surface into React /
//! React Native / future targets, but the language never talks transport
//! — the surface declares **what** the audience sees, never **how**
//! React Router or Expo wires it.
//!
//! ## Submodule map
//!
//! - [`core`] — `Surface`, `SurfaceTarget`, `Audience`, the `View` enum,
//!   `QueryRef` / `CommandRef` / `CellBinding` / `RouteParam`.
//! - [`views`] — concrete view shapes (`ViewList`, `ViewDetail`,
//!   `ViewCreate`), `OnSuccessSpec`, `FlashSpec`, `ListRender`.
//! - [`controls`] — search / filter / sort / selection declarations and
//!   the closed `SearchMode` / `FilterCardinality` / `SortDir` /
//!   `SelectionMode` catalogs. Also `BindingRef`.
//! - [`settings_and_drawer`] — view settings (enum/bool/int with
//!   persistence) + drawer sub-views.
//!
//! ## `on_success` orchestration
//!
//! [`OnSuccessSpec`] captures the post-submit orchestration for
//! `ViewCreate` views. The shape is a *declaration of intent*, not a
//! callback: codegen emits the JavaScript/React Router / Expo Router
//! moves; the language stays out of the navigation library.
//!
//! ## See also
//!
//! - `docs/proposals/lzx-integration-codegen.md` §5 (grammar) + §6
//!   (emission shapes).
//! - [`crate::PolicyAtom`] — atom used by [`Audience::requires`]
//!   (defined in crate root because it's shared with command / query /
//!   workflow policy expressions).
//! - [`crate::InvalidatesSpec`] — cache-invalidation declaration (shared
//!   with command/query lowering).
//! - [`crate::TranslationKeyRef`] — i18n key reference shape.

pub mod controls;
pub mod core;
pub mod settings_and_drawer;
pub mod ux;
pub mod views;

pub use core::{
    Audience, CellBinding, CommandRef, QueryKind, QueryRef, RouteParam, Surface, SurfaceTarget,
    View,
};

pub use controls::{
    BindingRef, FilterCardinality, FilterDecl, SearchDecl, SearchField, SearchMode, SelectionDecl,
    SelectionMode, SortDecl, SortDir,
};
pub use settings_and_drawer::{
    DrawerBindingSource, DrawerRouteBinding, DrawerSubView, DrawerTrigger, SettingDecl,
    SettingPersistence, SettingValueSpace,
};
pub use ux::{
    AudienceUx, InlineTable, RenderMode, TabEntry, TabGroup, TabGroupCase, Tabs, ViewUx, Wizard,
    WizardStep, WizardSteps,
};
pub use views::{FlashSpec, ListRender, OnSuccessSpec, ViewCreate, ViewDetail, ViewList};

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn surface_target_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(SurfaceTarget::Mobile).unwrap(),
            json!("mobile")
        );
    }

    #[test]
    fn view_tags_kind() {
        let v = View::List(ViewList {
            name: "customers".to_owned(),
            route: Some("/customers".to_owned()),
            source: QueryRef {
                feature: "customer".to_owned(),
                kind: QueryKind::List,
                name: "list".to_owned(),
            },
            render: ListRender::Table {
                columns: vec!["name".to_owned(), "email".to_owned()],
            },
            search: None,
            filter: vec![],
            cells: vec![],
            actions: vec![],
            drawer: None,
            sort: None,
            selection: None,
            settings: vec![],
            redacted_fields: vec![],
            ux: Default::default(),
            span_ref: None,
        });
        let value = serde_json::to_value(&v).unwrap();
        assert_eq!(value["kind"], json!("list"));
        assert_eq!(v.name(), "customers");
        assert_eq!(v.route(), Some("/customers"));
    }

    #[test]
    fn list_render_table_serializes_with_kind() {
        let r = ListRender::Table {
            columns: vec!["a".to_owned()],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["kind"], json!("table"));
        let back: ListRender = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn selection_mode_round_trips() {
        for m in [
            SelectionMode::None,
            SelectionMode::Single,
            SelectionMode::Multi,
        ] {
            let value = serde_json::to_value(m).unwrap();
            let back: SelectionMode = serde_json::from_value(value).unwrap();
            assert_eq!(back, m);
        }
    }

    #[test]
    fn binding_ref_round_trips_filter() {
        let b = BindingRef::Filter {
            name: "status".to_owned(),
        };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["kind"], json!("filter"));
        let back: BindingRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn surface_round_trips_through_json() {
        let s = Surface {
            feature: "customer".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![],
            span_ref: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        let back: Surface = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn on_success_omits_empty_slots() {
        let os = OnSuccessSpec {
            back: false,
            redirect: None,
            flash: None,
            invalidates: vec![],
            replace: false,
        };
        let v = serde_json::to_value(&os).unwrap();
        let obj = v.as_object().unwrap();
        assert!(
            obj.is_empty(),
            "OnSuccessSpec with all-default fields must serialize empty"
        );
    }
}
