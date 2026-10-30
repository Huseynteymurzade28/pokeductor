//! "Who's that Pokemon?": the rules of the quiz, without a terminal.
//!
//! The app decides which species to ask about and the renderer draws its
//! silhouette; what lives here is everything in between that can be stated as
//! a rule — what counts as the right name, and what a right answer, a wrong one
//! and giving up each do to the streak.

use crate::i18n;
use crate::models::{title_case, PokemonDetail};

/// The quiz as it stands: the question on the table, if any, and the streak
/// that outlives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quiz {
    pub round: Option<Round>,
    /// Right answers in a row. Kept between runs, like the best.
    pub streak: u32,
    pub best: u32,
}

/// One question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    /// Raw API name of the Pokemon in the silhouette.
    pub target: String,
    /// What the player has typed so far.
    pub guess: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Still waiting for the right name. `missed` is set by a wrong answer and
    /// cleared by the next keystroke, so the card can say "not quite" until
    /// the player starts on another try.
    Asking {
        missed: bool,
    },
    Right,
    GaveUp,
}

impl Round {
    pub fn is_open(&self) -> bool {
        matches!(self.outcome, Outcome::Asking { .. })
    }
}

impl Quiz {
    /// Puts a new Pokemon on the table, replacing whatever was there.
    pub fn ask(&mut self, target: String) {
        self.round = Some(Round {
            target,
            guess: String::new(),
            outcome: Outcome::Asking { missed: false },
        });
    }

    /// The round still waiting for an answer, if there is one.
    pub fn open_round(&self) -> Option<&Round> {
        self.round.as_ref().filter(|round| round.is_open())
    }

    fn open_round_mut(&mut self) -> Option<&mut Round> {
        self.round.as_mut().filter(|round| round.is_open())
    }

    pub fn type_char(&mut self, c: char) {
        if let Some(round) = self.open_round_mut() {
            round.guess.push(c);
            round.outcome = Outcome::Asking { missed: false };
        }
    }

    pub fn backspace(&mut self) {
        if let Some(round) = self.open_round_mut() {
            round.guess.pop();
            round.outcome = Outcome::Asking { missed: false };
        }
    }

    /// Checks the guess against `accepted`, the names that count for the
    /// target. A wrong answer costs nothing but the try: the streak breaks on
    /// giving up, not on a typo.
    pub fn submit(&mut self, accepted: &[String]) {
        let Some(round) = self.open_round_mut() else {
            return;
        };
        if round.guess.trim().is_empty() {
            return; // an Enter on nothing is not an answer
        }
        if is_right(&round.guess, accepted) {
            round.outcome = Outcome::Right;
            self.streak += 1;
            self.best = self.best.max(self.streak);
        } else {
            round.outcome = Outcome::Asking { missed: true };
        }
    }

    pub fn give_up(&mut self) {
        if let Some(round) = self.open_round_mut() {
            round.outcome = Outcome::GaveUp;
            self.streak = 0;
        }
    }
}

/// The names a guess is held to for `detail`: its own name and its species'
/// (so `Raichu` is right for Alolan Raichu, whose silhouette is a Raichu's),
/// each as the slug and as it reads, and the species' name in English and in
/// the interface language, where PokeAPI has one.
///
/// Only those two languages: a German name is a fair answer in a German
/// interface, but accepting every language at once would make a guess right
/// for reasons the player cannot see.
///
/// The two Nidoran also answer to plain `nidoran`. Their silhouettes do tell
/// them apart, but the sign is a character few keyboards have, and asking for
/// it marks a player wrong for typing rather than for not knowing.
pub fn accepted_names(detail: &PokemonDetail, code: &str) -> Vec<String> {
    let mut names = vec![
        detail.name.clone(),
        title_case(&detail.name),
        detail.species.clone(),
        title_case(&detail.species),
    ];
    names.extend(detail.names.get("en").cloned());
    names.extend(detail.names.get(code).cloned());
    if let Some(unsexed) = ["-f", "-m"]
        .iter()
        .find_map(|sign| detail.species.strip_suffix(sign))
    {
        names.push(unsexed.to_string());
    }
    names
}

/// Whether `guess` names one of `accepted`, reading both the way a person
/// would: case, accents, spaces and punctuation do not matter, so `mr mime`,
/// `Mr. Mime` and `mr-mime` are the same answer, as are `flabebe` and
/// `Flabébé`.
pub fn is_right(guess: &str, accepted: &[String]) -> bool {
    let guess = normalize(guess);
    !guess.is_empty() && accepted.iter().any(|name| normalize(name) == guess)
}

/// A name reduced to the letters and digits that tell it apart. The gender
/// signs are spelled out first, as the slugs spell them, since dropping them
/// would make the two Nidoran one answer.
fn normalize(name: &str) -> String {
    i18n::fold(name)
        .chars()
        .map(|c| match c {
            '♀' => 'f',
            '♂' => 'm',
            other => other,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_name_is_right_whatever_its_case_spacing_or_punctuation() {
        let accepted = names(&["mr-mime", "Mr. Mime"]);
        for guess in ["mr-mime", "Mr. Mime", "MR MIME", "mrmime", "  mr mime "] {
            assert!(is_right(guess, &accepted), "{guess}");
        }
        assert!(!is_right("mime jr", &accepted));
    }

    #[test]
    fn a_name_is_right_with_or_without_its_accents() {
        let accepted = names(&["flabebe", "Flabébé"]);
        assert!(is_right("Flabébé", &accepted));
        assert!(is_right("flabebe", &accepted));
    }

    #[test]
    fn a_nidoran_is_right_without_its_sign_and_wrong_with_the_other_one() {
        let mut detail = crate::app::loaded("nidoran-f");
        detail.names = HashMap::from([("en".to_string(), "Nidoran♀".to_string())]);
        let female = accepted_names(&detail, "en");
        assert!(is_right("Nidoran♀", &female));
        assert!(is_right("nidoran f", &female));
        assert!(is_right("nidoran", &female));
        assert!(!is_right("Nidoran♂", &female));
        assert!(!is_right("nidoran m", &female));
    }

    #[test]
    fn nothing_typed_is_never_right() {
        assert!(!is_right("", &names(&["mew"])));
        assert!(!is_right(" - ", &names(&["mew"])));
    }

    #[test]
    fn a_form_is_answered_by_its_species_and_the_interface_language() {
        let mut detail = crate::app::loaded("raichu-alola");
        detail.species = "raichu".to_string();
        detail.names = HashMap::from([
            ("en".to_string(), "Raichu".to_string()),
            ("fr".to_string(), "Raichu".to_string()),
            ("de".to_string(), "Raichu".to_string()),
        ]);
        let accepted = accepted_names(&detail, "en");
        assert!(is_right("Raichu", &accepted));
        assert!(is_right("raichu alola", &accepted));

        let mut bulbasaur = crate::app::loaded("bulbasaur");
        bulbasaur.names = HashMap::from([
            ("en".to_string(), "Bulbasaur".to_string()),
            ("de".to_string(), "Bisasam".to_string()),
            ("fr".to_string(), "Bulbizarre".to_string()),
        ]);
        assert!(is_right("Bisasam", &accepted_names(&bulbasaur, "de")));
        assert!(
            !is_right("Bulbizarre", &accepted_names(&bulbasaur, "de")),
            "only the interface language and English count"
        );
    }

    #[test]
    fn a_right_answer_extends_the_streak_and_raises_the_best() {
        let mut quiz = Quiz {
            streak: 2,
            best: 2,
            ..Quiz::default()
        };
        quiz.ask("mew".to_string());
        "Mew".chars().for_each(|c| quiz.type_char(c));
        quiz.submit(&names(&["mew"]));
        assert_eq!(quiz.round.as_ref().unwrap().outcome, Outcome::Right);
        assert_eq!((quiz.streak, quiz.best), (3, 3));
    }

    #[test]
    fn a_wrong_answer_costs_the_try_but_not_the_streak() {
        let mut quiz = Quiz {
            streak: 4,
            best: 9,
            ..Quiz::default()
        };
        quiz.ask("mew".to_string());
        "mewtwo".chars().for_each(|c| quiz.type_char(c));
        quiz.submit(&names(&["mew"]));
        assert_eq!(
            quiz.round.as_ref().unwrap().outcome,
            Outcome::Asking { missed: true }
        );
        assert_eq!(quiz.streak, 4);

        quiz.backspace();
        assert_eq!(
            quiz.round.as_ref().unwrap().outcome,
            Outcome::Asking { missed: false },
            "a new try clears the miss"
        );
    }

    #[test]
    fn giving_up_ends_the_streak_and_keeps_the_best() {
        let mut quiz = Quiz {
            streak: 4,
            best: 9,
            ..Quiz::default()
        };
        quiz.ask("mew".to_string());
        quiz.give_up();
        assert_eq!(quiz.round.as_ref().unwrap().outcome, Outcome::GaveUp);
        assert_eq!((quiz.streak, quiz.best), (0, 9));
    }

    #[test]
    fn an_answered_round_takes_no_more_input() {
        let mut quiz = Quiz::default();
        quiz.ask("mew".to_string());
        "mew".chars().for_each(|c| quiz.type_char(c));
        quiz.submit(&names(&["mew"]));
        quiz.type_char('x');
        quiz.give_up();
        let round = quiz.round.as_ref().unwrap();
        assert_eq!(round.guess, "mew");
        assert_eq!(round.outcome, Outcome::Right);
        assert_eq!(quiz.streak, 1, "giving up after the fact breaks nothing");
    }
}
