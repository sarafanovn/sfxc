# sfxc-cli

Make and edit sfxc sounds from a terminal. Sounds go into the same library as the sfxc app, so a person can open them there, listen and change them. The app shows CLI changes within a second.

## Install

```bash
brew install sarafanovn/tap/sfxc-cli
# or, from a checkout:
cargo install --path crates/sfxc-cli
```

`SFXC_LIBRARY=/path/to/library.db` points both the app and the CLI at another library.

Every command prints JSON. Errors go to stderr as one `error: …` line with exit code 1.

`sfxc-cli update` installs the latest release to `~/.local/bin` (a Homebrew install reports `"installed_with": "homebrew"` and is updated with `brew upgrade sfxc-cli`); `update --check` only prints `current`, `latest` and `update_available`. When stderr is a terminal, other commands may add one "is available" line to stderr (checked at most once a day; `SFXC_NO_UPDATE_CHECK=1` turns it off). Without a terminal there is no check and no extra output.

## Workflow

1. `sfxc-cli schema` — categories, modes, sources, effects, every parameter range, and an example patch. Read it first.
2. Create: `sfxc-cli new --name "Player jump" --category jump [--mode 8bit]`. Names are unique.
3. Check: `sfxc-cli analyze "Player jump"` — duration, peak, RMS, brightness (`spectral_centroid_hz`) and a 10-point envelope.
4. Adjust: `sfxc-cli set "Player jump" layers.0.pitch.base_freq=520 layers.0.env.release=0.12`, or `sfxc-cli mutate "Player jump"` for small random variations.
5. Put it into the game. Once per game: `sfxc-cli project new "My game" --root path/to/game/assets` (default root: the current directory). Then `sfxc-cli project add "My game" "Player jump" --path sfx/jump.wav` — the path is relative to the project's root, not to your current directory; the output's `abs_path` shows where the file goes. Or create and add in one step: `sfxc-cli new --name coin --category coin --project "My game" --path sfx/coin.ogg`.
6. Write files: `sfxc-cli project export "My game"` writes only the sounds changed since the last project export (including a person's unsaved edits in the app). After a person changes sounds in the app: `sfxc-cli project export --all`.

A sound is an id or its exact name.

## What the parameters do

| Parameter | Effect |
|---|---|
| `layers.N.source` | Waveform. `Pulse` (with `duty`) is the classic chiptune tone, `Triangle`/`Sine` are soft, `Saw` is buzzy, `Noise` is for explosions, hits and wind. |
| `pitch.base_freq` | Starting pitch in Hz. |
| `pitch.slide` | Pitch sweep in octaves per second: positive rises (power-ups), negative falls (lasers, falls). |
| `pitch.delta_slide` | Makes the sweep speed up or slow down. |
| `pitch.vibrato_depth` / `vibrato_rate` | Wobble in semitones / Hz. |
| `pitch.arp_steps` / `arp_speed` | Semitone steps played in turn (coin "ding-ding"). |
| `env.attack` / `decay` / `sustain_level` / `sustain_time` / `release` | Loudness over time, in seconds. Short attack + short release = snappy. |
| `env.punch` | Extra click at the start of the sustain. |
| `layers.N.eq.gains` | Six bands (60, 150, 400, 1000, 2400, 15000 Hz), ±12 dB. Cut the top for darker, boost the low bands for weight. |
| `master_volume` | Level of the whole sound. |
| `mode` | `Modern`, `Bit8` (NES-like) or `Bit16`. |

## Editing rules

- Paths use dots and list indices: `layers.0.pitch.base_freq`.
- Values are JSON; a bare word is a string (`mode=Bit8`).
- Out-of-range values are clamped; `set` prints the value that was kept.
- Switch a source by replacing the whole object: `set S 'layers.0.source={"type":"Noise","kind":"White"}'`. Defaults for each source are in `schema`.
- Effects: `sfxc-cli fx add S Reverb` prints the effect's id and path (`master_effects.0`); tune it with `set S master_effects.0.kind.mix=0.3`; remove with `sfxc-cli fx remove S ID`. `--layer N` adds to a layer's own chain.
- Whole patches: `sfxc-cli put S --patch file.json` (or `-` for stdin). Unknown fields are reported in `warnings`.

## Projects

- `project list` / `project show P` — sounds, paths, `changed` (will be written) and `missing` (unchanged, but the file is gone).
- `project add P S [--path REL]` and `project set P S [--path REL]` take export options: `--format wav|ogg`, `--bits 8|16|24|auto` (auto follows the sound's mode), `--quality 0-10`, `--rate HZ`, `--length SECS` / `--auto-length`, `--normalize` / `--no-normalize`, `--trim` / `--no-trim`. Changing the format renames the extension. `set` prints `previous_path` when the path changed; the old file stays — delete it yourself if it is not needed.
- `project export P|--all [--include-missing] [--create-root] [--dry-run]`. Without `--include-missing`, files gone from disk are listed in `skipped_missing`. A project folder that does not exist (a disk that is not connected?) is an error unless `--create-root`. Exit code 1 when any file failed.
- `project rename`, `project set-root P DIR` (files are not moved; everything is written again next time), `project remove P S`, `project delete P` (sounds and files stay).
- `list --project P` / `list --unassigned`; `list` and `show` include each sound's `projects`.
- `export S --to FILE` writes one file once and remembers nothing. Links made by older versions (`export --to`) became the project "Linked".

## History

Every change from the CLI is a version noted `cli: <command>` (unless nothing changed). The person sees these in the app's history and can restore any of them; `sfxc-cli versions S` and `sfxc-cli restore S VERSION` do the same from the terminal. The CLI never deletes sounds.
