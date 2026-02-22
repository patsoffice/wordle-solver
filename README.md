# Wordle Solver

A Wordle solving assistant that suggests optimal guesses based on letter frequency analysis and word commonality. Available as both a CLI tool and an HTMX web interface with a Wordle-style grid.

## How It Works

1. Fetches the complete Wordle word list and past answers from the web
2. Filters out previously used answers and regular plurals (never valid Wordle answers)
3. Scores remaining words using a blend of letter frequency (how often each letter appears across candidates) and word commonality (how common the word is in everyday English)
4. After each guess, applies your feedback (green/yellow/grey) to narrow candidates and re-rank suggestions

## Usage

### CLI

```bash
cargo run
```

Enter your 5-letter guess, then provide feedback for each letter:

- `g` = green (correct letter, correct position)
- `y` = yellow (correct letter, wrong position)
- `x` = grey (letter not in the word)

```text
=== Wordle Solver ===
Top starter suggestions:
   1. steal  (2.87)
   2. swear  (2.85)
   ...

Enter guess (or 'q' to quit, '?' for help): crane
Enter feedback (g/y/x): xygxg

Constraints:
  Green:    __A_E
  Required: aer
  Excluded: cn
  Remaining candidates: 23

Top suggestions:
   1. oware  (1.65)
   ...
```

### Web Interface

```bash
cargo run --bin web
```

Open <http://localhost:3000>. Type letters into the Wordle-style grid, click tiles to cycle feedback colors (grey -> yellow -> green), and submit to get ranked suggestions. On mobile, tap the tile row to bring up the keyboard.

The header shows when the word data was last loaded, turning red if the data is more than 12 hours old. Click **Reload Data** to re-fetch word lists from the web without restarting the server.

## Data Sources

- **Word list**: [tabatkins/wordle-list](https://github.com/tabatkins/wordle-list) -- complete set of valid Wordle words
- **Past answers**: [Rock Paper Shotgun](https://www.rockpapershotgun.com/wordle-past-answers) -- scraped list of previously used answers
- **Word frequency**: [hermitdave/FrequencyWords](https://github.com/hermitdave/FrequencyWords) -- English word frequency from OpenSubtitles (used for commonality scoring and plural detection)

## Scoring

Words are ranked by a 50/50 blend of:

- **Letter score**: sum of letter presence frequencies (each letter counted once per word, normalized 0-1). Naturally penalizes repeated letters.
- **Commonality score**: log-normalized frequency from the OpenSubtitles corpus. Common words like "crane" rank higher than obscure ones.

## Plural Filtering

Regular plurals formed by adding "S" or "ES" are never valid Wordle answers ([confirmed by NYT, Nov 2022](https://www.nytimes.com)). The solver filters these out by checking if removing the suffix yields a valid root word. Irregular plurals (geese, fungi, teeth) are kept.

## Dependencies

- [reqwest](https://crates.io/crates/reqwest) -- HTTP client for fetching word data
- [scraper](https://crates.io/crates/scraper) -- HTML parsing for past answers
- [itertools](https://crates.io/crates/itertools) -- iterator utilities
- [axum](https://crates.io/crates/axum) -- web framework (web binary)
- [askama](https://crates.io/crates/askama) -- compiled HTML templates (web binary)
- [tokio](https://crates.io/crates/tokio) -- async runtime (web binary)

## Development

```bash
cargo fmt                                    # format code
cargo clippy --all-features --all-targets    # run lints
```

## License

This project is licensed under the [MIT License](LICENSE).
