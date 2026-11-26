//! The emoji box's memory: which emoji the user picks most ("Frequently used") and the skin tone
//! they chose. Pure: no I/O. `effect.rs` reads and writes the file; this module only knows its
//! text form.
//!
//! The file is plain text, one line per emoji, `name count sequence`, plus one `#tone N` line
//! (N is 0 for none, 1 to 5). A name is never message text, only the name an emoji was sent under.
//! Lines that cannot be understood are skipped.

/// Most emoji "Frequently used" shows.
pub const MAX_FREQUENT: usize = 16;

/// A file larger than this is not read.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// Most names kept. Past it the least picked go, so the file cannot grow without end.
const MAX_NAMES: usize = 512;

/// Longest name kept, in bytes. Custom emoji names are short; anything longer is not a name.
const MAX_NAME_BYTES: usize = 100;

/// The line that holds the tone. A name cannot start with `#`, so it cannot be mistaken for one.
const TONE_KEY: &str = "#tone";

/// Picks and the skin tone.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Recents {
    /// `(name, times picked, sequence of the last pick)`. The sequence orders ties by recency.
    counts: Vec<(String, u32, u64)>,
    /// 0 for none, 1 to 5 for the tones.
    pub tone: u8,
}

/// Redacting `Debug`: lengths only, never a name.
impl std::fmt::Debug for Recents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Recents(<{}> names, tone {})",
            self.counts.len(),
            self.tone
        )
    }
}

/// True for a name the file may hold: not empty, short, no whitespace or control character and not
/// starting with the `#` of the tone line.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && !name.starts_with('#')
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl Recents {
    /// Count one pick of `name`. A name the file could not hold is ignored.
    pub fn record(&mut self, name: &str) {
        if !valid_name(name) {
            return;
        }
        let seq = self.counts.iter().map(|c| c.2).max().map_or(1, |m| m + 1);
        match self.counts.iter_mut().find(|c| c.0 == name) {
            Some(entry) => {
                entry.1 = entry.1.saturating_add(1);
                entry.2 = seq;
            }
            None => self.counts.push((name.to_string(), 1, seq)),
        }
        if self.counts.len() > MAX_NAMES {
            // Drop the one picked least, the oldest of those.
            if let Some(at) = self
                .counts
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| (c.1, c.2))
                .map(|(at, _)| at)
            {
                self.counts.remove(at);
            }
        }
    }

    /// How many times `name` was picked.
    #[cfg(test)]
    pub fn count_of(&self, name: &str) -> u32 {
        self.counts.iter().find(|c| c.0 == name).map_or(0, |c| c.1)
    }

    /// Whether anything was picked.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// The most picked names, most picked first and ties on the most recent pick, at most `limit`,
    /// leaving out the names `offered` says no to (they stay remembered).
    pub fn top(&self, limit: usize, mut offered: impl FnMut(&str) -> bool) -> Vec<&str> {
        let mut ranked: Vec<&(String, u32, u64)> = self.counts.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        ranked
            .into_iter()
            .map(|c| c.0.as_str())
            .filter(|name| offered(name))
            .take(limit)
            .collect()
    }

    /// The next tone of the cycle: none, 1, 2, 3, 4, 5 and back to none.
    pub fn cycle_tone(&mut self) {
        self.tone = (self.tone + 1) % 6;
    }

    /// Fold in what the file held, when it arrives after a pick of this session: the counts add up
    /// and the session's own tone choice, if any, wins.
    pub fn merge_loaded(&mut self, loaded: Recents) {
        let mut merged = loaded;
        let base = merged.counts.iter().map(|c| c.2).max().unwrap_or(0);
        for (name, count, seq) in std::mem::take(&mut self.counts) {
            match merged.counts.iter_mut().find(|c| c.0 == name) {
                Some(entry) => {
                    entry.1 = entry.1.saturating_add(count);
                    entry.2 = base + seq;
                }
                None => merged.counts.push((name, count, base + seq)),
            }
        }
        if self.tone != 0 {
            merged.tone = self.tone;
        }
        *self = merged;
    }

    /// Understand the text of the file: bad lines are skipped, the rest is used.
    pub fn parse(text: &str) -> Recents {
        let mut recents = Recents::default();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(name), Some(count)) = (parts.next(), parts.next()) else {
                continue;
            };
            if name == TONE_KEY {
                if let Ok(tone) = count.parse::<u8>()
                    && tone <= 5
                {
                    recents.tone = tone;
                }
                continue;
            }
            let Ok(count) = count.parse::<u32>() else {
                continue;
            };
            let seq = match parts.next() {
                Some(seq) => match seq.parse::<u64>() {
                    Ok(seq) => seq,
                    Err(_) => continue,
                },
                None => 0,
            };
            if count == 0
                || !valid_name(name)
                || recents.counts.len() >= MAX_NAMES
                || recents.counts.iter().any(|c| c.0 == name)
            {
                continue;
            }
            recents.counts.push((name.to_string(), count, seq));
        }
        recents
    }

    /// The text of the file.
    pub fn format(&self) -> String {
        let mut out = format!("{TONE_KEY} {}\n", self.tone);
        for (name, count, seq) in &self.counts {
            out.push_str(&format!("{name} {count} {seq}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picked(names: &[&str]) -> Recents {
        let mut recents = Recents::default();
        for name in names {
            recents.record(name);
        }
        recents
    }

    fn top(recents: &Recents) -> Vec<&str> {
        recents.top(MAX_FREQUENT, |_| true)
    }

    #[test]
    fn the_most_picked_comes_first() {
        let recents = picked(&["tada", "+1", "tada", "tada"]);
        assert_eq!(top(&recents), ["tada", "+1"]);
        assert_eq!(recents.count_of("tada"), 3);
    }

    #[test]
    fn a_tie_goes_to_the_one_picked_last() {
        let recents = picked(&["a", "b", "a", "b"]);
        assert_eq!(top(&recents), ["b", "a"]);
        let recents = picked(&["a", "b", "b", "a"]);
        assert_eq!(top(&recents), ["a", "b"]);
    }

    #[test]
    fn at_most_sixteen_are_shown() {
        let names: Vec<String> = (0..20).map(|n| format!("e{n}")).collect();
        let recents = picked(&names.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(recents.top(MAX_FREQUENT, |_| true).len(), 16);
    }

    #[test]
    fn a_name_that_is_not_offered_is_left_out_but_remembered() {
        let recents = picked(&["gone", "gone", "tada"]);
        assert_eq!(recents.top(16, |n| n != "gone"), ["tada"]);
        assert_eq!(recents.top(16, |_| true), ["gone", "tada"]);
        // The limit counts only what is offered.
        assert_eq!(recents.top(1, |n| n != "gone"), ["tada"]);
    }

    #[test]
    fn the_tone_cycles_through_none_and_five_and_back() {
        let mut recents = Recents::default();
        let seen: Vec<u8> = (0..7)
            .map(|_| {
                recents.cycle_tone();
                recents.tone
            })
            .collect();
        assert_eq!(seen, [1, 2, 3, 4, 5, 0, 1]);
    }

    #[test]
    fn the_file_text_round_trips() {
        let mut recents = picked(&["tada", "+1", "tada", "rtshipit"]);
        recents.tone = 3;
        assert_eq!(Recents::parse(&recents.format()), recents);
    }

    #[test]
    fn a_line_that_cannot_be_understood_is_skipped_and_the_rest_used() {
        let text = "tada 3 7\nnot-a-count x\nlonely\n\n+1 2\n#tone 9\n#tone 2\nzero 0 1\ndup 1 1\ndup 5 2\nbad name 1 1\n";
        let recents = Recents::parse(text);
        // `+1 2` has no sequence and reads as 0; `dup` keeps its first line; `bad name 1 1` has a
        // sequence that is not a number.
        assert_eq!(recents.top(16, |_| true), ["tada", "+1", "dup"]);
        assert_eq!(recents.count_of("tada"), 3);
        assert_eq!(recents.count_of("+1"), 2);
        assert_eq!(recents.count_of("dup"), 1);
        assert_eq!(recents.tone, 2, "an out-of-range tone line is skipped");
    }

    #[test]
    fn a_name_that_starts_with_the_tone_mark_or_holds_control_characters_is_never_kept() {
        let mut recents = Recents::default();
        recents.record("#tone");
        recents.record("a\u{1b}b");
        recents.record("");
        recents.record(&"x".repeat(MAX_NAME_BYTES + 1));
        assert!(recents.is_empty());
        let parsed = Recents::parse("#evil 4 1\nok 1 1\n");
        assert_eq!(parsed.top(16, |_| true), ["ok"]);
    }

    #[test]
    fn the_names_kept_are_capped() {
        let mut recents = Recents::default();
        for n in 0..MAX_NAMES + 10 {
            recents.record(&format!("e{n}"));
        }
        assert_eq!(recents.counts.len(), MAX_NAMES);
        // The oldest single pick went first.
        assert_eq!(recents.count_of("e0"), 0);
        assert_eq!(recents.count_of(&format!("e{}", MAX_NAMES + 9)), 1);
        let many: String = (0..MAX_NAMES + 50).map(|n| format!("e{n} 1 1\n")).collect();
        assert_eq!(Recents::parse(&many).counts.len(), MAX_NAMES);
    }

    #[test]
    fn loaded_counts_add_to_the_picks_made_before_they_arrived() {
        let mut recents = picked(&["tada"]);
        recents.tone = 4;
        let mut loaded = picked(&["tada", "tada", "+1"]);
        loaded.tone = 1;
        recents.merge_loaded(loaded);
        assert_eq!(recents.count_of("tada"), 3);
        assert_eq!(recents.count_of("+1"), 1);
        assert_eq!(recents.tone, 4);
        // The pick of this session is the most recent.
        assert_eq!(top(&recents)[0], "tada");
    }

    #[test]
    fn debug_prints_counts_and_never_a_name() {
        let recents = picked(&["secret_name_9f"]);
        let text = format!("{recents:?}");
        assert!(!text.contains("secret"), "{text}");
        assert!(text.contains("1"), "{text}");
    }
}
