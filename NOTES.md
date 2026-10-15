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
  in `extra/`, each listed in `extra/PROVENANCE.txt`. `bej3pak pack` refuses to build if a
  file has no entry. That check only verifies an entry exists, so entries must be honest.
- Do not use or ship files from community-modified or translated releases. They bundle the
  original assets with unlicensed additions. Do not include any ad-related content: the paid
  Steam and GOG versions are not ad-supported.
- Never commit `*.pak` or anything extracted from one. `.gitignore` covers this; add it
  before the first commit, because deleting a file later does not remove it from history.
- Name and README: unofficial fan project, no use of the game's title or logo as branding.
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
- Not yet decoded: sprite formats, `.pam`, `.ppf`, `.cfw2`, `.p3d`, config schemas.

## Rules implemented in bej3core (from the owner's description)

Line of 3: clear. Line of 4: flame gem (clears 3x3 when matched). Line of 5: hypercube
(swap with a gem to clear that color; two together clear the board). Two crossing groups of
3: star gem (clears row and column when matched). Straight line of 6+: supernova (clears 3
rows and 3 columns when matched). Priority: supernova, hypercube, star, flame. Specials
trigger when part of a match in their color; specials caught in a blast trigger too.
Specials are created on the swapped cell when it is part of the match, else at the
crossing (star) or the middle of the longest line.

## Open questions

- Hypercube caught in another special's blast: currently just cleared.
- Special + special swap combinations other than hypercube + hypercube: not defined.
- Scoring, cascade multipliers, level progress, hint delay, animation timing: need to be
  observed in the real game; put in data files, not hard-coded.
- Does the supernova need a straight line (current) or any six-gem shape?
- What the round counter in the bomb-mode HUD counts.
- Where the time bomb quest music lives (probably inside the `.mo3`).

## Status and caveats

- `bej3pak` and `bej3core` were written without a Rust compiler available. Run
  `cargo test` first and fix any compile errors before anything else.
- No renderer yet. Next: decode sprite formats, then the Vulkan renderer that plays back
  `bej3core` events.
