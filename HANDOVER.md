# Handover: read this first

For the next contributor, human or Claude. A previous Claude session built this project with the
owner. That session's chat history and memory are **not** available to you, so everything you need
is in this repository. Read this file, then `NOTES.md` (design decisions and file-format findings).

## What Geminator is

An unofficial, from-scratch engine for playing a well-known PopCap match-3 game (Bejeweled 3)
natively on Linux, with no Proton or Wine. The player must own the game on Steam or GOG. The engine
loads graphics, sound and data from the player's own install at runtime. This repository contains
no game code, art, sound or data. MIT license, code only.

## Ground rules (do not break these)

1. **No game data in the repo, ever.** No `.pak` files, nothing extracted from one, no screenshots
   of the original game. `.gitignore` covers the usual patterns, but check `git status` before
   every commit. A file committed once stays in history even if deleted later.
2. **Clean-room.** Learn formats from the data files and behavior from playing the game. Do not
   disassemble or decompile the game's executable, and do not copy code from mods that hook into
   it (read about them for ideas only).
3. **Nothing from community-modified or translated releases** (their files bundle the original
   assets with unlicensed additions), and nothing ad-related (the paid versions have no ads).
4. **Never talk to the original game's servers** or Steam leaderboards, and never imitate the
   official client. Our leaderboard is our own, optional and off by default (see `NOTES.md`).
5. **Extra assets only through `extra/`**, each file listed in `extra/PROVENANCE.txt` with its
   origin and license. `geminator-pak pack` refuses to build otherwise. Original or generated only.
6. Public name and README: unofficial fan project, unaffiliated, no use of the game's title or
   logo as branding. None of this is legal advice.

## State at handover

| Crate | What it does | Status |
|---|---|---|
| `geminator-pak` | Reads and writes the PAK container; `info`, `list`, `cat`, `pack` commands | Compiled; 4 unit tests pass on the owner's machine |
| `geminator-core` | Game rules with no graphics: matches, flame/star/hypercube/supernova gems, cascades, move checking, level bar, praise events, level-up events, terminal harness | Compiled and ran on the owner's machine (a full `cargo test` run reached the doc-tests); 21 unit tests expected, counts not confirmed. Level bar speed and praise thresholds are **placeholders** |
| `geminator-img` | Loads sprites: JPEG 2000 color plus paired GIF alpha mask, alpha-only images, plain GIFs | Real-data test passes: 5,153 of 5,153 images load. PNG and JPG are not supported yet (106 images) |
| `geminator-res` | Parsers for `resources.xml` (not valid XML, with `!ref:` aliases), font descriptors, and `.p3d` 3D models | Manifest and font parsers: 13 unit tests and 2 real-data tests pass. **The `.p3d` module was added afterwards and has not been compiled or run** (expect 16 unit tests and 3 real-data tests) |
| `tools/gem_tools.py` | Standalone Python analysis of a PAK (the prototypes the Rust was checked against) | Verified against the owner's PAK |

Not built yet: the renderer, audio, menus and mode wheel, profile/records/badges, bomb gems,
scoring, the level-up transition, `.pam` animations, `.ppf` particles, `.p3danim`, the leaderboard
protocol and server.

## First 15 minutes

1. `cargo test` from the repo root. Fix any compile errors in `geminator-res/src/p3d.rs` first.
2. With a PAK available (keep it outside the repo or rely on `.gitignore`):
   `GEMINATOR_PAK=/path/to/main.pak cargo test --release -p geminator-res -- --ignored --nocapture`
   Expect "parsed 10 models; not parsed: ..." listing only `temple.p3d` and `questtemple.p3d`.
   Also run the same for `-p geminator-img` (expect "loaded 5153 of 5153 images, 0 failures").
3. `python3 tools/gem_tools.py /path/to/main.pak manifest` (also `fonts`, `models`, `info`) to see
   the data checks pass outside Rust.
4. Before the first push: `git add -n .` and confirm it lists only source files.

## How the previous session worked

- It could not compile Rust. Formats were worked out with Python prototypes against the owner's
  real PAK, then ported to Rust with synthetic unit tests plus an ignored real-data test, and the
  owner ran `cargo test` and pasted the output. Keep doing it that way if you cannot compile.
- Small steps. Ask for output after each one. Record every finding in `NOTES.md`, marking what is
  verified and what is a guess.
- If you need game data and do not have it, ask the owner for specific small files or run the
  tools locally. Do not ask for the executable.

## Roadmap

1. **Foundation:** tests green, first push. 
2. **Classic mode playable in a window:** renderer (plan: `wgpu` on its Vulkan backend with
   `winit`; check the current versions and APIs on docs.rs first), board and gems from the 2D
   sheets, HUD and fonts, swap and fall animations from core events, mouse input, level bar,
   praise popups and voices. Start tiny: open a window and draw one sprite from the PAK.
3. **Game shell:** main menu and mode wheel (modes as data files), options, local profile,
   records, badges, the level-up transition.
4. **Modes:** Zen, Lightning, Poker, quests (extendable), new match bomb and time bomb, sandbox,
   mods folder (data-only first, scripting later).
5. **Polish and distribution:** `extra.pak`, Steam and GOG install detection, gamepad, packaging.
6. **Optional online:** `geminator-proto` crate, replay export and import, then the scoreboard
   server (a separate optional build; the game's networking behind an `online` feature, off by
   default). Replay verification needs final scoring in the core.

## Next steps, in order

1. Make the tests green (above).
2. Renderer prototype. Gem art: `IMAGE_GEMS_<COLOR>` sheets are 5x4 grids of 128 px frames; always
   use `cols`/`rows` from the manifest (two shadow sheets break the 128 assumption). Resolve
   resources by ID and resolution with `Manifest::resolve`. Alpha-only images come back as white
   with an alpha; tint them at draw time.
3. Bomb gems in the core (art and sounds already exist in the PAK, see `NOTES.md`).
4. Scoring, level bar and praise numbers, from observation.
5. Mode wheel, profile and badges.
6. Audio: Ogg voice and effects; the `.mo3` music needs a tracker-module decoder.
7. `.p3danim` and the real level-up transition (partly understood, see `NOTES.md`).

## What only the owner can supply (observation tasks)

- Scoring examples (plain 3-match, 4-match, a cascade, special gems).
- How fast the level bar fills and what each praise tier needs.
- Badge screen screenshots with descriptions and tiers; where the game keeps profiles and records.
- How bomb gems behave: starting numbers, defusing, spawning.
- What the gold crown on the mode wheel means (assumed: a completed quest mode).

## Known pitfalls and unknowns

- `resources.xml` is not valid XML; strict parsers fail. `!ref:ID` paths are aliases; some IDs are
  defined only in shared groups (no `res`) for one resolution.
- A `.jp2` mask GIF's alpha is the palette's gray color, not the raw index. Color is straight alpha.
- Font descriptors are mostly Latin-1; three start with a UTF-8 byte order mark. Ignore `.cfw2`.
- `.p3d` texture names say `.png`/`.psd`; the real image is `images/<size>/tex/<name>.jp2` (or
  gif). `temple.p3d` and `questtemple.p3d` do not parse yet. The board and gems are real 3D models;
  the first renderer can use the 2D sprite sheets instead.
- `.p3danim` is not decoded. `.pam`, `.ppf`, `.d3dfx` (compiled shaders: write our own) untouched.
- The manifest has a `2DOnly` group: the original probably has a 2D fallback worth studying.
- `x`/`y` attributes on images are probably draw offsets, unconfirmed.

## Start prompt for a fresh Claude session

Paste this, then attach or point to the repo:

> I'm continuing an open-source project called Geminator (MIT): an unofficial native Linux engine
> for a well-known PopCap match-3 game, loading the player's own purchased game files at runtime.
> Please read HANDOVER.md and NOTES.md in the repo first. Follow the ground rules strictly: no game
> data, no disassembly of the executable, nothing from modified or translated releases, and never
> contact the original game's servers. You may not be able to compile Rust, so work in small steps,
> prototype formats in Python against my PAK first, port to Rust with unit tests, and ask me to
> paste `cargo test` output after each step. Record findings in NOTES.md and say clearly what is
> verified and what is a guess. Start with the first-15-minutes checklist in HANDOVER.md.
