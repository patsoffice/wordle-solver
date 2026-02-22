use askama::Template;
use askama_web::WebTemplate;
use axum::{
    extract::State,
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::SystemTime,
};
use uuid::Uuid;
use wordle_word::*;

// ---------- App state ----------

struct WordData {
    available_words: Vec<String>,
    commonality: HashMap<String, f64>,
    loaded_at: SystemTime,
}

fn format_timestamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Convert to date/time components (UTC)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;

    // Days since 1970-01-01 to Y-M-D
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0;
    for md in &month_days {
        if remaining < *md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        y,
        m + 1,
        remaining + 1,
        hours,
        minutes
    )
}

struct AppState {
    word_data: RwLock<WordData>,
    sessions: RwLock<HashMap<String, Session>>,
}

struct Session {
    state: GameState,
    candidates: Vec<String>,
    guesses: Vec<(String, String)>,
}

impl Session {
    fn new(available_words: &[String]) -> Self {
        Self {
            state: GameState::new(),
            candidates: available_words.to_vec(),
            guesses: Vec::new(),
        }
    }
}

type SharedState = Arc<AppState>;

fn load_word_data() -> WordData {
    let all = match all_words() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{}", e);
            return WordData {
                available_words: Vec::new(),
                commonality: HashMap::new(),
                loaded_at: SystemTime::now(),
            };
        }
    };

    let used = used_words();
    let available: std::collections::HashSet<&String> = all.difference(&used).collect();

    let freq_data = load_frequency_data(&available);

    let mut available_words: Vec<String> = available.into_iter().cloned().collect();
    let plurals_removed = filter_regular_plurals(&mut available_words, &freq_data.dictionary);

    println!(
        "{} total words, {} past answers excluded, {} regular plurals filtered, {} candidates available.",
        all.len(),
        used.len(),
        plurals_removed,
        available_words.len()
    );

    WordData {
        available_words,
        commonality: freq_data.commonality,
        loaded_at: SystemTime::now(),
    }
}

// ---------- Template data structs ----------

struct TileData {
    letter: char,
    class: String,
}

struct SuggestionEntry {
    word: String,
    score: String,
}

fn build_grid_rows(guesses: &[(String, String)]) -> Vec<Vec<TileData>> {
    guesses
        .iter()
        .map(|(word, feedback)| {
            word.chars()
                .zip(feedback.chars())
                .map(|(letter, fb)| {
                    let class = match fb {
                        'g' => "green".to_string(),
                        'y' => "yellow".to_string(),
                        _ => "grey".to_string(),
                    };
                    TileData { letter, class }
                })
                .collect()
        })
        .collect()
}

fn build_suggestions(ranked: &[(String, f64)]) -> Vec<SuggestionEntry> {
    ranked
        .iter()
        .map(|(word, score)| SuggestionEntry {
            word: word.clone(),
            score: format!("{:.2}", score),
        })
        .collect()
}

// ---------- Templates ----------

#[derive(Template, WebTemplate)]
#[template(path = "game.html")]
struct GameTemplate {
    grid_rows: Vec<Vec<TileData>>,
    guess_count: usize,
    solved: bool,
    no_matches: bool,
    suggestions: Vec<SuggestionEntry>,
    candidate_count: usize,
    has_constraints: bool,
    has_green: bool,
    green_display: String,
    required_display: String,
    excluded_display: String,
    data_loaded_at: String,
    data_stale: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/results.html")]
struct ResultsTemplate {
    grid_rows: Vec<Vec<TileData>>,
    guess_count: usize,
    solved: bool,
    no_matches: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/suggestions.html")]
struct SuggestionsTemplate {
    suggestions: Vec<SuggestionEntry>,
    candidate_count: usize,
    has_constraints: bool,
    has_green: bool,
    green_display: String,
    required_display: String,
    excluded_display: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/reload_status.html")]
struct ReloadStatusTemplate {
    success: bool,
    message: String,
}

// ---------- Session helpers ----------

fn get_session_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie.strip_prefix("session=").map(|v| v.to_string())
        })
}

fn set_session_cookie(session_id: &str) -> (header::HeaderName, String) {
    (
        header::SET_COOKIE,
        format!("session={}; Path=/; SameSite=Lax", session_id),
    )
}

// ---------- Handlers ----------

async fn index(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let session_id = get_session_id(&headers).unwrap_or_default();

    let (
        session_id_out,
        guesses,
        candidates_len,
        green_disp,
        required_disp,
        excluded_disp,
        ranked,
        loaded_at,
        data_stale,
    ) = {
        let word_data = state.word_data.read().unwrap();
        let mut sessions = state.sessions.write().unwrap();

        let sid = if sessions.contains_key(&session_id) && !session_id.is_empty() {
            session_id
        } else {
            let new_id = Uuid::new_v4().to_string();
            sessions.insert(new_id.clone(), Session::new(&word_data.available_words));
            new_id
        };

        let session = sessions.get(&sid).unwrap();
        let ranked = rank_words_owned(&session.candidates, &word_data.commonality);
        let top: Vec<(String, f64)> = ranked.into_iter().take(15).collect();

        let stale = SystemTime::now()
            .duration_since(word_data.loaded_at)
            .unwrap_or_default()
            .as_secs()
            > 12 * 3600;

        (
            sid,
            session.guesses.clone(),
            session.candidates.len(),
            session.state.green_display(),
            session.state.required_display(),
            session.state.excluded_display(),
            top,
            format_timestamp(word_data.loaded_at),
            stale,
        )
    };

    let has_green = green_disp != "_____";
    let has_constraints = has_green || !required_disp.is_empty() || !excluded_disp.is_empty();

    let template = GameTemplate {
        grid_rows: build_grid_rows(&guesses),
        guess_count: guesses.len(),
        solved: false,
        no_matches: false,
        suggestions: build_suggestions(&ranked),
        candidate_count: candidates_len,
        has_constraints,
        has_green,
        green_display: green_disp,
        required_display: required_disp,
        excluded_display: excluded_disp,
        data_loaded_at: loaded_at,
        data_stale,
    };

    let mut response = template.into_response();
    let (name, value) = set_session_cookie(&session_id_out);
    response.headers_mut().insert(name, value.parse().unwrap());
    response
}

#[derive(Deserialize)]
struct GuessForm {
    guess: String,
    feedback: String,
}

async fn submit_guess(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Form(form): Form<GuessForm>,
) -> Response {
    let session_id = get_session_id(&headers).unwrap_or_default();
    let guess = form.guess.to_ascii_lowercase();
    let feedback = form.feedback.to_ascii_lowercase();

    let (guesses, solved, no_matches) = {
        let word_data = state.word_data.read().unwrap();
        let mut sessions = state.sessions.write().unwrap();
        let session = match sessions.get_mut(&session_id) {
            Some(s) => s,
            None => {
                sessions.insert(session_id.clone(), Session::new(&word_data.available_words));
                sessions.get_mut(&session_id).unwrap()
            }
        };

        session.state.update(&guess, &feedback);
        session.candidates.retain(|w| session.state.matches(w));
        session.guesses.push((guess, feedback.clone()));

        let solved = feedback == "ggggg";
        let no_matches = session.candidates.is_empty() && !solved;

        (session.guesses.clone(), solved, no_matches)
    };

    ResultsTemplate {
        grid_rows: build_grid_rows(&guesses),
        guess_count: guesses.len(),
        solved,
        no_matches,
    }
    .into_response()
}

async fn submit_suggestions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Form(_form): Form<GuessForm>,
) -> Response {
    let session_id = get_session_id(&headers).unwrap_or_default();

    let (ranked, candidates_len, green_disp, required_disp, excluded_disp) = {
        let word_data = state.word_data.read().unwrap();
        let sessions = state.sessions.read().unwrap();
        match sessions.get(&session_id) {
            Some(session) => {
                let ranked = rank_words_owned(&session.candidates, &word_data.commonality);
                let top: Vec<(String, f64)> = ranked.into_iter().take(15).collect();
                (
                    top,
                    session.candidates.len(),
                    session.state.green_display(),
                    session.state.required_display(),
                    session.state.excluded_display(),
                )
            }
            None => (Vec::new(), 0, String::new(), String::new(), String::new()),
        }
    };

    let has_green = green_disp != "_____" && !green_disp.is_empty();
    let has_constraints = has_green || !required_disp.is_empty() || !excluded_disp.is_empty();

    SuggestionsTemplate {
        suggestions: build_suggestions(&ranked),
        candidate_count: candidates_len,
        has_constraints,
        has_green,
        green_display: green_disp,
        required_display: required_disp,
        excluded_display: excluded_disp,
    }
    .into_response()
}

async fn reset_game(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let session_id = get_session_id(&headers).unwrap_or_default();

    {
        let word_data = state.word_data.read().unwrap();
        let mut sessions = state.sessions.write().unwrap();
        sessions.insert(session_id, Session::new(&word_data.available_words));
    }

    ResultsTemplate {
        grid_rows: Vec::new(),
        guess_count: 0,
        solved: false,
        no_matches: false,
    }
    .into_response()
}

async fn reset_suggestions(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let session_id = get_session_id(&headers).unwrap_or_default();

    let (ranked, candidates_len) = {
        let word_data = state.word_data.read().unwrap();
        let sessions = state.sessions.read().unwrap();
        match sessions.get(&session_id) {
            Some(session) => {
                let ranked = rank_words_owned(&session.candidates, &word_data.commonality);
                let top: Vec<(String, f64)> = ranked.into_iter().take(15).collect();
                (top, session.candidates.len())
            }
            None => (Vec::new(), 0),
        }
    };

    SuggestionsTemplate {
        suggestions: build_suggestions(&ranked),
        candidate_count: candidates_len,
        has_constraints: false,
        has_green: false,
        green_display: String::new(),
        required_display: String::new(),
        excluded_display: String::new(),
    }
    .into_response()
}

async fn reload_data(State(state): State<SharedState>) -> Response {
    println!("Reloading word data...");

    let new_data = match tokio::task::spawn_blocking(load_word_data).await {
        Ok(data) => data,
        Err(e) => {
            return ReloadStatusTemplate {
                success: false,
                message: format!("Reload failed: {}", e),
            }
            .into_response();
        }
    };

    if new_data.available_words.is_empty() {
        return ReloadStatusTemplate {
            success: false,
            message: "Reload failed: no words loaded.".to_string(),
        }
        .into_response();
    }

    let count = new_data.available_words.len();

    {
        let mut word_data = state.word_data.write().unwrap();
        *word_data = new_data;
    }
    {
        let mut sessions = state.sessions.write().unwrap();
        sessions.clear();
    }

    println!("Reload complete. {} candidates available.", count);

    ReloadStatusTemplate {
        success: true,
        message: format!("Reloaded. {} candidates available.", count),
    }
    .into_response()
}

// ---------- Main ----------

#[tokio::main]
async fn main() {
    println!("Wordle Solver - Loading word lists...");

    let word_data = tokio::task::spawn_blocking(load_word_data)
        .await
        .expect("Failed to load word lists");

    let state = Arc::new(AppState {
        word_data: RwLock::new(word_data),
        sessions: RwLock::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/guess", post(submit_guess))
        .route("/suggestions", post(submit_suggestions))
        .route("/reset", post(reset_game))
        .route("/reset-suggestions", post(reset_suggestions))
        .route("/reload", post(reload_data))
        .with_state(state);

    println!("Server running at http://localhost:3000");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
