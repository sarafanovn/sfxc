# sfxc-cli

Make and edit sfxc sounds from a terminal. Sounds go into the same library as the sfxc app, so a person can open them there, listen and change them. The app shows CLI changes within a second.

## Install

```bash
cargo install --path crates/sfxc-cli
```

`SFXC_LIBRARY=/path/to/library.db` points both the app and the CLI at another library.

Every command prints JSON. Errors go to stderr as one `error: …` line with exit code 1.

`sfxc-cli update` installs the latest release to `~/.local/bin`; `update --check` only prints `current`, `latest` and `update_available`. When stderr is a terminal, other commands may add one "is available" line to stderr (checked at most once a day; `SFXC_NO_UPDATE_CHECK=1` turns it off). Without a terminal there is no check and no extra output.

## Workflow

1. `sfxc-cli schema` — categories, modes, sources, effects, every parameter range, and an example patch. Read it first.
2. Create: `sfxc-cli new --name "Player jump" --category jump [--mode 8bit]`. Names are unique.
3. Check: `sfxc-cli analyze "Player jump"` — duration, peak, RMS, brightness (`spectral_centroid_hz`) and a 10-point envelope.
4. Adjust: `sfxc-cli set "Player jump" layers.0.pitch.base_freq=520 layers.0.env.release=0.12`, or `sfxc-cli mutate "Player jump"` for small random variations.
5. Export into the project: `sfxc-cli export "Player jump" --to assets/sfx/jump.wav`. The path is remembered.
6. After a person changes sounds in the app: `sfxc-cli export --linked` rewrites every remembered file.

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

## History

Every change from the CLI is a version noted `cli: <command>` (unless nothing changed). The person sees these in the app's history and can restore any of them; `sfxc-cli versions S` and `sfxc-cli restore S VERSION` do the same from the terminal. The CLI never deletes sounds.
