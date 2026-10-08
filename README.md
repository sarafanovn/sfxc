<div align="center">

# sfxc

**Game sound effects in under a minute. Every sound stays editable forever.**

A small native synthesizer for game SFX, built in Rust. Retro 8-bit and 16-bit character, a modern effect chain, and a CLI so your AI agent can make sounds too.

[Features](#features) · [Getting started](#getting-started) · [Agent CLI](#agent-cli) · [Roadmap](#roadmap)

<img src="docs/screenshot.png" alt="sfxc main window" width="880">

</div>

## About

sfxc is in the spirit of sfxr and bfxr: click a category, get a coin, a laser or an explosion, tweak a few sliders, export. It adds the things those tools leave out.

- **Sounds are never baked.** Every sound is stored as synthesis parameters with a full version timeline. Open one months later, change the pitch, re-export. Restore any earlier version, or duplicate it as a new sound.
- **Made for you and your agent.** `sfxc-cli` works on the same library as the app. Let Claude Code or another terminal agent create, edit and export sounds straight into your game project, then open them in the GUI to listen and fine-tune. Every agent change is a version you can restore.
- **Authentic or modern, per sound.** Switch between clean modern synthesis, NES / Game Boy style 8-bit, and Mega Drive / SNES style 16-bit FM, then stack effects in any order.
- **Reproducible.** The same patch always renders the identical audio.

## Features

| | |
|---|---|
| **Styles** | Modern, 8-bit (pulse duties, 4-bit triangle, LFSR noise, NES period quantization, 22.05 kHz 8-bit output), 16-bit (4-operator FM, 32 kHz output) |
| **Generators** | Coin, Shoot, Explosion, Power-up, Hit, Jump, Blip, Random, and **Mutate** to nudge an existing sound |
| **Sources** | Pulse, saw, triangle, sine, white and LFSR noise, FM |
| **Shaping** | Slide, slide acceleration, vibrato, arpeggio, ADSR with punch, resonant filter with sweep |
| **Effects** | Bitcrusher, distortion, phaser, flanger, delay, reverb, compressor. Add, remove, drag to reorder, use the same one twice |
| **Library** | Local SQLite library with search, tags, autosaved drafts and per-sound history |
| **Export** | WAV (8/16/24-bit) and OGG at 22.05, 44.1 or 48 kHz, with peak normalization and silence trimming |

## Getting started

You need a recent stable Rust toolchain (edition 2024). Packaging and testing target macOS.

```sh
git clone <repo-url> sfxc
cd sfxc
cargo run --release -p sfxc-app
```

Build a macOS app bundle (unsigned, for local use):

```sh
./scripts/bundle.sh
open target/sfxc.app
```

The library lives in `~/Library/Application Support/sfxc/library.db`. Set `SFXC_LIBRARY=/path/to/library.db` to use another file.

## Using the app

1. Press **New sound** and pick a style.
2. Click a generator, or build from a source.
3. Adjust Pitch, Envelope and Filter. The sound plays when you release a slider. Double-click a slider to reset it.
4. Add effects and drag them into order.
5. Press `⌘S` to save a version, `⌘E` to export.

| Key | Action |
|---|---|
| `Space` | Play |
| `M` | Mutate |
| `⌘N` | New sound |
| `⌘S` | Save version |
| `⌘E` | Export |
| `⌘D` | Duplicate |
| `⌘Z` / `⇧⌘Z` | Undo / redo |

**Versions.** The working draft autosaves continuously and is not a version. A version is created on export, on `⌘S`, or after 5 minutes of unsaved changes. Restoring a version saves the current draft first, so nothing is lost. Undo and redo are per session and independent of versions.

## Agent CLI

`sfxc-cli` shares the library with the app, so a sound an agent creates appears in the GUI right away. It prints JSON, never plays audio and never deletes sounds.

```sh
cargo install --path crates/sfxc-cli

sfxc-cli schema                                    # every parameter, range and an example patch
sfxc-cli new --name coin --category PickupCoin     # generate a sound
sfxc-cli analyze coin                              # peak, rms, brightness, envelope (for agents that can't listen)
sfxc-cli set coin layers.0.pitch.base_freq=880 mode=Bit8
sfxc-cli fx add coin Delay
sfxc-cli export coin --to assets/sfx/coin.ogg      # remembers the path as a link
sfxc-cli export --linked                           # re-export every linked sound after you tweak them
```

Also available: `list`, `show`, `mutate`, `put`, `fx remove`, `rename`, `tag`, `versions`, `restore`. The full guide for agents is in [docs/cli.md](docs/cli.md).

## Project layout

```
crates/
├─ sfxc-core/    DSP and data model: patch, oscillators, effects, render, generators, export, analysis.
│                No UI, no audio device, no database.
├─ sfxc-store/   SQLite library: sounds, versions, export links. Shared by the app and the CLI.
├─ sfxc-app/     eframe/egui app: UI, cpal playback, background render worker, undo history.
└─ sfxc-cli/     Command line interface for agents.
scripts/bundle.sh   Builds target/sfxc.app
```

A parameter edit updates the patch, a background worker renders the whole sound offline (sounds are short, capped at 10 s, so this takes milliseconds), and the buffer goes to the audio output. The draft is saved to SQLite after a short delay.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

`sfxc-core` tests cover determinism (bit-identical output), finite output with peak at or below 1.0 for random patches, NES LFSR and period tables, effect stability, patch migration and clamping, and WAV/OGG round trips.

## Roadmap

- Use `sfxc-core` inside a game to generate sounds at runtime
- Multi-layer editing UI (the data model already supports layers)
- Export paths bound to a game folder from the GUI
- User presets and templates
- Keyboard / MIDI play
- Sample import
- MCP server wrapping the CLI
- Signed and notarized macOS builds

## Contributing

Issues and pull requests are welcome. Please run `cargo test --workspace` and `cargo clippy` first.

## License

[MIT](LICENSE). The bundled Geist fonts are under the SIL Open Font License, see [crates/sfxc-app/assets/fonts/OFL.txt](crates/sfxc-app/assets/fonts/OFL.txt).
