# Project notes (read this first)

Handoff notes for anyone, human or Claude, picking this project up. Nothing here comes from
the game's code. Format findings come from looking at the owner's own game files; rules come
from the owner's description and from playing the game.

## Goal

A native Linux way to play the game with no Proton or Wine. The player must own the game on
Steam or GOG. The repository ships no game data.

## Legal approach (keep to it)

- Clean-room spirit: learn **behavior** by playing and **formats** by inspecting data files.
  Do not disassemble or decompile the game's executable, and do not paste code from any
  mod that hooks into the executable. Mods are fine to read about for ideas, not for code.
- Assets come from the player's own install. Gaps are filled by original or generated assets
  in `extra/`, each listed in `extra/PROVENANCE.txt`. `geminator-pak pack` refuses to build if a
  file has no entry. That check only verifies an entry exists, so entries must be honest.
- Do not use or ship files from community-modified or translated releases. They bundle the
  original assets with unlicensed additions. Do not include any ad-related content: the paid
  Steam and GOG versions are not ad-supported.
- Never commit `*.pak` or anything extracted from one. `.gitignore` covers this; add it
  before the first commit, because deleting a file later does not remove it from history.
- Name: Geminator. License: MIT (code only; grants nothing in the original game's assets).
  README says: unofficial fan project, unaffiliated, no use of the game's title or logo as
  branding. Crate and folder names use the project name, not the game's title.
- None of this is legal advice.

## Decisions

- Language: Rust. Rendering: Vulkan (plan: `wgpu` forced to its Vulkan backend, or `ash`
  if more control is needed). Windowing/input: SDL2 or winit. Shaders: our own GLSL/WGSL.
- Game logic is separate from rendering and returns an event list, so replays and tests are
  deterministic (seeded RNG plus recorded moves).
- UI layout follows the console layout: main menu with a central PLAY button and the modes
  revolving around it, instead of a grid. Layouts live in data files using screen-fraction
  anchors so they work at any resolution. Mode screens are the same for all modes.
- Modes are data definitions (name, icon, HUD, rules, music). Quest mode is extendable via
  quest packs in the same schema as the game's own quest config. A sandbox mode exposes all
  rules as settings. Modders get a mods folder; start data-only, add scripting (Rhai or Lua)
  later if needed.
- Planned new modes: match bomb (bombs count down per move) and time bomb (bombs count down
  per second). Same HUD, and time bomb reuses the time bomb quest music. Rules to be
  described by observation, not taken from any mod's code.
- First milestone: classic mode working.

## PAK format (verified against the owner's Steam main.pak, 10,117 entries)

- Whole file XORed with 0xF7. After un-XOR: u32 magic 0xBAC04AC0, u32 version 0.
- Index records until a flag byte with bit 0x80: flag u8, name length u8, name (backslash
  paths), size u32, Windows FILETIME u64. File data follows, concatenated in index order.
  Sizes sum exactly to the remaining bytes.
- Contents (~89 MB): ~4,900 `.gif` and ~4,500 `.jp2` (sprites; probably color plus alpha
  mask pairs, unconfirmed), `.ogg` sound effects, one `.mo3` music module, `.pam` animations,
  `.ppf` particles, `.cfw2` font data, `.p3d`/`.p3danim` 3D models and animations (board and
  gems are 3D), `.d3dfx` compiled Shader Model 2.0 effects (bytecode, not source: write our
  own shaders), and config in `properties/` (`resources.xml` manifest, `quest.cfg`,
  `music.xml`, ...). Most image folders exist in three sizes: 600, 768, 1200.
- Zen audio/text (`affirmations`, `ambient`, `binaural`) sit loose beside the PAK on Steam.
- Not yet decoded: `.pam`, `.ppf`, `.cfw2`, `.p3d`, `.mo3`, quest/other config schemas.

## Sprite format (verified on samples)

- 4,557 `.jp2` files; 4,236 have a mask named `<name>_.gif` of identical size. The other
  321 are opaque (large backgrounds). 664 other `.gif` files are ordinary indexed images
  (font layers, some backgrounds).
- The mask's alpha is the gray **palette color**, not the raw index: in all 60 sampled masks
  the palette is pure gray and is not an identity ramp. Soft edges are present.
- Color is not premultiplied (treat as straight alpha). Confirm by eye once rendering.
- `geminator-img` implements this (including alpha-only images; png/jpg not yet) with `hayro-jpeg2000` (pure Rust) and `gif`. Pillow decoded all
  samples, but the Rust decoders are untested on this data: run the ignored test
  `every_jp2_decodes_and_its_mask_matches` against your PAK.

## Resource manifest (`properties/resources.xml`, verified by parsing it)

- It is **not valid XML**: some attribute values are unquoted (`x=159`) and some attributes
  have no value (`nobits nopal`). A strict XML parser fails at line 7. Write a small lenient
  tokenizer instead. A Python prototype parsed all 7,237 elements.
- Element types: `Image` (6,628), `Sound` (189), `Font` (128), `PIEffect` (118, particle
  files), `PopAnim` (64, animations), `File` (36), `RenderEffect` (14, shader effects), plus
  `Resources` / `Group` / `CompositeResources` grouping. `Resources` groups carry a `res`
  attribute (for example 1200), and the same resource ID is defined once per resolution set
  (2,153 image IDs appear more than once).
- Per-resource attributes: `id` (for example `IMAGE_BADGES_BIG_ANNIHILATOR`), `path` (file path
  without extension), optional `x`/`y` (probably a draw offset; unconfirmed), `cols`/`rows` (the
  image is a sprite sheet cut into a grid; 387 images), `origpath` (alias: 1,407 images reuse
  another image's file), `variant`, and format flags.
- Path resolution: **all 6,628 images resolve**. 5,803 are color images (`.jp2`, `.png` or
  `.jpg`), 68 are plain `.gif`, and 757 are **alpha-only** images: a `<path>_.gif` mask with no
  color file (glow and particle pieces, quest fanfare pieces, and the font glyph layers). They
  are meant to be tinted. `geminator-img` returns them as white with that alpha (confirm when
  rendering). My first check missed them because it did not try the `_.gif` name. They are not
  leftovers from another game and not loose files. All 189 sounds, 64 animations and 118
  particle effects resolve too.
- Aliases: 59 `Font` entries, all in the shared `Init_Common` and `LoadingThread_Common` groups
  (no `res`), have a path of the form `!ref:OTHER_ID`, meaning "whatever OTHER_ID is at the
  current resolution". One target (`FONT_FLAREGOTHIC28`) has its 1200 definition in a shared
  group, so lookups by resolution fall back to groups without one. `Manifest::resolve` follows
  aliases; `Manifest::find` does the group fallback.
- Plan: a loader that looks resources up by ID and resolution, like the original, so mods can
  add or override IDs.

## Fonts (verified: all 69 descriptors parse in a Python prototype)

- 22 fonts per size set, each with a text descriptor `fonts/<size>/<name>.txt`: a script of
  statements ending in `;`. Statements seen: `SetDefaultPointSize`, `SetCharMap`,
  `CreateLayer`, `Define <name> (list)`, `LayerSet<Property> <layer> ...` (Ascent,
  AscentPadding, PointSize, AlphaCorrection, Offset, Height, BaseOrder, Spacing, DrawMode,
  ColorMult, CharWidths, CharOffsets, KerningPairs, Image, ImageMap) and
  `LayerRequireTags` / `LayerExcludeTags <layer> ("TAG")`. Layers are named like OUTLINE,
  OUTLINE__MOD, MAIN, MAIN__MOD, DROPSHADOW, BEVEL.
- Encoding: most descriptors are Latin-1/Windows-1252 (a raw 0xA0 byte appears), three start
  with a UTF-8 byte order mark. Decode accordingly (`Font::parse_bytes` does).
- Glyph sheets are alpha-only images named `<image>_.gif` in the same folder, tinted at draw
  time. In every descriptor, list lengths agree and every layer image exists.
- The `<name>.txt.cfw2` files look like scrambled cached copies of the descriptors. Ignore them.
- Not understood yet: the exact meaning of the tag statements and `DrawMode`.
- `geminator-res` implements the manifest and font parsers. Run its ignored tests against your
  PAK: `manifest_parses_and_every_path_resolves` and
  `every_font_descriptor_parses_and_is_consistent`.

## Rules implemented in geminator-core (from the owner's description)

Line of 3: clear. Line of 4: flame gem (clears 3x3 when matched). Line of 5: hypercube
(swap with a gem to clear that color; two together clear the board). Two crossing groups of
3: star gem (clears row and column when matched). Straight line of 6+: supernova (clears 3
rows and 3 columns when matched). Priority: supernova, hypercube, star, flame. Specials
trigger when part of a match in their color; specials caught in a blast trigger too.
Specials are created on the swapped cell when it is part of the match, else at the
crossing (star) or the middle of the longest line.

## Classic mode flow (from the owner's description)

- Matches fill a progress bar that runs left to right under the board. When it fills, the
  level is complete.
- Level complete: the "Level Complete" voice plays, then the old board's gems smash through
  the board and fly through a vortex, then gems fly in at random and settle into place on the
  next board. The PAK has matching assets by name: `hyperspace_main_*` animations and
  `hyperspace_shatter_*` / `hyperspace_gem_land_*` sounds (names only, not yet inspected).
- No legal move left: "No more moves!" voice, then game over (classic uses this in place of
  "Time's up!").
- Praise: a text popup (for example "Good!") appears on the board and the matching voice line
  plays together with it. Triggered by cascades, similar to Candy Crush praises. The PAK has
  `voice_good`, `excellent`, `awesome`, `spectacular`, `extraordinary`, `unbelievable`.
- Other voice lines belong to other modes or the start of a game: `get ready` and `go` play
  before a game starts; `thirty seconds`, `time's up`, `blazing speed`, `challenge complete`
  are for timed and other modes.
- HUD (classic): score pill with a small level badge, "GEMS" and "CASCADES" counters, a HINT
  button and a MENU button on the left, the bar under the board.
- In geminator-core: `Progress`, `ProgressRules`, `Praise`, `PraiseRules`, and the events
  `Praise`, `LevelComplete`, `BoardReplaced`. **The bar speed and the praise thresholds are
  placeholders**, not measured. Assumptions: the bar is checked after the whole move settles,
  overflow is discarded, and the next level gets a fresh random board.

## Records, badges and leaderboard (planned)

- Local first: a profile file in the player's own data folder holds the player name, records
  and badge progress. No network involved.
- Badges: data-driven (name, the statistic it counts, four thresholds, art). Default
  thresholds are read from the player's own `properties/badges.txt` at runtime, not copied
  into this repo. What each badge counts is not in that file: take it from the real game's
  badge screen (descriptions and tiers) and write our own definitions.
- Leaderboard: our own, optional and off by default. Never talk to the original game's
  servers or Steam leaderboards, and never imitate the official client. Verification by
  replay: a submission is ruleset version + seed + move list, and the server re-simulates it
  with `geminator-core`. This needs the core to stay deterministic across platforms (integer
  scoring, no platform-dependent math) and boards separated by ruleset hash so modded rules
  do not mix with standard ones. Players are pseudonymous (random local ID, optional display
  name, no email). The engine takes a configurable server URL so forks can point elsewhere.
- Order: local records, then replay export/import as files, then a server only if wanted.
- Network captures (netstat, Wireshark) are fine for debugging our own client and server.
  Keep captures out of the repo (`.gitignore` covers pcap files).

## Open questions

- Hypercube caught in another special's blast: currently just cleared.
- Special + special swap combinations other than hypercube + hypercube: not defined.
- Scoring, cascade multipliers, level progress, hint delay, animation timing: need to be
  observed in the real game; put in data files, not hard-coded.
- Does the supernova need a straight line (current) or any six-gem shape?
- How fast the level bar fills (per gem, per match, per points?) and whether special gems or
  cascades fill it faster. Praise thresholds. What the level badge and counters show exactly.
- Whether a level-up keeps or replaces the board; whether bar overflow carries over.
- What the round counter in the bomb-mode HUD counts.
- Where the time bomb quest music lives (probably inside the `.mo3`).

## Status and caveats

- `geminator-pak`, `geminator-core` and `geminator-img` compiled and their tests passed on the
  owner's machine (sprite test: 5,153 of 5,153 images). `geminator-res` was written later and
  its unit tests pass (11); the manifest alias handling was added after the
  first real-data run and has not been run yet. Run
  `cargo test` first and fix any compile errors before anything else.
- No renderer yet. Next: run the sprite test on a real PAK, then the Vulkan renderer that
  plays back `geminator-core` events, then audio (Ogg voice and sound effects, `.mo3` music).
