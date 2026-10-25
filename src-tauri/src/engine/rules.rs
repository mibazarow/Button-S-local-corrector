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
    let indent_length = previous_line.len() - previous_line.trim_start().len();
    let indent = &previous_line[..indent_length];
    let trimmed = previous_line.trim();
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
        return Some(format!("{indent}{} ", first));
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
        return Some(format!("{indent}{}{} ", number + 1, delimiter));
    }

    let mut body_chars = body.chars();
    let letter = body_chars.next()?;
    if body_chars.next().is_some() || !letter.is_alphabetic() {
        return None;
    }

    let next = char::from_u32(letter as u32 + 1).filter(|c| c.is_alphabetic())?;

    Some(format!("{indent}{}{} ", next, delimiter))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(list_continuation("  - вложенный"), Some("  - ".into()));
        assert_eq!(list_continuation("просто текст"), None);
        assert_eq!(list_continuation("1. "), None);
    }
}
