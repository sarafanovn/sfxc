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

1. `sfxc-cli schema` — categories, modes, sources, effects, every parameter range, an example patch and short working notes. Read it first.
2. Look before you create: `sfxc-cli project list` (is there a project for this game?) and `sfxc-cli list --project "My game" --search jump` (is there already a sound like this?). Reuse and edit instead of making a near-duplicate; names are unique.
3. Create: `sfxc-cli new --name "Player jump" --category jump [--mode 8bit] --project "My game" --path sfx/player/jump.ogg`. The project must exist; once per game: `sfxc-cli project new "My game" --root path/to/game/assets` (default root: the current directory).
4. Tag it right away (see [Tags](#tags-and-names)): `sfxc-cli tag "Player jump" "player movement short"`.
5. Check: `sfxc-cli analyze "Player jump"` — duration, peak, RMS, brightness (`spectral_centroid_hz`) and a 10-point envelope. You cannot listen, so judge by these numbers and compare with similar sounds.
6. Adjust: one `sfxc-cli set "Player jump" layers.0.pitch.base_freq=520 layers.0.env.release=0.12` with all changes at once (every call is one version), or `sfxc-cli mutate "Player jump"` for small random variations. Repeat 5–6.
7. Write files: `sfxc-cli project export "My game"` writes only the sounds changed since the last project export (including a person's unsaved edits in the app). Preview with `--dry-run`. After a person changes sounds in the app, nothing special is needed: `project export` sees their changes; `project export --all` rewrites everything.

A sound is an id or its exact name.

## Working efficiently

- **One project per game, one path per sound.** Paths are relative to the project's root, so `sfx/ui/click.ogg`, not an absolute path. Group by folder (`sfx/ui/`, `sfx/player/`, `sfx/enemies/`): the folder is what the game code loads. The output's `abs_path` shows where the file goes.
- **Create and add in one step** (`new --project --path`), so no sound is left in no project. `sfxc-cli list --unassigned` finds the stragglers; add them with `project add`.
- **Format.** `ogg` is small and fine for the game; `wav` for short UI blips or when the engine wants it. The extension of `--path` picks it; `project add`/`set` also take `--format` (changing it renames the extension).
- **Batch.** One `set` with many assignments, one `project export` at the end of a session, not after every tweak. `project export` is idempotent: running it again writes nothing.
- **Keep the person's work.** A person may have tuned a sound in the app. Before replacing a patch with `put`, read it with `show`; `versions` and `restore` undo anything. Prefer `set` for small changes. The CLI never deletes sounds, and sfxc never deletes files on disk.
- **Read, don't guess.** `show S` has the patch, tags, versions and `projects` with each `path` and `changed`; `project show P` lists the whole project with `changed` and `missing` flags. Both are small; call them instead of remembering.

## Tags and names

Tags make sounds findable for you and for the person (the app's search and the CLI's `--search` match the name and the tags; inside a project also the path). `sfxc-cli tag S "a b c"` **replaces** all tags, so read the current ones first (`show S` or `list`) and send the full set. Tags are space-separated lowercase words.

Use a small stable vocabulary, one word from each group that applies:

| Group | Examples |
|---|---|
| what it is (category) | `coin` `jump` `hit` `explosion` `laser` `powerup` `blip` `ambient` |
| who or where | `player` `enemy` `boss` `ui` `menu` `pickup` `door` |
| feel | `short` `long` `soft` `harsh` `bright` `dark` `retro` |
| status | `wip` while you iterate, `final` when it is ready; remove `wip` when you finish |

Before inventing a tag, run `sfxc-cli list` and reuse the words already there. Name sounds by what they do in the game (`Player jump`, `Coin pickup`), not by how they were made; the path carries the folder.

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
