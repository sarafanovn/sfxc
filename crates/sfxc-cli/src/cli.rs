use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use sfxc_core::generators::Category;
use sfxc_core::patch::Mode;

/// Make and edit sfxc sounds from the terminal. Sounds land in the app's library; the app shows changes within a second.
/// Start with `sfxc-cli schema`.
#[derive(Parser)]
#[command(name = "sfxc-cli", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Categories, modes, sources, effects, parameter ranges and an example patch.
    Schema,
    /// List sounds, newest first.
    List {
        #[arg(long, default_value = "")]
        search: String,
    },
    /// A sound's patch, versions and export link. SOUND is an id or an exact name.
    Show { sound: String },
    /// Create a sound from a generator category or from patch JSON.
    New {
        #[arg(long)]
        name: String,
        #[arg(long, value_enum, required_unless_present = "patch", conflicts_with = "patch")]
        category: Option<CategoryArg>,
        #[arg(long, value_enum, default_value = "modern")]
        mode: ModeArg,
        #[arg(long)]
        seed: Option<u64>,
        /// Patch JSON file, or `-` for stdin.
        #[arg(long)]
        patch: Option<String>,
    },
    /// Nudge every parameter a little, like the app's Mutate button.
    Mutate {
        sound: String,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Set values by path, e.g. `layers.0.pitch.base_freq=880 master_volume=0.7`.
    Set {
        sound: String,
        #[arg(required = true)]
        assignments: Vec<String>,
    },
    /// Replace the whole patch with JSON from a file or `-` for stdin.
    Put {
        sound: String,
        #[arg(long)]
        patch: String,
    },
    /// Add or remove effects.
    #[command(subcommand)]
    Fx(FxCommand),
    Rename { sound: String, name: String },
    /// Replace the tags (space separated).
    Tag { sound: String, tags: String },
    Versions { sound: String },
    /// Make an older version the current one; the current state is kept as a version first.
    Restore { sound: String, version: i64 },
    /// Render to a file and remember it; later `export SOUND` or `export --linked` rewrites the same file.
    Export(ExportArgs),
    /// Duration, level, brightness and envelope of the rendered sound.
    Analyze { sound: String },
    /// Install the latest sfxc-cli release to ~/.local/bin (a Homebrew install says to use `brew upgrade`).
    /// Prints current and latest versions.
    Update {
        /// Only report whether a newer release exists.
        #[arg(long)]
        check: bool,
    },
    /// Projects: a folder in a game and the sounds written into it. `project export` writes what changed.
    #[command(subcommand)]
    Project(ProjectCommand),
}

#[derive(Subcommand)]
pub enum FxCommand {
    /// Add an effect with default settings (master chain unless --layer). Prints its id and path.
    Add {
        sound: String,
        /// Bitcrusher, Distortion, Phaser, Flanger, Delay, Reverb or Compressor.
        kind: String,
        #[arg(long)]
        layer: Option<usize>,
        /// Position in the chain; default is the end.
        #[arg(long)]
        at: Option<usize>,
    },
    /// Remove the effect with this id.
    Remove { sound: String, id: u64 },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CategoryArg {
    Coin,
    Shoot,
    Explosion,
    Powerup,
    Hit,
    Jump,
    Blip,
    Random,
}

impl CategoryArg {
    pub fn category(self) -> Category {
        match self {
            CategoryArg::Coin => Category::PickupCoin,
            CategoryArg::Shoot => Category::LaserShoot,
            CategoryArg::Explosion => Category::Explosion,
            CategoryArg::Powerup => Category::PowerUp,
            CategoryArg::Hit => Category::HitHurt,
            CategoryArg::Jump => Category::Jump,
            CategoryArg::Blip => Category::BlipSelect,
            CategoryArg::Random => Category::Random,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ModeArg {
    Modern,
    #[value(name = "8bit")]
    Bit8,
    #[value(name = "16bit")]
    Bit16,
}

impl ModeArg {
    pub fn mode(self) -> Mode {
        match self {
            ModeArg::Modern => Mode::Modern,
            ModeArg::Bit8 => Mode::Bit8,
            ModeArg::Bit16 => Mode::Bit16,
        }
    }
}

#[derive(clap::Args)]
pub struct ExportArgs {
    /// Sound to export; omit with --linked.
    #[arg(required_unless_present = "linked", conflicts_with = "linked")]
    pub sound: Option<String>,
    /// Re-export every sound that has an export link.
    #[arg(long)]
    pub linked: bool,
    /// Target file; relative paths resolve against the current directory. Sets the sound's export link.
    #[arg(long)]
    pub to: Option<PathBuf>,
    /// Default: from the file extension.
    #[arg(long, value_enum, requires = "to")]
    pub format: Option<FormatArg>,
    /// WAV depth 8, 16 or 24. Default: from the sound's mode.
    #[arg(long, requires = "to")]
    pub bits: Option<u16>,
    /// OGG quality 0–10. Default: 6.
    #[arg(long, requires = "to")]
    pub quality: Option<f32>,
    /// Sample rate. Default: 44100.
    #[arg(long, requires = "to")]
    pub rate: Option<u32>,
    /// Exact length in seconds; cut with a fade or padded with silence.
    #[arg(long, requires = "to")]
    pub length: Option<f32>,
    #[arg(long, requires = "to")]
    pub no_normalize: bool,
    #[arg(long, requires = "to")]
    pub no_trim: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    Wav,
    Ogg,
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Create a project. --root is the game folder sounds go into; default: the current directory.
    New {
        name: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Projects with how many sounds are changed or missing on disk.
    List,
    /// A project's sounds with their paths and whether each is changed or missing. PROJECT is an id or a name.
    Show { project: String },
    Rename { project: String, name: String },
    /// Point the project at another folder. Files are not moved; every sound is exported again next time.
    SetRoot { project: String, dir: PathBuf },
    /// Delete the project. Its sounds and files stay.
    Delete { project: String },
    /// Add a sound. --path is relative to the project folder (an absolute path inside it also works);
    /// default: the sound's name in the folder of the sound added last.
    Add {
        project: String,
        sound: String,
        #[arg(long)]
        path: Option<String>,
        #[command(flatten)]
        options: MemberArgs,
    },
    /// Change a sound's path or export settings. Changing the format renames the extension.
    Set {
        project: String,
        sound: String,
        #[arg(long)]
        path: Option<String>,
        #[command(flatten)]
        options: MemberArgs,
    },
    /// Take a sound out of the project. The sound and its file stay.
    Remove { project: String, sound: String },
}

/// Export settings of a sound in a project. Unset flags keep the current value (`add`: the default).
#[derive(clap::Args, Default)]
pub struct MemberArgs {
    /// Default: from the path's extension, else WAV.
    #[arg(long, value_enum)]
    pub format: Option<FormatArg>,
    /// WAV depth 8, 16, 24, or `auto` to follow the sound's mode (default).
    #[arg(long, value_parser = parse_bits)]
    pub bits: Option<Bits>,
    /// OGG quality 0–10. Default: 6.
    #[arg(long)]
    pub quality: Option<f32>,
    /// Sample rate. Default: 44100.
    #[arg(long)]
    pub rate: Option<u32>,
    /// Exact length in seconds; cut with a fade or padded with silence.
    #[arg(long, conflicts_with = "auto_length")]
    pub length: Option<f32>,
    /// Back to the sound's natural length.
    #[arg(long)]
    pub auto_length: bool,
    #[arg(long, conflicts_with = "no_normalize")]
    pub normalize: bool,
    #[arg(long)]
    pub no_normalize: bool,
    #[arg(long, conflicts_with = "no_trim")]
    pub trim: bool,
    #[arg(long)]
    pub no_trim: bool,
}

impl MemberArgs {
    pub fn is_empty(&self) -> bool {
        self.format.is_none()
            && self.bits.is_none()
            && self.quality.is_none()
            && self.rate.is_none()
            && self.length.is_none()
            && !(self.auto_length || self.normalize || self.no_normalize || self.trim || self.no_trim)
    }
}

/// `--bits`: `None` follows the sound's mode.
#[derive(Clone, Copy, Debug)]
pub struct Bits(pub Option<u16>);

fn parse_bits(s: &str) -> Result<Bits, String> {
    match s {
        "auto" => Ok(Bits(None)),
        "8" | "16" | "24" => Ok(Bits(Some(s.parse().expect("matched digits")))),
        _ => Err("use 8, 16, 24 or auto".into()),
    }
}
