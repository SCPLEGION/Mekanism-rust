//! `WildcardMatcher`: the pattern of a tag or mod id filter. `*` is any run of characters, `?` any one,
//! `#` any one digit; the comparison ignores case.

/// `WildcardMatcher.matches(wildcard, text)`.
pub fn matches(wildcard: &str, text: &str) -> bool {
    let wildcard: Vec<char> = wildcard.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    run(&wildcard, &text, 0, 0, false)
}

/// Whether the pattern has any wildcard character, so a plain lookup does.
pub fn is_pattern(wildcard: &str) -> bool {
    wildcard.contains(['*', '?', '#'])
}

fn run(wildcard: &[char], text: &[char], start: usize, mut text_index: usize, continue_search: bool) -> bool {
    let mut at = start;
    while at < wildcard.len() {
        let wc = wildcard[at];
        let mut fail = false;
        if wc == '*' {
            return run(wildcard, text, at + 1, text_index, true);
        } else if text_index >= text.len() {
            return false;
        } else if wc == '#' {
            if !text[text_index].is_ascii_digit() {
                fail = true;
            }
        } else if wc != '?' && wc != text[text_index] {
            fail = true;
        }
        if fail {
            return continue_search && run(wildcard, text, start, text_index + 1, true);
        }
        text_index += 1;
        at += 1;
    }
    // Stop if there is text left and the pattern did not end in a `*`.
    text_index >= text.len() || wildcard.last() == Some(&'*')
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn plain_text_must_match_whole() {
        assert!(matches("c:ores", "c:ores"));
        assert!(matches("C:Ores", "c:ores"));
        assert!(!matches("c:ore", "c:ores"));
    }

    #[test]
    fn star_question_and_digit() {
        assert!(matches("c:ores/*", "c:ores/iron"));
        assert!(matches("*:ores", "c:ores"));
        assert!(matches("c:?res", "c:ores"));
        assert!(matches("tier#", "tier3"));
        assert!(!matches("tier#", "tierx"));
        assert!(matches("*", ""));
        assert!(!matches("c:*/iron", "c:ores/gold"));
    }
}
