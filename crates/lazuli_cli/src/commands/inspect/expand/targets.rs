//! Inference for omitted `target ...` clauses.
//!
//! When a `command` mutates an existing resource (`updates X` /
//! `deletes X`) under a feature that exposes a `query.lookup by_id by
//! id: ID` lookup, the canonical-source projector synthesises a
//! `target query.by_id(id: route.id)` line. This file owns the
//! traversal that walks the lines, finds the matching command bodies,
//! and inserts the inferred line.
//!
//! Lifted out of the `expand` god-file in the rails-style split.

use super::text_utils::leading_spaces;

pub(super) fn infer_local_targets(lines: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            output.extend(infer_local_targets_in_feature(&lines[start..index]));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

pub(super) fn infer_local_targets_in_feature(lines: &[String]) -> Vec<String> {
    if !feature_has_id_lookup(lines) {
        return lines.to_vec();
    }

    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            output.extend(infer_local_target_in_command(&lines[start..index]));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

pub(crate) fn feature_has_id_lookup(lines: &[String]) -> bool {
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("query.lookup by_id by id:") {
            return true;
        }

        if leading_spaces(line) == 4 && trimmed == "query.lookup by_id" {
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > 4 {
                if lines[child_index].trim_start() == "key id = params.id" {
                    return true;
                }
                child_index += 1;
            }
        }
    }

    false
}

pub(super) fn infer_local_target_in_command(lines: &[String]) -> Vec<String> {
    let has_target = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start().starts_with("target "));
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    if has_target || !has_route_id || !mutates_existing {
        return lines.to_vec();
    }

    let mut output = Vec::new();
    let mut inserted = false;

    for line in lines {
        if !inserted && leading_spaces(line) == 4 && line.trim_start().starts_with("policy ") {
            output.push("    target query.by_id(id: route.id)".to_owned());
            inserted = true;
        }

        output.push(line.to_owned());
    }

    output
}
