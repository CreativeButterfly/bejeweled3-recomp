# Native match-3 engine (unofficial)

An unofficial, from-scratch engine for playing a well-known match-3 game natively on Linux,
without a compatibility layer. It is a fan project and is not affiliated with or endorsed by
the game's developer or publisher.

**You must own the game.** The engine loads graphics, sound and data from your own purchased
copy (Steam or GOG) at runtime. This repository contains no game code, art, sound or data,
and never will. Anything the paid game does not provide is either original work or generated
for this project and is listed, with its origin and license, in `extra/PROVENANCE.txt`.

## Layout

| Path | What it is |
|---|---|
| `bej3pak/` | Reader/writer for the game's PAK container, plus a small command-line tool |
| `bej3core/` | Game logic with no graphics, plus a terminal harness to play it as text |
| `extra/` | Original assets that go into `extra.pak`, each listed in `PROVENANCE.txt` |
| `NOTES.md` | Design decisions, file-format findings, rules, open questions |

## Try it

```
cargo test                          # all crates
cargo run -p bej3core               # play in the terminal (seed optional: -- 42)
cargo run -p bej3pak --release -- info /path/to/main.pak
cargo run -p bej3pak --release -- pack extra extra.pak
```

Status: early. The PAK reader and the game logic exist; sprite decoding and the Vulkan
renderer do not yet. See `NOTES.md` before contributing.
