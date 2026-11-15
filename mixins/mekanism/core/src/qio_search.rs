//! `SearchQueryParser`: the QIO item viewer's search. A query is `|`-separated alternatives; each one
//! ANDs its parts, and a part is a key list with an optional prefix: `@mod`, `$tooltip`, `#tag`, none
//! for the name. A key list is a word, a quoted text or a parenthesised `(a|b)` list. Everything is
//! compared lower case.

/// What a part looks at (`QueryType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Name,
    Mod,
    Tooltip,
    Tag,
}

impl Kind {
    fn prefix(c: char) -> Option<Kind> {
        match c {
            '@' => Some(Kind::Mod),
            '$' => Some(Kind::Tooltip),
            '#' => Some(Kind::Tag),
            _ => None,
        }
    }
}

/// The characters that start a part's kind (`QueryType.getPrefixChars`).
pub const PREFIXES: [char; 3] = ['@', '$', '#'];

/// One AND of parts: every kind needs one of its keys to match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Part {
    pub keys: Vec<(Kind, Vec<String>)>,
}

/// The parsed search: any of its alternatives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub any: Vec<Part>,
}

/// What a query asks of an item.
pub trait Item {
    fn name(&self) -> String;
    fn mod_id(&self) -> String;
    fn tooltip(&self) -> Vec<String>;
    fn tags(&self) -> Vec<String>;
}

impl Query {
    /// Whether the item passes (an invalid or empty query passes everything, as the viewer's
    /// `ISearchQuery.INVALID` does not filter).
    pub fn test(&self, item: &dyn Item) -> bool {
        if self.any.is_empty() {
            return true;
        }
        self.any.iter().any(|part| {
            part.keys.iter().all(|(kind, keys)| {
                keys.iter().any(|key| match kind {
                    Kind::Name => item.name().to_lowercase().contains(key),
                    Kind::Mod => item.mod_id().to_lowercase().contains(key),
                    Kind::Tooltip => item.tooltip().iter().any(|l| l.to_lowercase().contains(key)),
                    Kind::Tag => item.tags().iter().any(|t| t.to_lowercase().contains(key)),
                })
            })
        })
    }
}

const TERMINATORS: [char; 4] = ['|', '(', '"', '\''];

/// Parses `query` (`SearchQueryParser.parse`, after the viewer lower-cases it). None is the invalid query:
/// an unterminated quote or list.
pub fn parse(query: &str) -> Option<Query> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(Query::default());
    }
    let chars: Vec<char> = query.chars().collect();
    let mut any: Vec<Part> = Vec::new();
    let mut cur = Part::default();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '|' {
            if !cur.keys.is_empty() {
                any.push(std::mem::take(&mut cur));
            }
            i += 1;
            continue;
        }
        if c == ' ' {
            i += 1;
            continue;
        }
        let kind = match Kind::prefix(c) {
            Some(k) => {
                i += 1;
                k
            }
            None => Kind::Name,
        };
        // `readKeyList`
        if i >= chars.len() {
            break;
        }
        let (keys, end) = match chars[i] {
            '(' => read_list(&chars, i)?,
            '"' | '\'' => {
                let (s, e) = read_quote(&chars, i)?;
                (vec![s], e)
            }
            _ => {
                let (s, e) = read_until(&chars, i, kind != Kind::Name);
                (vec![s], e)
            }
        };
        if !keys.is_empty() {
            // The part keeps one key list a kind (`EnumMap.put`).
            match cur.keys.iter_mut().find(|(k, _)| *k == kind) {
                Some(e) => e.1 = keys,
                None => cur.keys.push((kind, keys)),
            }
        }
        i = end + 1;
    }
    if !cur.keys.is_empty() {
        any.push(cur);
    }
    Some(Query { any })
}

fn read_list(c: &[char], start: usize) -> Option<(Vec<String>, usize)> {
    let mut out = Vec::new();
    let mut sb = String::new();
    let mut i = start + 1;
    while i < c.len() {
        match c[i] {
            ')' => {
                let k = sb.trim().to_owned();
                if !k.is_empty() {
                    out.push(k);
                }
                return Some((out, i));
            }
            '|' => {
                let k = sb.trim().to_owned();
                if !k.is_empty() {
                    out.push(k);
                }
                sb.clear();
            }
            '"' | '\'' => {
                let (s, e) = read_quote(c, i)?;
                out.push(s);
                i = e;
            }
            other => sb.push(other),
        }
        i += 1;
    }
    None
}

fn read_quote(c: &[char], start: usize) -> Option<(String, usize)> {
    let q = c[start];
    let mut s = String::new();
    for (i, &ch) in c.iter().enumerate().skip(start + 1) {
        if ch == q {
            return Some((s, i));
        }
        s.push(ch);
    }
    None
}

/// Reads to a terminator and returns the text and the index of its last character.
fn read_until(c: &[char], start: usize, space_ends: bool) -> (String, usize) {
    let mut s = String::new();
    let mut i = start;
    while i < c.len() {
        let ch = c[i];
        if TERMINATORS.contains(&ch) || Kind::prefix(ch).is_some() || (space_ends && ch == ' ') {
            return (s.trim().to_owned(), i - 1);
        }
        s.push(ch);
        i += 1;
    }
    (s.trim().to_owned(), i.saturating_sub(1).max(start.saturating_sub(1)).min(c.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct It(&'static str, &'static str);
    impl Item for It {
        fn name(&self) -> String {
            self.0.to_owned()
        }
        fn mod_id(&self) -> String {
            self.1.to_owned()
        }
        fn tooltip(&self) -> Vec<String> {
            vec![]
        }
        fn tags(&self) -> Vec<String> {
            vec!["c:ingots/iron".to_owned()]
        }
    }

    #[test]
    fn name_words_are_one_key() {
        let q = parse("iron ingot").unwrap();
        assert!(q.test(&It("Iron Ingot", "minecraft")));
        assert!(!q.test(&It("Iron Block", "minecraft")));
    }

    #[test]
    fn prefixes_and_alternatives() {
        let q = parse("@mekanism steel|#ingots").unwrap();
        assert!(q.test(&It("Steel Casing", "mekanism")));
        assert!(q.test(&It("Anything", "minecraft")));
        let q = parse("@mekanism steel").unwrap();
        assert!(!q.test(&It("Steel Casing", "minecraft")));
    }

    #[test]
    fn lists_and_quotes() {
        let q = parse("(iron|gold)").unwrap();
        assert!(q.test(&It("Gold Ore", "minecraft")));
        assert!(!q.test(&It("Copper Ore", "minecraft")));
        assert!(parse("\"iron").is_none());
        assert!(parse("(iron").is_none());
        assert!(parse("").unwrap().test(&It("x", "y")));
    }
}
