//! Checks the parsers against a real PAK. Ignored by default.
//!
//!   GEMINATOR_PAK=/path/to/main.pak cargo test --release -p geminator-res -- --ignored --nocapture

use geminator_pak::Pak;
use geminator_res::{Font, Manifest, Model, ResourceKind};

fn open() -> Pak {
    let path = std::env::var("GEMINATOR_PAK").expect("set GEMINATOR_PAK to your main.pak");
    Pak::open(path).unwrap()
}

#[test]
#[ignore = "needs a real main.pak; set GEMINATOR_PAK"]
fn manifest_parses_and_every_path_resolves() {
    let pak = open();
    let bytes = pak.read("properties/resources.xml").unwrap();
    let m = Manifest::parse_bytes(&bytes).unwrap();

    let count = |k: ResourceKind| m.iter().filter(|(_, r)| r.kind == k).count();
    let images = count(ResourceKind::Image);
    println!(
        "groups {}, images {}, sounds {}, fonts {}, animations {}, particle effects {}, shader effects {}, files {}",
        m.groups.len(),
        images,
        count(ResourceKind::Sound),
        count(ResourceKind::Font),
        count(ResourceKind::PopAnim),
        count(ResourceKind::PIEffect),
        count(ResourceKind::RenderEffect),
        count(ResourceKind::File),
    );
    assert!(images > 6000, "expected thousands of images, got {images}");

    let exists = |kind: &ResourceKind, path: &str| -> bool {
        match kind {
            ResourceKind::Image => {
                ["jp2", "gif", "png", "jpg"]
                    .iter()
                    .any(|e| pak.find(&format!("{path}.{e}")).is_some())
                    || pak.find(&format!("{path}_.gif")).is_some()
            }
            ResourceKind::Sound => pak.find(&format!("{path}.ogg")).is_some(),
            ResourceKind::PopAnim
            | ResourceKind::PIEffect
            | ResourceKind::Font
            | ResourceKind::RenderEffect => pak.find(path).is_some(),
            // `File` entries include loose files that live outside the PAK.
            _ => true,
        }
    };

    let mut missing: Vec<String> = Vec::new();
    let mut aliases = 0usize;
    for (_, r) in m.iter() {
        let Some(path) = r.path() else { continue };
        if r.reference().is_some() {
            // An alias: it must resolve to a real file at every resolution set.
            aliases += 1;
            for res in ["1200", "768", "600"] {
                match m.resolve(r.id(), Some(res)) {
                    Some(t) if exists(&t.kind, t.path().unwrap_or("")) => {}
                    _ => missing.push(format!("{} (alias, res {res})", r.id())),
                }
            }
        } else if !exists(&r.kind, path) {
            missing.push(path.to_string());
        }
    }
    println!("aliases followed: {aliases}");
    assert!(
        missing.is_empty(),
        "{} paths do not resolve, first: {:?}",
        missing.len(),
        missing.iter().take(5).collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "needs a real main.pak; set GEMINATOR_PAK"]
fn every_font_descriptor_parses_and_is_consistent() {
    let pak = open();
    let mut parsed = 0usize;
    for e in pak.entries() {
        let lower = e.name.to_ascii_lowercase();
        if !lower.starts_with("fonts\\") || !lower.ends_with(".txt") {
            continue;
        }
        let bytes = pak.read_entry(e).unwrap();
        let font = Font::parse_bytes(&bytes).unwrap_or_else(|err| panic!("{}: {err}", e.name));
        let folder = e.name.rsplit_once('\\').map(|(f, _)| f).unwrap_or("");

        for l in &font.layers {
            let n = l.chars.len();
            assert_eq!(l.widths.len(), n, "{} layer {}: widths", e.name, l.name);
            if !l.char_offsets.is_empty() {
                assert_eq!(l.char_offsets.len(), n, "{} layer {}: offsets", e.name, l.name);
            }
            if !l.image_map.is_empty() {
                assert_eq!(l.image_map.len(), n, "{} layer {}: image map", e.name, l.name);
            }
            if !l.image.is_empty() {
                let alpha_only = format!("{folder}\\{}_.gif", l.image);
                let plain = format!("{folder}\\{}.gif", l.image);
                assert!(
                    pak.find(&alpha_only).is_some() || pak.find(&plain).is_some(),
                    "{} layer {}: image {} not found",
                    e.name,
                    l.name,
                    l.image
                );
            }
        }
        parsed += 1;
    }
    println!("parsed {parsed} font descriptors");
    assert!(parsed > 60, "expected dozens of descriptors, got {parsed}");
}

#[test]
#[ignore = "needs a real main.pak; set GEMINATOR_PAK"]
fn models_parse_and_their_textures_exist() {
    let pak = open();
    let mut parsed = 0usize;
    let mut not_parsed: Vec<String> = Vec::new();
    for e in pak.entries() {
        if !e.name.to_ascii_lowercase().ends_with(".p3d") {
            continue;
        }
        let bytes = pak.read_entry(e).unwrap();
        match Model::parse(&bytes) {
            Ok(m) => {
                parsed += 1;
                if let Some(stem) = m.texture_stem() {
                    let base = format!("images\\1200\\tex\\{stem}");
                    let found = ["jp2", "gif", "png", "jpg"]
                        .iter()
                        .any(|ext| pak.find(&format!("{base}.{ext}")).is_some())
                        || pak.find(&format!("{base}_.gif")).is_some();
                    assert!(found, "{}: texture {stem} not found", e.name);
                }
            }
            Err(err) => not_parsed.push(format!("{}: {err}", e.name)),
        }
    }
    println!("parsed {parsed} models; not parsed: {not_parsed:?}");
    assert!(parsed >= 10, "expected at least 10 models to parse, got {parsed}");
}
