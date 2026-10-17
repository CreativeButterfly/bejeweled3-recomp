# Project notes

Start with `HANDOVER.md` (status, rules, next steps), then read this file for the details.

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

## Gem and UI art (from the manifest, 1200 set; metadata only, images not yet viewed)

- Gems are 2D sprite sheets per color: `IMAGE_GEMS_<COLOR>` is 640x512 with a 5x4 grid (20 frames
  of 128x128, probably a rotation or shine animation). Matching sets: `GemsNormal` (same
  layout, probably normal maps for lighting), `GemsShadow` (shadow sheets; two are odd sizes,
  Purple 640x536 and Orange 520x640 with a 4x5 grid, so always use the manifest `cols`/`rows`
  and never assume 128), `Gems_80px` (400x320, 5x4 of 80px), `gems_shadowed` (896x128, 7
  frames, one per color), `gemshard` (1200x30, 40 frames of 30px), `bombgems` (2261x324, 7
  columns, bomb-ring art per color; draw offset 26,29).
- Board position and cell size are NOT in `properties/defaultUIConstants.xml`. That file only
  holds dialog constants (options sliders, zen options, game-over dialog offsets, a few
  dialog rectangles) and mentions modes such as Lightning, Ice Storm and Poker. Take board
  layout from observation and put it in our own layout data file.

## 3D models (`.p3d`, verified on 10 of 12 files)

- Layout: magic `00 EF BE 3D`, u32 version (1 for gems and the tunnel, 2 for the board), then
  counted lists of shape names, materials (version 2 puts one extra byte before each material
  string) and key/value properties (`texture0.fileName`), then mesh blocks: u16 flags, u32
  vertex format (always 338), u16 vertex count, 36-byte vertices (position 3 f32, normal 3 f32,
  color 4 u8, uv 2 f32), u16 triangle count, then 3 u16 indices per triangle. Strings are a u16
  length plus Latin-1 bytes. Full layout in `geminator-res/src/p3d.rs`.
- Checked: every byte is accounted for, all indices are in range, normals are unit length. A
  flat-shaded render of the seven gem meshes (for my own review, not saved in the repo) shows
  the expected faceted gem shapes.
- Sizes: gems 30 to 180 vertices (4 to 116 triangles; the round white gem is the largest),
  board 455 vertices and 776 triangles, `warptube` 1,953 vertices and 3,720 triangles,
  `warptube_cap` 93 vertices.
- Textures: the property says `.png` or `.psd`, but the image is at
  `images/<size>/tex/<name without extension>` as `.jp2` (some with a `_.gif` mask); the tunnel
  uses `warp_lin.gif`, a plain GIF.
- `.p3danim` (partly understood, NOT decoded): all five hyperspace files are 917,364 bytes and
  89 to 99 percent byte-identical to each other, so they are variants with the same structure.
  They start with two u32 values (3, then 310; 310 may be the frame count) followed by a table
  of (id, count) pairs: ids run 1 to 65 (about 64 board gems plus a few extra objects, probably
  a camera) with counts of roughly 182 to 297 (likely keyframes per object). After the table
  the data turns into floats (a pair of zeros, then values such as 1.0). A simple "counts times a
  fixed key size equals the rest of the file" test did not fit, so the record layout is still
  unknown. Do not rely on this yet.
- Not parsed yet: `temple.p3d` and `questtemple.p3d` (a different header, probably several
  meshes). The `.p3danim` animation files are the next unknown (917,364 bytes each for the
  five hyperspace animations).
- Run the ignored test `models_parse_and_their_textures_exist` against your PAK.

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
- Level complete, in order: (1) the "Level Complete" text and voice appear over the full board
  with the bar full; (2) the gems smash through the board toward the camera (there are
  `hyperspace_shatter` sounds); (3) they fly through a vortex (a rainbow tunnel with large 3D
  gems tumbling in perspective); (4) they fly into place at random positions on the new board.
  Step 4 is presentation only: the core hands over the new board in `BoardReplaced` and the
  renderer picks random flight paths and arrival order. Every gem must end on its correct cell,
  and the board becomes playable once they have all landed.
- Assets for the vortex (names and sizes only): `warptube.p3d` and `warptube_cap.p3d` (tunnel
  model), `hyperspace.jpg` and `hyperspace_initial.jpg` per size set (tunnel texture), 3D gem
  models `gem<Color>.p3d`, five `hyperspace_*.p3danim` animation files of identical size
  (917,364 bytes each: main 0 to 3 and zen 0), `hyperspace.ogg`, `hyperspace_gem_land_*` sounds
  and a `firegem_hyperspace` particle effect. `.p3d` files start with a magic number and
  version and contain named records (shape and material names, texture file names such as
  `red_gem.png`); vertex data is not decoded yet.
- Level-up sounds (owner's description plus file lengths, not yet listened to): the smash has its
  own effect (`hyperspace_shatter_1` 1.8 s and `_2` 2.1 s, `_zen` 3.3 s for Zen); the vortex has
  `hyperspace.ogg` (6.7 s); and the landing is a cascade of one sound at different pitches:
  `hyperspace_gem_land_1` to `_7` (0.36 s down to 0.30 s, so probably rising pitch; confirm by
  ear), with longer Zen versions `hyperspace_gem_land_zen_1` to `_7` (about 1.3 to 1.6 s). Plan:
  play the next step of the 7-step ladder as each gem lands, wrapping around. The voice line is
  `voice_levelcomplete`. All are 44.1 kHz stereo Ogg Vorbis.
- Staging: 2D renderer first, then a placeholder level-up (text, gems scaling outward, a
  procedural swirl, the board panel breaking into shards), then decode `.p3d` and `.p3danim`
  and render the real tunnel and gems with our own shaders. The PAK has matching assets by name: `hyperspace_main_*` animations and
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

## Mode wheel (PS3 layout, from a screenshot of the mode-select screen)

- After PLAY, the modes sit on a ring of faceted crystal spheres, each with an icon inside. The
  selected mode is the largest and sits top center, with a glow behind its icon. In the
  screenshot, eight crystals: the selected one (Diamond Mine, pickaxe), a snowflake (Ice
  Storm), a butterfly (Butterflies), a card fan (Poker), a yin-yang (Zen), a lightning bolt
  (Lightning), a gold crown with laurel and "100%", and a blue triangle gem (the blue gem is
  Classic, as the owner confirmed; the crown is unconfirmed; the owner assumes the crown with "100%" is a completed quest mode, so a wheel
  entry can show a completion state, which means the profile must expose quest completion).
  Upper crystals are bigger and in front; the lower ones are smaller and partly
  hidden behind them, as on a tilted ring.
- Around it: the mode title at the top in a large outlined font, a one-line description under
  the ring, and button prompts in the bottom corners (select on the left, back on the right).
  We draw our own prompts for the active input device.
- The three quest types (Diamond Mine, Butterflies, Ice Storm) are separate entries on the
  wheel, so the wheel is a flat list of modes. Each entry is data: id, title, description,
  icon resource, tint. Ring positions come from the entry's offset from the selected one, so
  any number of modes works; with many, show the nearest few and scroll.
- Art found by name in the PAK: `crystalball` (+ mask), `crystalball glow` and `crystalball
  shadow` (the sphere), and `questmenu/icon_*` quest icons: alchemy, avalanche, balance,
  buried_treasure, butterfly, gold_rush, ice_storm, poker_copy, sandstorm, stratamax,
  time_bomb, wall_blast. Icons for Classic, Zen and Lightning were not found by that search and
  the pickaxe is unconfirmed. Not yet viewed.
- Confirmed by the owner: the blue gem is Classic, the yin-yang is Zen and the card fan is Poker.
  So the wheel holds Classic, Zen, Lightning, Poker and the quest types (Diamond Mine,
  Butterflies, Ice Storm), plus the crown entry.

## Bomb gems (for match bomb and time bomb; from the manifest, names and sizes only)

- Art already in the Steam PAK: `IMAGE_BOMBGEMS` (2261x324, 7 columns, one ring per gem color,
  draw offset 26,29), `IMAGE_BOMBGLOWS_GLOW` (1421x203, 7 columns), `IMAGE_BOMBGLOWS_DANGERGLOW`
  (1344x192, 7 columns, probably for low counts), `IMAGE_COUNTDOWNSHARDS` (1680x84, 20 frames
  of 84 px, probably the break-up when a countdown gem is destroyed), `IMAGE_ANGRYBOMB`
  (384x384) and a `COUNTDOWNBAR` particle effect. The time bomb quest has its own menu art
  (`icon_time_bomb`, `questmenu/stratamax` sheet with 3 rows).
- Sounds: `bomb_appears`, `bomb_explode`, `countdown_warning`, `gem_countdown_destroyed`,
  `timebombexplode`.
- Not bomb gems: `dangersnow_*` and `fbomb*` are Ice Storm effects (ice chunks, snowflakes).
- The number font: the community-modified PAK added a special bomb font, but the Steam PAK has
  none by that name. The numbers probably use an existing font such as `flaregothic55`;
  confirm by eye once a bomb gem is rendered. Nothing from the modified PAK is needed.
- Plan for the core: an optional countdown on a gem; match bomb counts down per move, time bomb
  per second; low counts use the danger glow and warning sound; reaching zero explodes the bomb
  and ends the game. Open (ask the owner to observe): what happens when a bomb gem is matched
  (defused and removed?), when new bombs appear, starting numbers (the screenshot showed 15, 13
  and 7).

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
  its unit tests (13) and both real-data tests pass on the owner's machine; the `.p3d` parser
  was added afterwards and has not been compiled yet. Run
  `cargo test` first and fix any compile errors before anything else.
- No renderer yet. Next: run the sprite test on a real PAK, then the Vulkan renderer that
  plays back `geminator-core` events, then audio (Ogg voice and sound effects, `.mo3` music).
