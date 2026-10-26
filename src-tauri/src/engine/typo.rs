use super::context;
use super::dictionary;
use super::layout::language_of;

#[derive(Debug, Clone, PartialEq)]
pub struct TypoSuggestion {
    pub text: String,
    pub distance: usize,
    pub confidence: f32,
}

pub fn suggest(word: &str, language: &str, max_distance: usize) -> Option<TypoSuggestion> {
    if word.chars().count() < 3 {
        return None;
    }
    if !word.chars().all(|c| c.is_alphabetic()) {
        return None;
    }
    if word.chars().filter(|c| c.is_uppercase()).count() == word.chars().count() {
        return None;
    }
    if context::has_inner_uppercase(word) {
        return None;
    }
    if dictionary::dictionary().known(language, word) {
        return None;
    }

    let list = dictionary::dictionary().list(language)?;
    let source: Vec<char> = dictionary::normalize(word).chars().collect();
    let mut best: Option<(&'static str, usize, f32, f32)> = None;
    let mut rows = Matrix::new(source.len(), max_distance);

    for (candidate, candidate_chars) in list.words.iter().zip(list.chars.iter()) {
        if candidate_chars.len().abs_diff(source.len()) > max_distance {
            continue;
        }
        if candidate_chars.first() != source.first() {
            continue;
        }
        let Some(distance) = rows.distance(&source, candidate_chars, max_distance) else {
            continue;
        };
        let score = list.scores.get(candidate).copied().unwrap_or(0.0);
        if score < 3.0 {
            continue;
        }
        let penalty = keyboard_penalty(&source, candidate_chars);
        let better = match best {
            None => true,
            Some((_, best_distance, best_penalty, best_score)) => {
                distance < best_distance
                    || (distance == best_distance
                        && (penalty < best_penalty
                            || (penalty == best_penalty && score > best_score)))
            }
        };
        if better {
            best = Some((candidate, distance, penalty, score));
        }
    }

    let (candidate, distance, _, score) = best?;
    let confidence = match distance {
        1 => (score / 7.0).clamp(0.6, 0.99),
        _ => (score / 9.0).clamp(0.5, 0.9),
    };

    Some(TypoSuggestion {
        text: restore_case(word, candidate),
        distance,
        confidence,
    })
}

pub fn language_of_word(word: &str) -> Option<&'static str> {
    language_of(word)
}

fn restore_case(original: &str, replacement: &str) -> String {
    let chars: Vec<char> = original.chars().collect();
    let upper = chars.iter().filter(|c| c.is_uppercase()).count();
    if upper == chars.len() && !chars.is_empty() {
        return replacement.to_uppercase();
    }
    if chars.first().map(|c| c.is_uppercase()).unwrap_or(false)
        && chars[1..].iter().all(|c| !c.is_uppercase())
    {
        let mut result = String::new();
        for (index, ch) in replacement.chars().enumerate() {
            if index == 0 {
                result.extend(ch.to_uppercase());
            } else {
                result.push(ch);
            }
        }
        return result;
    }
    replacement.to_string()
}

const KEY_ROWS: [(&str, &str); 3] = [
    ("qwertyuiop", "йцукенгшщзхъ"),
    ("asdfghjkl", "фывапролджэ"),
    ("zxcvbnm", "ячсмитьбюё"),
];

fn key_position(ch: char) -> Option<(usize, usize)> {
    let lower = ch.to_lowercase().next()?;
    for (row, (latin, cyrillic)) in KEY_ROWS.iter().enumerate() {
        if let Some(col) = latin.chars().position(|c| c == lower) {
            return Some((row, col));
        }
        if let Some(col) = cyrillic.chars().position(|c| c == lower) {
            return Some((row, col));
        }
    }
    None
}

fn keys_adjacent(left: char, right: char) -> bool {
    let (Some((left_row, left_col)), Some((right_row, right_col))) =
        (key_position(left), key_position(right))
    else {
        return false;
    };
    if left_row == right_row {
        return left_col.abs_diff(right_col) == 1;
    }
    left_row.abs_diff(right_row) == 1 && left_col.abs_diff(right_col) <= 1
}

fn single_transposition(source: &[char], target: &[char]) -> bool {
    if source.len() != target.len() {
        return false;
    }
    let mismatches: Vec<usize> = source
        .iter()
        .zip(target)
        .enumerate()
        .filter_map(|(index, (left, right))| (left != right).then_some(index))
        .collect();
    mismatches.len() == 2
        && mismatches[1] == mismatches[0] + 1
        && source[mismatches[0]] == target[mismatches[1]]
        && source[mismatches[1]] == target[mismatches[0]]
}

fn keyboard_penalty(source: &[char], target: &[char]) -> f32 {
    if source.len() != target.len() {
        return 0.0;
    }
    if single_transposition(source, target) {
        return 0.0;
    }
    source
        .iter()
        .zip(target)
        .map(|(left, right)| {
            if left == right {
                0.0
            } else if keys_adjacent(*left, *right) {
                0.3
            } else {
                0.6
            }
        })
        .sum()
}

struct Matrix {
    prev_prev: Vec<usize>,
    prev: Vec<usize>,
    current: Vec<usize>,
}

impl Matrix {
    fn new(width: usize, _max: usize) -> Self {
        Self {
            prev_prev: vec![0; width + 2],
            prev: vec![0; width + 2],
            current: vec![0; width + 2],
        }
    }

    fn distance(&mut self, source: &[char], target: &[char], max: usize) -> Option<usize> {
        let n = source.len();
        let m = target.len();
        if n.abs_diff(m) > max {
            return None;
        }

        for (index, slot) in self.prev.iter_mut().take(m + 1).enumerate() {
            *slot = index;
        }
        self.prev_prev[..m + 1].copy_from_slice(&self.prev[..m + 1]);

        for i in 1..=n {
            self.current[0] = i;
            let mut row_min = i;

            for j in 1..=m {
                let cost = usize::from(source[i - 1] != target[j - 1]);
                let mut value = (self.prev[j] + 1)
                    .min(self.current[j - 1] + 1)
                    .min(self.prev[j - 1] + cost);

                if i > 1 && j > 1 && source[i - 1] == target[j - 2] && source[i - 2] == target[j - 1]
                {
                    value = value.min(self.prev_prev[j - 2] + 1);
                }

                self.current[j] = value;
                row_min = row_min.min(value);
            }

            if row_min > max {
                return None;
            }

            std::mem::swap(&mut self.prev_prev, &mut self.prev);
            std::mem::swap(&mut self.prev, &mut self.current);
        }

        let distance = self.prev[m];
        if distance <= max {
            Some(distance)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_russian_typo() {
        let suggestion = suggest("превет", "ru", 1).expect("suggestion");
        assert_eq!(suggestion.text, "привет");
    }

    #[test]
    fn fixes_transposition_in_english() {
        let suggestion = suggest("teh", "en", 1).expect("suggestion");
        assert_eq!(suggestion.text, "the");
    }

    #[test]
    fn leaves_correct_words_alone() {
        assert!(suggest("привет", "ru", 1).is_none());
        assert!(suggest("hello", "en", 1).is_none());
    }

    #[test]
    fn does_not_touch_urls_or_symbols() {
        assert!(suggest("http://x", "en", 1).is_none());
        assert!(suggest("a1b2", "en", 1).is_none());
    }

    #[test]
    fn does_not_touch_inner_case() {
        assert!(suggest("iPhone", "en", 1).is_none());
        assert!(suggest("МакДональдс", "ru", 1).is_none());
    }

    #[test]
    fn keyboard_distance_breaks_ties() {
        assert!(keys_adjacent('s', 'd'));
        assert!(keys_adjacent('ф', 'ы'));
        assert!(!keys_adjacent('s', 'o'));

        let source: Vec<char> = "slives".chars().collect();
        let slices: Vec<char> = "slices".chars().collect();
        let olives: Vec<char> = "olives".chars().collect();
        assert!(keyboard_penalty(&source, &slices) < keyboard_penalty(&source, &olives));
    }
}
