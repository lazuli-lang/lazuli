//! Auto-photo emitter — generates `<feature>/auto_photo.gen.go` from
//! the four-command synth set produced by `@cap.File(...)` lowering
//! (request/confirm/clear/get_url). Each site emits a single
//! `lazuli.AutoPhotoSpec` value plus an `init()` that registers the
//! framework handlers; the runtime owns the bodies.

use super::casing::{gen_package_name, lower_camel, pascal_case};
use super::command::effect_resource_pascal;
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use lazuli_ir::{
    AutoPhotoCommandRole, Command, CommandEffect, CommandInput, Feature, Policies, PolicyRef,
    TypeRef,
};
use std::collections::BTreeMap;
/// Emit `<feature>/auto_photo.gen.go`, or `None` when the feature has no
/// `@cap.File`-synthesized commands.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_auto_photo_file("photoshare.lzi", &feature, "demo", &cross_index);
/// ```
pub fn emit_auto_photo_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    let _ = (module_name, cross_index);
    let sites = collect_auto_photo_sites(feature);
    if sites.is_empty() {
        return None;
    }
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/storage");
    let mut p = GoPrinter::new();
    p.banner(source_label, &gen_package_name(&feature.name));
    imports.emit(&mut p);
    p.blank();
    for (i, site) in sites.values().enumerate() {
        if i > 0 {
            p.blank();
        }
        emit_site(&mut p, feature, site);
    }
    Some(p.finish())
}
struct AutoPhotoSite<'a> {
    resource: String,
    field: String,
    commands: [Option<&'a Command>; 4],
}
fn collect_auto_photo_sites(feature: &Feature) -> BTreeMap<(String, String), AutoPhotoSite<'_>> {
    let mut sites = BTreeMap::new();
    for command in &feature.commands {
        let Some(marker) = command.synthesized_from_cap_file.as_ref() else {
            continue;
        };
        let key = (marker.resource.clone(), marker.field.clone());
        let site = sites.entry(key).or_insert_with(|| AutoPhotoSite {
            resource: marker.resource.clone(),
            field: marker.field.clone(),
            commands: [None, None, None, None],
        });
        site.commands[role_index(marker.role)].get_or_insert(command);
    }
    sites
}
fn emit_site(p: &mut GoPrinter, feature: &Feature, site: &AutoPhotoSite<'_>) {
    p.line("// ----------------------------------------------------------------------------");
    p.line(&format!("// Auto-photo: {}.{}", site.resource, site.field));
    p.line("// ----------------------------------------------------------------------------");
    p.blank();
    emit_spec(p, feature, site);
    p.blank();
    emit_init(p, feature, site);
}
fn emit_spec(p: &mut GoPrinter, feature: &Feature, site: &AutoPhotoSite<'_>) {
    let var_name = auto_photo_spec_var_name(feature, site);
    p.line(&format!("var {var_name} = lazuli.AutoPhotoSpec{{"));
    p.indent();
    let rows = vec![
        ("Feature:", format!("{:?},", feature.name)),
        ("Resource:", format!("{:?},", site.resource)),
        ("Field:", format!("{:?},", site.field)),
        ("Table:", format!("{:?},", lower_snake(&site.resource))),
        ("StoreBinding:", "\"object_store\",".to_owned()),
        ("BucketSlot:", "\"media\",".to_owned()),
        (
            "Contract:",
            format!(
                "storage.FileContract({}),",
                storage_var_name_for(feature, &site.field)
            ),
        ),
    ];
    let key_width = rows
        .iter()
        .map(|(key, _)| key.len())
        .chain(std::iter::once("PolicyAtoms:".len()))
        .max()
        .unwrap_or(0);
    for (key, value) in rows {
        p.line(&format!("{key:<key_width$} {value}"));
    }
    let atoms = policy_atoms_for(site, &feature.policies);
    if atoms.is_empty() {
        p.line(&format!("{:<key_width$} nil,", "PolicyAtoms:"));
    } else {
        p.line(&format!(
            "{:<key_width$} []lazuli.PolicyAtom{{",
            "PolicyAtoms:"
        ));
        p.indent();
        for atom in atoms {
            p.line(&format!("{{Namespace: {:?}, Name: {:?}}},", atom.0, atom.1));
        }
        p.dedent();
        p.line("},");
    }
    p.dedent();
    p.line("}");
}
fn emit_init(p: &mut GoPrinter, feature: &Feature, site: &AutoPhotoSite<'_>) {
    p.line("//lazuli:pattern auto_photo_register v1");
    p.line("func init() {");
    p.indent();
    let mut emitted = false;
    for (i, role) in [
        AutoPhotoCommandRole::Request,
        AutoPhotoCommandRole::Confirm,
        AutoPhotoCommandRole::Clear,
        AutoPhotoCommandRole::GetUrl,
    ]
    .into_iter()
    .enumerate()
    {
        let Some(command) = site.commands[i] else {
            continue;
        };
        if emitted {
            p.blank();
        }
        emitted = true;
        emit_register_fn(p, feature, site, command, role);
    }
    p.dedent();
    p.line("}");
}
fn emit_register_fn(
    p: &mut GoPrinter,
    feature: &Feature,
    site: &AutoPhotoSite<'_>,
    command: &Command,
    role: AutoPhotoCommandRole,
) {
    let qualified = format!("{}.{}", feature.name, command.name);
    let spec = auto_photo_spec_var_name(feature, site);
    match role {
        AutoPhotoCommandRole::Request => {
            let input = command_input_type(command);
            let output = command_output_type(command);
            emit_go(
                p,
                &format!(
                    "lazuli.RegisterFn({qualified:?},\n\tfunc(ctx *lazuli.Ctx, input {input}) ({output}, error) {{\n\t\tres, err := lazuli.AutoPhotoRequest(ctx, {spec}, lazuli.AutoPhotoRequestArgs{{\n\t\t\tContentType: input.ContentType,\n\t\t\tSizeBytes:   input.SizeBytes,\n\t\t}})\n\t\tif err != nil {{\n\t\t\treturn {output}{{}}, err\n\t\t}}\n\t\treturn {output}{{\n\t\t\tURL:                res.URL,\n\t\t\tMethod:             res.Method,\n\t\t\tHeadersContentType: res.HeadersContentType,\n\t\t\tKey:                res.Key,\n\t\t\tExpiresAt:          lazuli.Time(res.ExpiresAt),\n\t\t}}, nil\n\t}})"
                ),
            );
        }
        AutoPhotoCommandRole::Confirm => {
            let input = command_input_type(command);
            emit_go(
                p,
                &format!(
                    "lazuli.RegisterFn({qualified:?},\n\tfunc(ctx *lazuli.Ctx, input {input}) (struct{{}}, error) {{\n\t\treturn struct{{}}{{}}, lazuli.AutoPhotoConfirm(ctx, {spec}, lazuli.AutoPhotoConfirmArgs{{\n\t\t\tKey: input.Key,\n\t\t}})\n\t}})"
                ),
            );
        }
        AutoPhotoCommandRole::Clear => {
            emit_go(
                p,
                &format!(
                    "lazuli.RegisterFn({qualified:?},\n\tfunc(ctx *lazuli.Ctx, _ struct{{}}) (struct{{}}, error) {{\n\t\treturn struct{{}}{{}}, lazuli.AutoPhotoClear(ctx, {spec})\n\t}})"
                ),
            );
        }
        AutoPhotoCommandRole::GetUrl => {
            let output = command_output_type(command);
            emit_go(
                p,
                &format!(
                    "lazuli.RegisterFn({qualified:?},\n\tfunc(ctx *lazuli.Ctx, _ struct{{}}) ({output}, error) {{\n\t\tres, err := lazuli.AutoPhotoGetURL(ctx, {spec})\n\t\tif err != nil {{\n\t\t\treturn {output}{{}}, err\n\t\t}}\n\t\tif res.URL == \"\" {{\n\t\t\treturn {output}{{}}, nil\n\t\t}}\n\t\turl := res.URL\n\t\texpires := lazuli.Time(res.ExpiresAt)\n\t\treturn {output}{{\n\t\t\tURL:       &url,\n\t\t\tExpiresAt: &expires,\n\t\t}}, nil\n\t}})"
                ),
            );
        }
    }
}
fn emit_go(p: &mut GoPrinter, block: &str) {
    for line in block.lines() {
        p.line(line);
    }
}
fn command_input_type(command: &Command) -> String {
    match command.input {
        CommandInput::Empty => "struct{}".to_owned(),
        _ => command_input_struct_name(&command.name, &effect_resource_pascal(&command.effect)),
    }
}
fn command_output_type(command: &Command) -> String {
    match &command.effect {
        CommandEffect::Returns(ret) => match &ret.return_type {
            TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => pascal_case(&qname.name),
            _ => "struct{}".to_owned(),
        },
        _ => "struct{}".to_owned(),
    }
}
fn command_input_struct_name(short_name: &str, resource_pascal: &str) -> String {
    let mut parts = short_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = pascal_case(verb);
    out.push_str(resource_pascal);
    for word in parts {
        out.push_str(&pascal_case(word));
    }
    out.push_str("Input");
    out
}
fn auto_photo_spec_var_name(feature: &Feature, site: &AutoPhotoSite<'_>) -> String {
    format!(
        "{}{}AutoSpec",
        lower_camel(&feature.name),
        pascal_case(&site.field)
    )
}
fn storage_var_name_for(feature: &Feature, field_snake: &str) -> String {
    format!(
        "{}{}File",
        pascal_case(&feature.name),
        pascal_case(field_snake)
    )
}
fn policy_atoms_for(site: &AutoPhotoSite<'_>, policies: &Policies) -> Vec<(String, String)> {
    let Some(command) = site.commands.iter().copied().flatten().next() else {
        return Vec::new();
    };
    let atoms: Vec<&String> = match &command.policy {
        PolicyRef::Local(name) => policies
            .categories
            .iter()
            .find(|category| category.name == *name)
            .map(|category| category.atoms.iter().collect())
            .unwrap_or_default(),
        PolicyRef::Atom(atom) => vec![atom],
        _ => Vec::new(),
    };
    atoms
        .into_iter()
        .filter_map(|a| parse_policy_atom(a))
        .collect()
}
fn parse_policy_atom(raw: &str) -> Option<(String, String)> {
    let stripped = raw.strip_prefix('@').unwrap_or(raw);
    let mut parts = stripped.splitn(2, '.');
    let namespace = parts.next()?.trim();
    let name = parts.next()?.trim();
    if namespace.is_empty() || name.is_empty() {
        return None;
    }
    Some((namespace.to_owned(), name.to_owned()))
}
fn lower_snake(raw: &str) -> String {
    let mut out = String::new();
    for (i, ch) in raw.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
fn role_index(role: AutoPhotoCommandRole) -> usize {
    match role {
        AutoPhotoCommandRole::Request => 0,
        AutoPhotoCommandRole::Confirm => 1,
        AutoPhotoCommandRole::Clear => 2,
        AutoPhotoCommandRole::GetUrl => 3,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Feature, Module};
    #[test]
    fn emit_auto_photo_file_returns_none_when_no_synthesized_commands() {
        let feature = lower_feature("feature photoshare\n  defaults\n    tenancy org\n");
        let module = module_with(feature.clone());
        let index = CrossFeatureIndex::build(&module);
        let out = emit_auto_photo_file("test", &feature, "test-module", &index);
        assert!(out.is_none(), "no synth commands => no file");
    }
    #[test]
    fn emit_auto_photo_file_emits_one_init_block_per_site() {
        let feature = lower_feature(
            "feature photoshare\n  defaults\n    tenancy org\n\n  uses org\n  uses account\n\n  policies\n    photoshare_only: @scope.authenticated, @role.host\n\n  domain\n    resource PhotoShare\n      org: Org required\n      user: User required unique\n      avatar: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional\n",
        );
        let module = module_with(feature.clone());
        let index = CrossFeatureIndex::build(&module);
        let out = emit_auto_photo_file("test", &feature, "test-module", &index)
            .expect("synth commands => emitter returns Some");
        assert!(
            out.contains("photoshareAvatarAutoSpec"),
            "spec var missing: {out}"
        );
        assert_eq!(
            out.matches("func init() {").count(),
            1,
            "init block count drift: {out}"
        );
        for needle in [
            r#"lazuli.RegisterFn("photoshare.request_avatar_upload""#,
            r#"lazuli.RegisterFn("photoshare.confirm_avatar_upload""#,
            r#"lazuli.RegisterFn("photoshare.clear_avatar""#,
            r#"lazuli.RegisterFn("photoshare.get_avatar_url""#,
            r#""lazuli.dev/runtime/lazuli""#,
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
    }
    fn module_with(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }
    fn lower_feature(source: &str) -> Feature {
        let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
        lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers")
    }
}
