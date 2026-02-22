use std::{
    collections::HashSet,
    io::{self, Write},
};
use wordle_word::*;

// ---------- Input handling ----------

fn read_line() -> String {
    let mut input = String::new();
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn print_help() {
    println!();
    println!("Usage:");
    println!("  Enter your 5-letter Wordle guess, then provide feedback:");
    println!("    g = green  (correct letter, correct position)");
    println!("    y = yellow (correct letter, wrong position)");
    println!("    x = grey   (letter not in the word)");
    println!();
    println!("  Example: if you guessed 'crane' and got green-yellow-grey-grey-green,");
    println!("           enter feedback: gyxxg");
    println!();
    println!("  Commands:");
    println!("    q = quit");
    println!("    ? = show this help");
    println!("    s = show current constraints");
}

fn display_suggestions(ranked: &[(&String, f64)], limit: usize) {
    for (i, (word, score)) in ranked.iter().take(limit).enumerate() {
        println!("  {:>2}. {}  ({:.2})", i + 1, word, score);
    }
}

// ---------- Main ----------

fn main() {
    println!("=== Wordle Solver ===");
    println!("Fetching word lists...");

    let all = match all_words() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let used = used_words();

    let available: HashSet<&String> = all.difference(&used).collect();
    let freq_data = load_frequency_data(&available);

    let mut candidate_strs: Vec<String> = available.iter().map(|s| s.to_string()).collect();
    let plurals_removed = filter_regular_plurals(&mut candidate_strs, &freq_data.dictionary);
    let candidate_set: HashSet<String> = candidate_strs.into_iter().collect();

    println!(
        "{} total words, {} past answers excluded, {} regular plurals filtered, {} candidates available.\n",
        all.len(),
        used.len(),
        plurals_removed,
        candidate_set.len()
    );

    let mut candidates: Vec<&String> = all.iter().filter(|w| candidate_set.contains(*w)).collect();
    let mut state = GameState::new();

    println!("Top starter suggestions:");
    let ranked = rank_words(&candidates, &freq_data.commonality);
    display_suggestions(&ranked, 15);

    loop {
        println!();
        print!("Enter guess (or 'q' to quit, '?' for help): ");
        let guess = read_line().to_ascii_lowercase();

        if guess == "q" {
            break;
        }
        if guess == "?" {
            print_help();
            continue;
        }
        if guess == "s" {
            println!("\nCurrent constraints:");
            state.display();
            println!("  Remaining candidates: {}", candidates.len());
            continue;
        }

        if guess.len() != 5 || !guess.chars().all(|c| c.is_ascii_lowercase()) {
            println!("Guess must be exactly 5 lowercase letters.");
            continue;
        }

        print!("Enter feedback (g/y/x): ");
        let feedback = read_line().to_ascii_lowercase();

        if feedback.len() != 5 || !feedback.chars().all(|c| matches!(c, 'g' | 'y' | 'x')) {
            println!("Feedback must be exactly 5 characters, each g, y, or x.");
            continue;
        }

        if feedback == "ggggg" {
            println!("Congratulations! You solved it: {}", guess);
            break;
        }

        state.update(&guess, &feedback);
        candidates.retain(|w| state.matches(w));

        println!("\nConstraints:");
        state.display();
        println!("  Remaining candidates: {}", candidates.len());

        if candidates.is_empty() {
            println!("\nNo words match these constraints. Double-check your feedback.");
            continue;
        }
        if candidates.len() == 1 {
            println!("\nThe answer is: {}", candidates[0]);
            break;
        }

        println!("\nTop suggestions:");
        let ranked = rank_words(&candidates, &freq_data.commonality);
        display_suggestions(&ranked, 15);
    }
}
