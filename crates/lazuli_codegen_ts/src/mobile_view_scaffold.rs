//! Per-view scaffold bodies for the mobile (Expo Router) target.
//!
//! `lazuli generate ts` walks every mobile surface and scaffolds one
//! `frontends/mobile/app/<audience>/<expo-route>.tsx` per view. The
//! files are written once (idempotent — `write_if_absent`) and never
//! overwritten by subsequent regen runs. Author replaces the
//! placeholder JSX with real RN components once the project's
//! component library is chosen.
//!
//! Plain React Native primitives only — no Tamagui / NativeWind /
//! Gluestack opinion (per `docs/proposals/mobile-target.md` §5.3).
//!
//! See `docs/proposals/mobile-target.md` §5.2 + §11 Cell C.3.

use lazuli_ir::View;

use crate::lzx::lzx_router_adapter::{RouterTarget, translate_route_path};
use crate::lzx::{view_hook_name, view_spec_const};

/// Compute the Expo Router file path for a view's `at "<route>"`, scoped
/// under an audience. Routes ending in a literal segment land at
/// `<segment>/index.tsx`; routes ending in a `:name` placeholder become
/// `<segment>/[name].tsx`. Returns the path relative to the project root.
pub fn expo_app_file_path(audience: &str, route: &str) -> String {
    let translated = translate_route_path(RouterTarget::Expo, route);
    let trimmed = translated.trim_matches('/');

    if trimmed.is_empty() {
        return format!("frontends/mobile/app/{}/index.tsx", audience);
    }

    let last_segment = trimmed.rsplit('/').next().unwrap_or("");
    let is_dynamic = last_segment.starts_with('[') && last_segment.ends_with(']');

    if is_dynamic {
        format!("frontends/mobile/app/{}/{}.tsx", audience, trimmed)
    } else {
        // Literal segment → directory with index.tsx.
        format!("frontends/mobile/app/{}/{}/index.tsx", audience, trimmed)
    }
}

/// Author the placeholder body for a scaffolded mobile route. Three
/// shapes — list, detail, create — keyed off the IR view kind. Every
/// body imports the matching generated hook and renders a minimal
/// `<SafeAreaView>` placeholder the author replaces.
pub fn scaffold_body_for_view(
    feature_name: &str,
    audience: &str,
    view: &View,
) -> String {
    let (view_name, kind) = match view {
        View::List(v) => (v.name.as_str(), "list"),
        View::Detail(v) => (v.name.as_str(), "detail"),
        View::Create(v) => (v.name.as_str(), "create"),
    };

    let hook = view_hook_name(audience, view_name);
    let spec = view_spec_const(audience, view_name);
    let import_path = format!("@/dist/ts-mobile/{}/views/{}/{}.gen", feature_name, audience, view_name);

    match kind {
        "list" => list_body(&hook, &spec, &import_path, view_name),
        "detail" => detail_body(&hook, &spec, &import_path, view_name),
        "create" => create_body(&hook, &spec, &import_path, view_name),
        _ => unreachable!(),
    }
}

fn list_body(hook: &str, spec: &str, import_path: &str, view_name: &str) -> String {
    format!(
        r#"// Scaffolded once by `lazuli generate ts`. Replace the placeholder JSX
// with your real list rendering — Lazuli will not overwrite this file.

import {{ FlatList, SafeAreaView, Text, View }} from "react-native";

import {{ {hook}, {spec} }} from "{import_path}";

export default function {view_name_pascal}Screen() {{
  const view = {hook}({{}});

  if (view.isLoading) {{
    return (
      <SafeAreaView style={{{{ flex: 1, alignItems: "center", justifyContent: "center" }}}}>
        <Text>Loading {view_name}…</Text>
      </SafeAreaView>
    );
  }}

  if (view.error) {{
    return (
      <SafeAreaView style={{{{ flex: 1, alignItems: "center", justifyContent: "center" }}}}>
        <Text>Failed to load {view_name}: {{String(view.error)}}</Text>
      </SafeAreaView>
    );
  }}

  return (
    <SafeAreaView style={{{{ flex: 1 }}}}>
      <FlatList
        data={{view.rows}}
        keyExtractor={{(item: any, index: number) => String(item?.id ?? index)}}
        renderItem={{({{ item }}: {{ item: any }}) => (
          <View style={{{{ paddingHorizontal: 16, paddingVertical: 12 }}}}>
            <Text>{{JSON.stringify(item)}}</Text>
          </View>
        )}}
      />
    </SafeAreaView>
  );
}}

// Spec ref: {spec}
"#,
        view_name_pascal = pascal(view_name),
    )
}

fn detail_body(hook: &str, spec: &str, import_path: &str, view_name: &str) -> String {
    format!(
        r#"// Scaffolded once by `lazuli generate ts`. Replace the placeholder JSX
// with your real detail rendering — Lazuli will not overwrite this file.

import {{ useLocalSearchParams }} from "expo-router";
import {{ SafeAreaView, ScrollView, Text, View }} from "react-native";

import {{ {hook}, {spec} }} from "{import_path}";

export default function {view_name_pascal}Screen() {{
  const params = useLocalSearchParams();
  const view = {hook}(params as any);

  if (view.isLoading) {{
    return (
      <SafeAreaView style={{{{ flex: 1, alignItems: "center", justifyContent: "center" }}}}>
        <Text>Loading {view_name}…</Text>
      </SafeAreaView>
    );
  }}

  if (view.error) {{
    return (
      <SafeAreaView style={{{{ flex: 1, alignItems: "center", justifyContent: "center" }}}}>
        <Text>Failed to load {view_name}: {{String(view.error)}}</Text>
      </SafeAreaView>
    );
  }}

  return (
    <SafeAreaView style={{{{ flex: 1 }}}}>
      <ScrollView contentContainerStyle={{{{ padding: 16 }}}}>
        <View>
          <Text>{{JSON.stringify(view.row ?? null, null, 2)}}</Text>
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}}

// Spec ref: {spec}
"#,
        view_name_pascal = pascal(view_name),
    )
}

fn create_body(hook: &str, spec: &str, import_path: &str, view_name: &str) -> String {
    format!(
        r##"// Scaffolded once by `lazuli generate ts`. Replace the placeholder JSX
// with your real form rendering — Lazuli will not overwrite this file.

import {{ Pressable, SafeAreaView, Text, View }} from "react-native";

import {{ {hook}, {spec} }} from "{import_path}";

export default function {view_name_pascal}Screen() {{
  const view = {hook}();

  return (
    <SafeAreaView style={{{{ flex: 1, padding: 16 }}}}>
      <View style={{{{ marginBottom: 16 }}}}>
        <Text>Replace this placeholder with your form fields.</Text>
      </View>
      <Pressable
        accessibilityRole="button"
        onPress={{() => view.submit({{}} as any)}}
        style={{{{ padding: 12, borderRadius: 8, backgroundColor: "#0a7" }}}}
      >
        <Text style={{{{ color: "white", textAlign: "center" }}}}>Submit</Text>
      </Pressable>
    </SafeAreaView>
  );
}}

// Spec ref: {spec}
"##,
        view_name_pascal = pascal(view_name),
    )
}

fn pascal(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expo_path_for_root_route() {
        assert_eq!(
            expo_app_file_path("buyer", "/"),
            "frontends/mobile/app/buyer/index.tsx"
        );
    }

    #[test]
    fn expo_path_for_literal_segment() {
        assert_eq!(
            expo_app_file_path("buyer", "/listings"),
            "frontends/mobile/app/buyer/listings/index.tsx"
        );
    }

    #[test]
    fn expo_path_for_dynamic_tail() {
        assert_eq!(
            expo_app_file_path("buyer", "/listings/:id"),
            "frontends/mobile/app/buyer/listings/[id].tsx"
        );
    }

    #[test]
    fn expo_path_for_deeply_nested_dynamic_tail() {
        assert_eq!(
            expo_app_file_path("admin", "/orgs/:org_id/users/:user_id"),
            "frontends/mobile/app/admin/orgs/[org_id]/users/[user_id].tsx"
        );
    }

    #[test]
    fn expo_path_strips_leading_and_trailing_slashes() {
        assert_eq!(
            expo_app_file_path("buyer", "/listings/"),
            "frontends/mobile/app/buyer/listings/index.tsx"
        );
    }

    #[test]
    fn pascal_lifts_snake_case_to_pascal() {
        assert_eq!(pascal("customer_list"), "CustomerList");
        assert_eq!(pascal("listing_detail"), "ListingDetail");
        assert_eq!(pascal("plain"), "Plain");
    }
}
