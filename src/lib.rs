use itertools::Itertools;
use std::collections::{HashMap, HashSet};

// ---------- Word fetching ----------

pub fn used_words() -> HashSet<String> {
    let response =
        match reqwest::blocking::get("https://www.rockpapershotgun.com/wordle-past-answers") {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "Warning: couldn't fetch past answers: {}. Proceeding with full word list.",
                    e
                );
                return HashSet::new();
            }
        };

    let html_content = match response.text() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Warning: couldn't read past answers response: {}.", e);
            return HashSet::new();
        }
    };

    let document = scraper::Html::parse_document(&html_content);
    let div_selector = scraper::Selector::parse("div.article_body_content").unwrap();
    let ul_selector = scraper::Selector::parse("ul.inline").unwrap();
    let li_selector = scraper::Selector::parse("li").unwrap();

    let Some(div) = document.select(&div_selector).next() else {
        eprintln!("Warning: page structure changed, couldn't find article body.");
        return HashSet::new();
    };
    let Some(ul) = div.select(&ul_selector).next() else {
        eprintln!("Warning: page structure changed, couldn't find word list.");
        return HashSet::new();
    };

    let mut words = HashSet::new();
    for li in ul.select(&li_selector) {
        let text = li.text().collect::<Vec<_>>();
        if let Some(first) = text.first() {
            let word = first.trim().to_ascii_lowercase();
            if word.len() == 5 && word.chars().all(|c| c.is_ascii_lowercase()) {
                words.insert(word);
            }
        }
    }
    words
}

pub fn all_words() -> Result<HashSet<String>, String> {
    let response = reqwest::blocking::get(
        "https://raw.githubusercontent.com/tabatkins/wordle-list/refs/heads/main/words",
    )
    .map_err(|e| format!("Failed to fetch word list: {}", e))?;

    let content = response
        .text()
        .map_err(|e| format!("Failed to read word list: {}", e))?;

    let words: HashSet<String> = content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(words)
}

pub struct FrequencyData {
    pub commonality: HashMap<String, f64>,
    pub dictionary: HashSet<String>,
}

pub fn load_frequency_data(words: &HashSet<&String>) -> FrequencyData {
    let content = match reqwest::blocking::get(
        "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/en/en_50k.txt",
    ) {
        Ok(r) => match r.text() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Warning: couldn't read word frequency data: {}.", e);
                return FrequencyData {
                    commonality: HashMap::new(),
                    dictionary: HashSet::new(),
                };
            }
        },
        Err(e) => {
            eprintln!("Warning: couldn't fetch word frequency data: {}. Commonality scoring disabled.", e);
            return FrequencyData {
                commonality: HashMap::new(),
                dictionary: HashSet::new(),
            };
        }
    };

    let mut raw: HashMap<String, f64> = HashMap::new();
    let mut max_freq: f64 = 0.0;
    let mut dictionary = HashSet::new();

    for line in content.lines() {
        let Some((word, count_str)) = line.split_once(' ') else {
            continue;
        };
        // Build dictionary of all words (used for plural detection)
        dictionary.insert(word.to_string());

        if word.len() != 5 || !words.contains(&word.to_string()) {
            continue;
        }
        if let Ok(count) = count_str.parse::<f64>() {
            if count > max_freq {
                max_freq = count;
            }
            raw.insert(word.to_string(), count);
        }
    }

    if max_freq > 0.0 {
        let log_max = max_freq.ln();
        for freq in raw.values_mut() {
            *freq = freq.ln() / log_max;
        }
    }

    FrequencyData {
        commonality: raw,
        dictionary,
    }
}

// ---------- Regular plural filtering ----------

/// Returns true if the word is likely a regular plural (formed by adding S or ES).
/// Words like "glass" (ends in ss), "focus" (ends in us), "geese" (irregular) are NOT filtered.
/// Words like "spots" (spot+s), "foxes" (fox+es), "flies" (fly->flies) ARE filtered.
pub fn is_regular_plural(word: &str, dictionary: &HashSet<String>) -> bool {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() != 5 || chars[4] != 's' {
        return false;
    }

    // Ends in 'ss' -> not a regular plural (glass, cross, dress)
    if chars[3] == 's' {
        return false;
    }

    // word[0..4] is a valid word -> regular plural by adding 's' (spots, hands, bikes)
    let without_s: String = chars[..4].iter().collect();
    if dictionary.contains(&without_s) {
        return true;
    }

    // Ends in 'es' and word[0..3] is a valid word -> plural by adding 'es' (foxes, boxes)
    if chars[3] == 'e' {
        let without_es: String = chars[..3].iter().collect();
        if dictionary.contains(&without_es) {
            return true;
        }
    }

    // Ends in 'ies' and root+'y' is a valid word -> plural of y->ies (flies, spies)
    if chars[2] == 'i' && chars[3] == 'e' {
        let root_y = format!("{}y", chars[..2].iter().collect::<String>());
        if dictionary.contains(&root_y) {
            return true;
        }
    }

    false
}

/// Filter out regular plurals from a word list. Returns the count of words removed.
pub fn filter_regular_plurals(words: &mut Vec<String>, dictionary: &HashSet<String>) -> usize {
    let before = words.len();
    words.retain(|w| !is_regular_plural(w, dictionary));
    before - words.len()
}

// ---------- Game state & constraints ----------

pub struct GameState {
    pub greens: [Option<char>; 5],
    pub yellows_not_at: [HashSet<char>; 5],
    pub required_letters: HashSet<char>,
    pub excluded_letters: HashSet<char>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            greens: [None; 5],
            yellows_not_at: Default::default(),
            required_letters: HashSet::new(),
            excluded_letters: HashSet::new(),
        }
    }

    pub fn update(&mut self, guess: &str, feedback: &str) {
        let guess_chars: Vec<char> = guess.chars().collect();
        let feedback_chars: Vec<char> = feedback.chars().collect();

        // Pass 1: greens and yellows (so required_letters is populated before grey check)
        for i in 0..5 {
            let letter = guess_chars[i];
            match feedback_chars[i] {
                'g' => {
                    self.greens[i] = Some(letter);
                    self.required_letters.insert(letter);
                    self.excluded_letters.remove(&letter);
                }
                'y' => {
                    self.yellows_not_at[i].insert(letter);
                    self.required_letters.insert(letter);
                    self.excluded_letters.remove(&letter);
                }
                _ => {}
            }
        }

        // Pass 2: greys
        for i in 0..5 {
            let letter = guess_chars[i];
            if feedback_chars[i] == 'x' {
                if !self.required_letters.contains(&letter) {
                    self.excluded_letters.insert(letter);
                }
                self.yellows_not_at[i].insert(letter);
            }
        }
    }

    pub fn matches(&self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();

        for (i, &ch) in chars.iter().enumerate().take(5) {
            if let Some(expected) = self.greens[i] {
                if ch != expected {
                    return false;
                }
            }
        }

        for (i, &ch) in chars.iter().enumerate().take(5) {
            if self.yellows_not_at[i].contains(&ch) {
                return false;
            }
        }

        for &letter in &self.required_letters {
            if !chars.contains(&letter) {
                return false;
            }
        }

        for &ch in &chars {
            if self.excluded_letters.contains(&ch) {
                return false;
            }
        }

        true
    }

    pub fn display(&self) {
        let green_str: String = (0..5)
            .map(|i| match self.greens[i] {
                Some(c) => c.to_ascii_uppercase(),
                None => '_',
            })
            .collect();
        println!("  Green:    {}", green_str);

        let required: String = self.required_letters.iter().sorted().collect();
        if !required.is_empty() {
            println!("  Required: {}", required);
        }

        let excluded: String = self.excluded_letters.iter().sorted().collect();
        if !excluded.is_empty() {
            println!("  Excluded: {}", excluded);
        }
    }

    pub fn green_display(&self) -> String {
        (0..5)
            .map(|i| match self.greens[i] {
                Some(c) => c.to_ascii_uppercase(),
                None => '_',
            })
            .collect()
    }

    pub fn required_display(&self) -> String {
        self.required_letters.iter().sorted().collect()
    }

    pub fn excluded_display(&self) -> String {
        self.excluded_letters.iter().sorted().collect()
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- Scoring ----------

pub fn letter_presence_frequency(words: &[&String]) -> HashMap<char, f64> {
    let mut counts: HashMap<char, u32> = HashMap::new();
    let total = words.len() as f64;

    for word in words {
        let unique: HashSet<char> = word.chars().collect();
        for ch in unique {
            *counts.entry(ch).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .map(|(ch, count)| (ch, count as f64 / total))
        .collect()
}

pub fn score_word(word: &str, freq: &HashMap<char, f64>) -> f64 {
    let unique: HashSet<char> = word.chars().collect();
    unique.iter().filter_map(|ch| freq.get(ch)).sum()
}

pub fn rank_words<'a>(
    words: &[&'a String],
    commonality: &HashMap<String, f64>,
) -> Vec<(&'a String, f64)> {
    let freq = letter_presence_frequency(words);
    let has_commonality = !commonality.is_empty();

    let mut scored: Vec<(&String, f64)> = words
        .iter()
        .map(|w| {
            let letter_score = score_word(w, &freq);
            if has_commonality {
                let common_score = commonality.get(w.as_str()).copied().unwrap_or(0.0);
                (*w, 0.5 * letter_score + 0.5 * common_score)
            } else {
                (*w, letter_score)
            }
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

pub fn rank_words_owned(
    words: &[String],
    commonality: &HashMap<String, f64>,
) -> Vec<(String, f64)> {
    let word_refs: Vec<&String> = words.iter().collect();
    let freq = letter_presence_frequency(&word_refs);
    let has_commonality = !commonality.is_empty();

    let mut scored: Vec<(String, f64)> = words
        .iter()
        .map(|w| {
            let letter_score = score_word(w, &freq);
            if has_commonality {
                let common_score = commonality.get(w.as_str()).copied().unwrap_or(0.0);
                (w.clone(), 0.5 * letter_score + 0.5 * common_score)
            } else {
                (w.clone(), letter_score)
            }
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}
