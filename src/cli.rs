//! Command-line front door.
//!
//! Pokeductor is a TUI first, and the flags here are deliberately narrow: they
//! exist to get *into* the interface in the right state, not to reimplement it
//! on the command line. The two cache operations are the exception. They have
//! no keybinding because they are not something you do mid-session — they are
//! what you reach for from a shell when something looks wrong, and until now
//! they meant finding `$XDG_CACHE_HOME/pokeductor` by hand and guessing.
//! `--json` is the other: the one way for a script to read what the app knows,
//! which it cannot do through a terminal interface. `--completions` and the
//! hidden `--man` are for whoever installs it, so the shell and `man` know the
//! flags below without anyone writing them out a second time.
//!
//! Output here stays in English while the interface is translated. Clap writes
//! its own help and errors in English regardless, so translating the handful of
//! lines around them would make the surface less consistent, not more.

use std::io::Write;
use std::path::Path;

use clap::builder::PossibleValue;
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;

use crate::cache;
use crate::color::Choice;
use crate::i18n::Language;
use crate::json;
use crate::theme::Theme;

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

    /// How much colour the terminal can show
    #[arg(long, value_enum, value_name = "WHEN", default_value = "auto")]
    color: Choice,

    /// Draw the interface in this palette
    #[arg(long, value_enum, value_name = "PALETTE")]
    theme: Option<Theme>,

    /// Print NAME as JSON and exit, instead of opening the interface
    ///
    /// NAME must come down to one species: an exact name, a Pokedex number,
    /// or a fragment only one name contains. Anything else exits non-zero.
    #[arg(long, requires = "name", conflicts_with_all = ["lang", "color", "theme"])]
    json: bool,

    /// Delete the on-disk cache and exit
    #[arg(long, conflicts_with_all = ["name", "lang", "color", "theme", "cache_dir", "json"])]
    clear_cache: bool,

    /// Print the cache directory and exit
    #[arg(long, conflicts_with_all = ["name", "lang", "color", "theme", "json"])]
    cache_dir: bool,

    /// Print a completion script for SHELL and exit
    #[arg(long, value_enum, value_name = "SHELL", exclusive = true)]
    completions: Option<Shell>,

    /// Print the man page, as roff, and exit. For packagers, so it stays out
    /// of `--help`.
    #[arg(long, hide = true, exclusive = true)]
    man: bool,
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
    /// Palette to draw in, or `None` for whatever the previous session left.
    pub theme: Option<Theme>,
    /// Colour depth to force, or [`Choice::Auto`] to work it out from the
    /// environment. Unlike the other two this is never `None`: "detect it"
    /// is itself one of the answers rather than the absence of one.
    pub color: Choice,
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
    if let Some(shell) = cli.completions {
        print(&completions(shell))?;
        return Ok(Outcome::Handled);
    }

    if cli.man {
        print(&man_page()?)?;
        return Ok(Outcome::Handled);
    }

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

    // `requires = "name"` has already made sure there is one.
    if let (true, Some(name)) = (cli.json, &cli.name) {
        json::run(name).await?;
        return Ok(Outcome::Handled);
    }

    Ok(Outcome::Launch(Startup {
        language: cli.lang,
        species: cli.name,
        theme: cli.theme,
        color: cli.color,
    }))
}

/// The completion script for `shell`, generated from the flags as defined.
fn completions(shell: Shell) -> Vec<u8> {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut command, name, &mut script);
    script
}

/// The man page, generated from the same definition `--help` is.
fn man_page() -> std::io::Result<Vec<u8>> {
    let mut page = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut page)?;
    Ok(page)
}

/// Writes `bytes` to stdout, treating a reader that stopped early — `| head` —
/// as done rather than as a failure. `println!` panics there, and a script
/// that only wanted the first lines should not see a panic message for it.
pub fn print(bytes: &[u8]) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
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

/// The `--color` values. Spelled for the command line rather than after the
/// variants: `256` is what anyone reaching for this flag would type, and
/// `never` says what it does to the interface rather than which encoding it
/// stops using.
impl ValueEnum for Choice {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Choice::Auto,
            Choice::Truecolor,
            Choice::Ansi256,
            Choice::Never,
        ]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Choice::Auto => PossibleValue::new("auto").help("Detect from COLORTERM and TERM"),
            Choice::Truecolor => PossibleValue::new("truecolor").help("Force 24-bit colour"),
            Choice::Ansi256 => PossibleValue::new("256").help("Force the 256-colour palette"),
            Choice::Never => PossibleValue::new("never").help("No colour, and no sprites"),
        })
    }
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

/// Accepted `--theme` values come from [`Theme::ALL`] and the codes sessions
/// are already stored with, so a third palette becomes a valid flag value by
/// existing, exactly as a seventh language would.
impl ValueEnum for Theme {
    fn value_variants<'a>() -> &'a [Self] {
        &Theme::ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.code()).help(self.label()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_palette_can_be_named_on_the_command_line() {
        assert_eq!(parse(&["--theme", "dmg"]).unwrap().theme, Some(Theme::Dmg));
        assert_eq!(
            parse(&["--theme", "pico8"]).unwrap().theme,
            Some(Theme::Pico8)
        );
        // A palette this build does not have is a usage error, not a silent
        // fallback: the flag was typed on purpose.
        assert!(parse(&["--theme", "cga"]).is_err());
        // And without it, the stored palette is left to answer.
        assert_eq!(parse(&[]).unwrap().theme, None);
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
    fn colour_defaults_to_working_it_out_from_the_environment() {
        assert_eq!(parse(&[]).unwrap().color, Choice::Auto);
    }

    #[test]
    fn every_colour_depth_is_reachable_by_the_name_it_is_offered_under() {
        let expected = [
            ("auto", Choice::Auto),
            ("truecolor", Choice::Truecolor),
            ("256", Choice::Ansi256),
            ("never", Choice::Never),
        ];
        for (value, choice) in expected {
            assert_eq!(parse(&["--color", value]).unwrap().color, choice);
        }
        assert_eq!(
            expected.len(),
            Choice::value_variants().len(),
            "every variant should have a spelling the tests cover"
        );
    }

    #[test]
    fn a_colour_depth_we_cannot_render_is_rejected() {
        assert!(parse(&["--color", "16"]).is_err());
        assert!(parse(&["--color", "always"]).is_err());
    }

    #[test]
    fn the_cache_commands_refuse_arguments_they_would_ignore() {
        assert!(parse(&["--clear-cache", "--cache-dir"]).is_err());
        assert!(parse(&["--clear-cache", "gengar"]).is_err());
        assert!(parse(&["--cache-dir", "gengar"]).is_err());
        assert!(parse(&["--cache-dir", "--lang", "tr"]).is_err());
        assert!(parse(&["--clear-cache", "--color", "never"]).is_err());
    }

    #[test]
    fn json_needs_a_name_and_nothing_that_only_shapes_the_interface() {
        let cli = parse(&["--json", "gengar"]).expect("a name is all it needs");
        assert!(cli.json);
        assert_eq!(cli.name.as_deref(), Some("gengar"));
        assert!(parse(&["--json"]).is_err());
        assert!(parse(&["--json", "gengar", "--lang", "tr"]).is_err());
        assert!(parse(&["--json", "gengar", "--theme", "dmg"]).is_err());
        assert!(parse(&["--json", "gengar", "--color", "never"]).is_err());
        assert!(parse(&["--json", "gengar", "--cache-dir"]).is_err());
        assert!(parse(&["--json", "gengar", "--clear-cache"]).is_err());
    }

    /// Every flag a user can see, spelled as it is typed.
    fn visible_long_flags() -> Vec<String> {
        Cli::command()
            .get_arguments()
            .filter(|arg| !arg.is_hide_set())
            .filter_map(|arg| arg.get_long())
            .map(|long| format!("--{long}"))
            .collect()
    }

    #[test]
    fn every_completion_script_knows_every_visible_flag() {
        let flags = visible_long_flags();
        assert!(flags.contains(&"--json".to_string()), "sanity: {flags:?}");
        for shell in Shell::value_variants() {
            let script = String::from_utf8(completions(*shell)).unwrap();
            for flag in &flags {
                // Some shells list a flag by its bare name.
                let bare = flag.trim_start_matches('-');
                assert!(
                    script.contains(flag.as_str()) || script.contains(bare),
                    "{shell} completions are missing {flag}"
                );
            }
        }
    }

    #[test]
    fn completion_scripts_offer_the_values_a_flag_accepts() {
        let script = String::from_utf8(completions(Shell::Bash)).unwrap();
        for value in ["pico8", "dmg", "truecolor", "tr", "de"] {
            assert!(script.contains(value), "bash completions lack {value}");
        }
    }

    #[test]
    fn the_man_page_documents_every_visible_flag() {
        let page = String::from_utf8(man_page().unwrap()).unwrap();
        assert!(page.starts_with(".ie"), "roff, not plain text");
        for flag in visible_long_flags() {
            // roff escapes a hyphen that must not be typeset as a dash.
            let escaped = flag.replace('-', "\\-");
            assert!(page.contains(&escaped), "man page is missing {flag}");
        }
        // `--man` itself is for packagers and stays out of the page.
        assert!(!page.contains("\\-\\-man"));
    }

    #[test]
    fn the_generators_stand_alone() {
        assert!(parse(&["--completions", "fish"]).is_ok());
        assert!(parse(&["--man"]).is_ok());
        assert!(parse(&["--completions", "fish", "gengar"]).is_err());
        assert!(parse(&["--completions", "fish", "--man"]).is_err());
        assert!(parse(&["--man", "--lang", "tr"]).is_err());
        assert!(parse(&["--completions", "tcsh"]).is_err());
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
                assert_eq!(startup.color, Choice::Auto);
            }
            Outcome::Handled => panic!("nothing here is handled on the command line"),
        }
    }
}
