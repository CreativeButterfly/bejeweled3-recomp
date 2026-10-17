# Geminator

Geminator is an unofficial, from-scratch engine for playing a well-known match-3 game natively on Linux,
without a compatibility layer. It is a fan project and is not affiliated with or endorsed by
the game's developer or publisher.

**You must own the game.** The engine loads graphics, sound and data from your own purchased
copy (Steam or GOG) at runtime. This repository contains no game code, art, sound or data,
and never will. Anything the paid game does not provide is either original work or generated
for this project and is listed, with its origin and license, in `extra/PROVENANCE.txt`.

## Layout

| Path | What it is |
|---|---|
| `geminator-pak/` | Reader/writer for the game's PAK container, plus a small command-line tool |
| `geminator-core/` | Game logic with no graphics, plus a terminal harness to play it as text |
| `geminator-img/` | Loads sprites from a PAK (JPEG 2000 color plus a paired GIF alpha mask) |
| `geminator-res/` | Parsers for the resource manifest (`resources.xml`), font descriptors and `.p3d` 3D models |
| `extra/` | Original assets that go into `extra.pak`, each listed in `PROVENANCE.txt` |
| `NOTES.md` | Design decisions, file-format findings, rules, open questions |

## Try it

```
cargo test                          # all crates
GEMINATOR_PAK=/path/to/main.pak cargo test --release -p geminator-res -- --ignored --nocapture
GEMINATOR_PAK=/path/to/main.pak cargo test -p geminator-img -- --ignored --nocapture   # decode every sprite in your PAK
cargo run -p geminator-core               # play in the terminal (seed optional: -- 42)
cargo run -p geminator-pak --release -- info /path/to/main.pak
cargo run -p geminator-pak --release -- pack extra extra.pak
```

Status: early. The PAK reader, the game logic and the sprite loader exist; the Vulkan
renderer does not yet. See `NOTES.md` before contributing.

## License

MIT, see `LICENSE`. It covers the code in this repository only. It grants no rights to the
original game's code, art, sound or data, which stay with their owners.
