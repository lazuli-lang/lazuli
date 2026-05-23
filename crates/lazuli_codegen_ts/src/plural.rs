/// Conservative English pluralization for generated TypeScript identifiers.
///
/// The helper intentionally covers only regular forms plus the evidence-backed
/// `quiz` case from Wave A.6. It is also idempotent for already-plural regular
/// identifiers such as `Payments`, so existing plural resource names do not
/// become `Paymentss` / `Paymentses`.
pub fn pluralize(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    let lower = word.to_ascii_lowercase();
    if is_likely_plural(&lower) {
        return word.to_owned();
    }

    if lower.ends_with("quiz") {
        return replace_ascii_suffix(word, "quiz", "quizzes");
    }

    if lower.ends_with('y') && preceding_char_is_consonant(&lower) {
        let stem = &word[..word.len() - 1];
        return format!("{stem}ies");
    }

    if lower.ends_with('s')
        || lower.ends_with('x')
        || lower.ends_with("ss")
        || lower.ends_with("sh")
        || lower.ends_with("ch")
    {
        return format!("{word}es");
    }

    format!("{word}s")
}

fn is_likely_plural(lower: &str) -> bool {
    lower.ends_with("ies")
        || (lower.ends_with('s')
            && !lower.ends_with("us")
            && !lower.ends_with("ss")
            && !lower.ends_with("sh")
            && !lower.ends_with("ch"))
}

fn preceding_char_is_consonant(lower: &str) -> bool {
    let mut chars = lower.chars().rev();
    let Some('y') = chars.next() else {
        return false;
    };
    let Some(prev) = chars.next() else {
        return false;
    };
    prev.is_ascii_alphabetic() && !matches!(prev, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn replace_ascii_suffix(word: &str, singular: &str, plural: &str) -> String {
    let stem_len = word.len() - singular.len();
    let suffix = &word[stem_len..];
    format!("{}{}", &word[..stem_len], match_suffix_case(suffix, plural))
}

fn match_suffix_case(original: &str, replacement_lower: &str) -> String {
    if original.chars().all(|ch| !ch.is_ascii_lowercase()) {
        return replacement_lower.to_ascii_uppercase();
    }
    if original
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        let mut chars = replacement_lower.chars();
        let mut out = String::with_capacity(replacement_lower.len());
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
        return out;
    }
    replacement_lower.to_owned()
}

#[cfg(test)]
mod tests {
    use super::pluralize;

    #[test]
    fn pluralize_regular_resource_names() {
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("host"), "hosts");
        assert_eq!(pluralize("payment"), "payments");
        assert_eq!(pluralize("property"), "properties");
        assert_eq!(pluralize("bus"), "buses");
        assert_eq!(pluralize("dish"), "dishes");
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(pluralize("quiz"), "quizzes");
    }

    #[test]
    fn pluralize_preserves_identifier_case() {
        assert_eq!(pluralize("Category"), "Categories");
        assert_eq!(pluralize("Property"), "Properties");
        assert_eq!(pluralize("Quiz"), "Quizzes");
        assert_eq!(
            pluralize("CustomServiceCategory"),
            "CustomServiceCategories"
        );
    }

    #[test]
    fn pluralize_leaves_regular_plural_identifiers_stable() {
        assert_eq!(pluralize("Payments"), "Payments");
        assert_eq!(pluralize("Hosts"), "Hosts");
        assert_eq!(pluralize("Properties"), "Properties");
    }
}
