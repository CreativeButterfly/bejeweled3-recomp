//! Sprite loading from a PAK.
//!
//! Findings this is built on (from the owner's own game files):
//!
//! * A sprite is a JPEG 2000 image (`name.jp2`, RGB) plus an optional mask
//!   named `name_.gif`. The mask is a paletted GIF of the same size.
//! * The mask's alpha is the gray color its palette gives each pixel, NOT the
//!   raw palette index (the palettes are not identity ramps).
//! * The color image is treated as straight (non-premultiplied) alpha.
//! * `.jp2` files without a mask are opaque. Standalone `.gif` files are
//!   ordinary indexed-color images.
//!
//! Everything is read from the player's own PAK at runtime.

use geminator_pak::{Pak, PakError};
use std::fmt;
use std::io::Cursor;

/// RGBA8 pixels, straight alpha, row-major from the top-left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub enum ImageError {
    Pak(PakError),
    Jp2(String),
    Gif(String),
    SizeMismatch { color: (u32, u32), mask: (u32, u32) },
    Unsupported(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Pak(e) => write!(f, "{e}"),
            ImageError::Jp2(e) => write!(f, "JPEG 2000 decode failed: {e}"),
            ImageError::Gif(e) => write!(f, "GIF decode failed: {e}"),
            ImageError::SizeMismatch { color, mask } => {
                write!(f, "color image is {color:?} but its mask is {mask:?}")
            }
            ImageError::Unsupported(e) => write!(f, "unsupported image: {e}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<PakError> for ImageError {
    fn from(e: PakError) -> Self {
        ImageError::Pak(e)
    }
}

/// A decoded GIF frame: palette indices plus the palette (RGB triples).
struct Indexed {
    width: u32,
    height: u32,
    indices: Vec<u8>,
    palette: Vec<u8>,
    transparent: Option<u8>,
}

fn read_gif(bytes: &[u8]) -> Result<Indexed, ImageError> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut dec = opts
        .read_info(Cursor::new(bytes))
        .map_err(|e| ImageError::Gif(e.to_string()))?;

    // Grab what we need from the decoder before reading the frame borrows it.
    let global: Vec<u8> = dec.global_palette().map(|p| p.to_vec()).unwrap_or_default();
    let (sw, sh) = (dec.width() as u32, dec.height() as u32);

    let frame = dec
        .read_next_frame()
        .map_err(|e| ImageError::Gif(e.to_string()))?
        .ok_or_else(|| ImageError::Gif("no frames".to_string()))?;

    if frame.left != 0 || frame.top != 0 || frame.width as u32 != sw || frame.height as u32 != sh {
        return Err(ImageError::Unsupported(
            "GIF frame does not cover the whole image".to_string(),
        ));
    }
    let palette = match &frame.palette {
        Some(p) => p.clone(),
        None => global,
    };
    Ok(Indexed {
        width: sw,
        height: sh,
        indices: frame.buffer.to_vec(),
        palette,
        transparent: frame.transparent,
    })
}

/// Alpha values from a mask GIF: the palette's gray color per pixel.
fn gif_alpha(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ImageError> {
    let g = read_gif(bytes)?;
    let alpha = g
        .indices
        .iter()
        .map(|&i| g.palette.get(i as usize * 3).copied().unwrap_or(0))
        .collect();
    Ok((g.width, g.height, alpha))
}

/// A standalone indexed GIF as RGBA (transparent index becomes alpha 0).
pub fn decode_gif_rgba(bytes: &[u8]) -> Result<Sprite, ImageError> {
    let g = read_gif(bytes)?;
    let mut rgba = Vec::with_capacity(g.indices.len() * 4);
    for &i in &g.indices {
        let o = i as usize * 3;
        let rgb = g.palette.get(o..o + 3).unwrap_or(&[0, 0, 0]);
        rgba.extend_from_slice(rgb);
        rgba.push(if g.transparent == Some(i) { 0 } else { 255 });
    }
    Ok(Sprite {
        width: g.width,
        height: g.height,
        rgba,
    })
}

/// Decode a JPEG 2000 (JP2) image to RGBA8.
fn decode_jp2(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ImageError> {
    use hayro_jpeg2000::{DecodeSettings, DecoderContext, Image};

    let image = Image::new(bytes, &DecodeSettings::default())
        .map_err(|e| ImageError::Jp2(format!("{e:?}")))?;
    let (w, h) = (image.width() as u32, image.height() as u32);
    let mut ctx = DecoderContext::default();
    let decoded = image
        .decode(&mut ctx)
        .map_err(|e| ImageError::Jp2(format!("{e:?}")))?;
    let data: Vec<u8> = decoded.data_u8().to_vec();

    let pixels = (w as usize) * (h as usize);
    if pixels == 0 || data.len() % pixels != 0 {
        return Err(ImageError::Jp2(format!(
            "unexpected buffer size {} for {w}x{h}",
            data.len()
        )));
    }
    let channels = data.len() / pixels;
    let mut rgba = Vec::with_capacity(pixels * 4);
    for px in data.chunks_exact(channels) {
        match channels {
            1 => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
            2 => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            3 => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            4 => rgba.extend_from_slice(px),
            n => return Err(ImageError::Jp2(format!("{n} channels not supported"))),
        }
    }
    Ok((w, h, rgba))
}

/// Load a sprite by path inside the PAK. Accepts `images/.../name`,
/// `images/.../name.jp2`, or a standalone `.gif`. Slashes and case don't matter.
///
/// Three kinds of image exist (checked against the whole manifest):
/// * `name.jp2` (+ optional `name_.gif` mask): a color image.
/// * `name.gif`: a plain indexed image.
/// * only `name_.gif`, no color file: an alpha-only image (glow, particle and
///   font-layer sprites meant to be tinted). Returned as white with that alpha.
pub fn load_sprite(pak: &Pak, path: &str) -> Result<Sprite, ImageError> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".gif") {
        return decode_gif_rgba(&pak.read(path)?);
    }
    let stem = lower.strip_suffix(".jp2").unwrap_or(&lower);
    let color_name = format!("{stem}.jp2");
    let mask_name = format!("{stem}_.gif");

    if pak.find(&color_name).is_none() {
        if pak.find(&mask_name).is_some() {
            let (w, h, alpha) = gif_alpha(&pak.read(&mask_name)?)?;
            let mut rgba = Vec::with_capacity(alpha.len() * 4);
            for a in alpha {
                rgba.extend_from_slice(&[255, 255, 255, a]);
            }
            return Ok(Sprite {
                width: w,
                height: h,
                rgba,
            });
        }
        if pak.find(&format!("{stem}.gif")).is_some() {
            return decode_gif_rgba(&pak.read(&format!("{stem}.gif"))?);
        }
        if pak.find(&format!("{stem}.png")).is_some()
            || pak.find(&format!("{stem}.jpg")).is_some()
        {
            return Err(ImageError::Unsupported(
                "png and jpg images are not supported yet".to_string(),
            ));
        }
    }

    let (w, h, mut rgba) = decode_jp2(&pak.read(&color_name)?)?;
    if pak.find(&mask_name).is_some() {
        let (mw, mh, alpha) = gif_alpha(&pak.read(&mask_name)?)?;
        if (mw, mh) != (w, h) {
            return Err(ImageError::SizeMismatch {
                color: (w, h),
                mask: (mw, mh),
            });
        }
        for (px, a) in rgba.chunks_exact_mut(4).zip(alpha) {
            px[3] = a;
        }
    }
    Ok(Sprite {
        width: w,
        height: h,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_alpha_comes_from_the_palette_color_not_the_index() {
        // Index 1 is white, 2 is mid gray, 3 is dark gray. If the loader used
        // the raw index as alpha the result would be [1, 2, 3].
        let palette = [0u8, 0, 0, 255, 255, 255, 128, 128, 128, 64, 64, 64];
        let mut buf = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut buf, 3, 1, &palette).unwrap();
            let mut frame = gif::Frame::default();
            frame.width = 3;
            frame.height = 1;
            frame.buffer = std::borrow::Cow::Owned(vec![1u8, 2, 3]);
            enc.write_frame(&frame).unwrap();
        }
        let (w, h, alpha) = gif_alpha(&buf).unwrap();
        assert_eq!((w, h), (3, 1));
        assert_eq!(alpha, vec![255, 128, 64]);
    }

    /// Loads every color image and every alpha-only image in a real PAK and
    /// checks each mask has the same size. png/jpg files are skipped (not
    /// supported yet). Run with:
    ///   GEMINATOR_PAK=/path/to/main.pak cargo test --release -p geminator-img -- --ignored --nocapture
    #[test]
    #[ignore = "needs a real main.pak; set GEMINATOR_PAK"]
    fn every_image_in_the_pak_loads() {
        let path = std::env::var("GEMINATOR_PAK").expect("set GEMINATOR_PAK to your main.pak");
        let pak = Pak::open(path).unwrap();

        let mut stems = std::collections::BTreeSet::new();
        for e in pak.entries() {
            let n = e.name.to_ascii_lowercase();
            if let Some(stem) = n.strip_suffix(".jp2") {
                stems.insert(stem.to_string());
            } else if let Some(stem) = n.strip_suffix("_.gif") {
                stems.insert(stem.to_string());
            }
        }

        let mut ok = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for stem in &stems {
            match load_sprite(&pak, stem) {
                Ok(s) => {
                    assert_eq!(s.rgba.len(), s.width as usize * s.height as usize * 4);
                    ok += 1;
                }
                Err(err) => failures.push(format!("{stem}: {err}")),
            }
        }
        println!("loaded {ok} of {} images, {} failures", stems.len(), failures.len());
        assert!(
            failures.is_empty(),
            "first failures: {:?}",
            failures.iter().take(5).collect::<Vec<_>>()
        );
    }
}
