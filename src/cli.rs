//! Command-line front door.
//!
//! Pokeductor is a TUI first, and the flags here are deliberately narrow: they
//! exist to get *into* the interface in the right state, not to reimplement it
//! on the command line. The two cache operations are the exception. They have
//! no keybinding because they are not something you do mid-session — they are
//! what you reach for from a shell when something looks wrong, and until now
//! they meant finding `$XDG_CACHE_HOME/pokeductor` by hand and guessing.
//!
//! Output here stays in English while the interface is translated. Clap writes
//! its own help and errors in English regardless, so translating the handful of
//! lines around them would make the surface less consistent, not more.

use std::path::Path;

use clap::builder::PossibleValue;
use clap::{Parser, ValueEnum};

use crate::cache;
use crate::i18n::Language;

/// A terminal Pokedex and evolution analyzer.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Open directly on this species, e.g. `pokeductor gengar`
    ///
    /// Goes into the search box, so everything its syntax understands works
    /// here too: `pokeductor 25` opens Pikachu, `pokeductor type:ghost` opens
    /// the list already filtered.
    #[arg(value_name = "NAME")]
    name: Option<String>,

    /// Start in this UI language
    #[arg(long, value_enum, value_name = "LANG")]
    lang: Option<Language>,

    /// Delete the on-disk cache and exit
    #[arg(long, conflicts_with_all = ["name", "lang", "cache_dir"])]
    clear_cache: bool,

    /// Print the cache directory and exit
    #[arg(long, conflicts_with_all = ["name", "lang"])]
    cache_dir: bool,
}

/// The state the arguments ask the TUI to open in.
///
/// Both fields are overrides rather than values: `None` means "whatever the
/// previous session left behind", which is what keeps a bare `pokeductor` from
/// having to know anything about session restore.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Startup {
    pub language: Option<Language>,
    pub species: Option<String>,
}

/// What the arguments amounted to.
#[derive(Debug)]
pub enum Outcome {
    /// Done on the command line; the TUI never opens.
    Handled,
    /// Open the TUI in this state.
    Launch(Startup),
}

/// Parses the process arguments and carries out anything that does not need a
/// terminal. Exits the process on `--help`, `--version` or a bad argument,
/// which is clap's job and its exit codes.
pub async fn run() -> anyhow::Result<Outcome> {
    dispatch(Cli::parse()).await
}

/// The half of [`run`] that does not touch the process arguments, so the
/// behaviour is reachable from a test with a hand-built `Cli`.
async fn dispatch(cli: Cli) -> anyhow::Result<Outcome> {
    if cli.cache_dir {
        println!("{}", cache_dir()?.display());
        return Ok(Outcome::Handled);
    }

    if cli.clear_cache {
        let dir = cache_dir()?;
        if cache::clear(dir).await? {
            println!("Removed {}", dir.display());
        } else {
            println!("Nothing to remove: {} does not exist", dir.display());
        }
        return Ok(Outcome::Handled);
    }

    Ok(Outcome::Launch(Startup {
        language: cli.lang,
        species: cli.name,
    }))
}

/// The resolved cache directory, or an error explaining why there is none.
///
/// [`cache::dir`] answering `None` means no home directory could be worked
/// out. The app itself treats that as "caching is off" and says nothing, since
/// it can still do its job; here the directory *is* the subject of the command,
/// so silence would leave the user with no idea what happened.
fn cache_dir() -> anyhow::Result<&'static Path> {
    cache::dir().ok_or_else(|| {
        anyhow::anyhow!(
            "could not work out a cache directory: set XDG_CACHE_HOME or HOME \
             (LOCALAPPDATA on Windows)"
        )
    })
}

/// Accepted `--lang` values are derived from [`Language::ALL`] and the codes
/// the app already stores sessions with, so a seventh language becomes a valid
/// flag value by existing rather than by anyone remembering this file.
impl ValueEnum for Language {
    fn value_variants<'a>() -> &'a [Self] {
        &Language::ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.flavor_code()).help(self.label()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("pokeductor").chain(args.iter().copied()))
    }

    #[test]
    fn the_command_definition_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_bare_invocation_overrides_nothing() {
        let cli = parse(&[]).expect("no arguments is valid");
        assert_eq!(cli.name, None);
        assert_eq!(cli.lang, None);
    }

    #[test]
    fn a_positional_argument_is_the_species_to_open() {
        assert_eq!(parse(&["gengar"]).unwrap().name.as_deref(), Some("gengar"));
        // Search syntax reaches the same field rather than a second parser.
        assert_eq!(
            parse(&["type:ghost"]).unwrap().name.as_deref(),
            Some("type:ghost")
        );
    }

    #[test]
    fn every_ui_language_is_an_accepted_lang_value() {
        for language in Language::ALL {
            let cli = parse(&["--lang", language.flavor_code()])
                .unwrap_or_else(|_| panic!("--lang {} should parse", language.flavor_code()));
            assert_eq!(cli.lang, Some(language));
        }
    }

    #[test]
    fn a_language_we_do_not_ship_is_rejected_rather_than_guessed_at() {
        assert!(parse(&["--lang", "ja"]).is_err());
        assert!(parse(&["--lang", "English"]).is_err());
    }

    #[test]
    fn the_cache_commands_refuse_arguments_they_would_ignore() {
        assert!(parse(&["--clear-cache", "--cache-dir"]).is_err());
        assert!(parse(&["--clear-cache", "gengar"]).is_err());
        assert!(parse(&["--cache-dir", "gengar"]).is_err());
        assert!(parse(&["--cache-dir", "--lang", "tr"]).is_err());
    }

    #[test]
    fn an_unknown_flag_is_an_error_rather_than_a_species_name() {
        assert!(parse(&["--shiny"]).is_err());
    }

    #[tokio::test]
    async fn arguments_that_ask_for_nothing_special_launch_the_tui() {
        let outcome = dispatch(parse(&["gengar", "--lang", "tr"]).unwrap())
            .await
            .expect("launching needs no filesystem");
        match outcome {
            Outcome::Launch(startup) => {
                assert_eq!(startup.species.as_deref(), Some("gengar"));
                assert_eq!(startup.language, Some(Language::Turkish));
            }
            Outcome::Handled => panic!("nothing here is handled on the command line"),
        }
    }
}
