use crate::config::Replacement;

fn rule_applies(rule: &Replacement, app_id: &str) -> bool {
    rule.enabled
        && (rule.apps.is_empty()
            || rule
                .apps
                .iter()
                .any(|app| app.eq_ignore_ascii_case(app_id)))
}

fn matches(rule: &Replacement, text: &str) -> bool {
    if rule.case_sensitive {
        text == rule.from
    } else {
        text.to_lowercase() == rule.from.to_lowercase()
    }
}

fn ends_with(rule: &Replacement, text: &str) -> bool {
    if rule.case_sensitive {
        text.ends_with(&rule.from)
    } else {
        let lower = text.to_lowercase();
        lower.ends_with(&rule.from.to_lowercase())
    }
}

pub fn immediate(word: &str, rules: &[Replacement], app_id: &str) -> Option<(usize, String)> {
    let mut best: Option<&Replacement> = None;

    for rule in rules {
        if rule.trigger != "immediate" || !rule_applies(rule, app_id) {
            continue;
        }
        if !ends_with(rule, word) {
            continue;
        }
        if best
            .map(|current| rule.from.chars().count() > current.from.chars().count())
            .unwrap_or(true)
        {
            best = Some(rule);
        }
    }

    best.map(|rule| (rule.from.chars().count(), rule.to.clone()))
}

pub fn apply_boundary(
    word: &str,
    separator: char,
    rules: &[Replacement],
    app_id: &str,
) -> Option<String> {
    let trigger = match separator {
        ' ' => "space",
        '\n' => "enter",
        '\t' => "tab",
        _ => "space",
    };

    let mut best: Option<&Replacement> = None;
    for rule in rules {
        if rule.trigger != trigger || !rule_applies(rule, app_id) {
            continue;
        }
        let matched = if rule.whole_word {
            matches(rule, word)
        } else {
            ends_with(rule, word)
        };
        if !matched {
            continue;
        }
        if best
            .map(|current| rule.from.chars().count() > current.from.chars().count())
            .unwrap_or(true)
        {
            best = Some(rule);
        }
    }

    let rule = best?;
    if rule.whole_word {
        return Some(rule.to.clone());
    }

    let original_count = word.chars().count();
    let suffix_count = rule.from.chars().count();
    if suffix_count > original_count {
        return None;
    }
    let prefix: String = word.chars().take(original_count - suffix_count).collect();
    Some(format!("{}{}", prefix, rule.to))
}

pub fn fix_stuck_shift(word: &str) -> Option<String> {
    if !word.chars().all(|c| c.is_alphabetic()) {
        return None;
    }
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    if chars.iter().all(|c| c.is_uppercase()) {
        return None;
    }

    for index in 1..chars.len() {
        if chars[index].is_uppercase() && chars[index - 1].is_uppercase() {
            let duplicate = chars[index]
                .to_lowercase()
                .eq(chars[index - 1].to_lowercase());
            let mut fixed = String::with_capacity(word.len());
            for (position, ch) in chars.iter().enumerate() {
                if position == index && duplicate {
                    continue;
                }
                if position == index {
                    fixed.extend(ch.to_lowercase());
                } else {
                    fixed.push(*ch);
                }
            }
            return Some(fixed);
        }
    }

    None
}

pub fn is_abbreviation(word: &str) -> bool {
    let lower = word.to_lowercase();
    matches!(
        lower.as_str(),
        "т" | "е" | "др" | "г" | "гг" | "ул" | "им" | "рис" | "стр" | "см" | "напр"
            | "etc" | "e.g" | "i.e" | "mr" | "mrs" | "dr" | "vs"
    ) || (word.chars().count() <= 2 && word.chars().all(|c| c.is_uppercase()))
}

pub fn list_continuation(previous_line: &str) -> Option<String> {
    let trimmed = previous_line.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let mut chars = trimmed.chars();
    let first = chars.next()?;

    if matches!(first, '-' | '*' | '•' | '+' | '—') {
        let rest = trimmed[first.len_utf8()..].trim_start();
        if rest.is_empty() {
            return None;
        }
        return Some(format!("{} ", first));
    }

    let marker_end = trimmed.find(' ')?;
    let marker = &trimmed[..marker_end];
    let rest = trimmed[marker_end..].trim_start();
    if rest.is_empty() {
        return None;
    }

    let delimiter = marker.chars().last()?;
    if delimiter != '.' && delimiter != ')' {
        return None;
    }
    let body = &marker[..marker.len() - delimiter.len_utf8()];

    if let Ok(number) = body.parse::<u32>() {
        return Some(format!("{}{} ", number + 1, delimiter));
    }

    let mut body_chars = body.chars();
    let letter = body_chars.next()?;
    if body_chars.next().is_some() || !letter.is_alphabetic() {
        return None;
    }

    let next = char::from_u32(letter as u32 + 1).filter(|c| c.is_alphabetic())?;

    Some(format!("{}{} ", next, delimiter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_replacements;

    #[test]
    fn applies_immediate_symbol_rule() {
        let rules = default_replacements();
        let value = immediate("a->", &rules, "app").expect("rule");
        assert_eq!(value, (2, "→".into()));
    }

    #[test]
    fn applies_space_rule_with_prefix() {
        let rules = default_replacements();
        let value = apply_boundary("foo(c)", ' ', &rules, "app").expect("rule");
        assert_eq!(value, "foo©");
    }

    #[test]
    fn fixes_stuck_shift() {
        assert_eq!(fix_stuck_shift("ППривет"), Some("Привет".into()));
        assert_eq!(fix_stuck_shift("ПРИВЕТ"), None);
        assert_eq!(fix_stuck_shift("iPhone"), None);
    }

    #[test]
    fn continues_numbered_list() {
        assert_eq!(list_continuation("1. Первый"), Some("2. ".into()));
        assert_eq!(list_continuation("a) пункт"), Some("b) ".into()));
        assert_eq!(list_continuation("- пункт"), Some("- ".into()));
        assert_eq!(list_continuation("просто текст"), None);
        assert_eq!(list_continuation("1. "), None);
    }
}
