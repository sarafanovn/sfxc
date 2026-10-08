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
