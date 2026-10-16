//! Parser for the font descriptor scripts in `fonts/<size>/<name>.txt`.
//!
//! A descriptor is a list of statements ending in `;`, for example:
//!
//! ```text
//! SetDefaultPointSize 120;
//! SetCharMap ('\xa0') (' ');
//! CreateLayer MAIN;
//! Define MAINCharList (' ', '+', '0');
//! LayerSetAscent MAIN 124;
//! LayerSetCharWidths MAIN MAINCharList MAINWidthList;
//! LayerSetImage MAIN 'name_layer1';
//! ```
//!
//! Each font has several layers (outline, fill, shadow, bevel...). A layer
//! names a glyph sheet (stored as `<image>_.gif`, an alpha-only image that is
//! tinted when drawn) and, per character, a width, an offset and a rectangle
//! on that sheet.
//!
//! Encoding: most descriptors are Windows-1252/Latin-1 (they contain a raw
//! 0xA0 byte); a few start with a UTF-8 byte order mark. [`Font::parse_bytes`]
//! handles both.
//!
//! Not modelled: the exact meaning of `LayerRequireTags` / `LayerExcludeTags`
//! (the tag lists are stored as written) and of `DrawMode`.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Everything needed to draw one character from one layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub width: i32,
    pub offset: (i32, i32),
    /// Where the glyph sits on the layer's image.
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layer {
    pub name: String,
    pub ascent: i32,
    pub ascent_padding: i32,
    pub point_size: i32,
    pub alpha_correction: i32,
    pub offset: (i32, i32),
    pub height: i32,
    pub base_order: i32,
    pub spacing: i32,
    pub draw_mode: i32,
    /// Color multiplier (r, g, b, a) when the layer sets one.
    pub color_mult: Option<[f32; 4]>,
    /// Glyph sheet name, without folder or `_.gif`.
    pub image: String,
    pub chars: Vec<char>,
    pub widths: Vec<i32>,
    pub char_offsets: Vec<(i32, i32)>,
    pub image_map: Vec<Rect>,
    /// ((left, right), adjustment) kerning pairs.
    pub kerning: Vec<((char, char), i32)>,
    pub require_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
}

impl Layer {
    /// Look up one character. `None` when the layer has no entry for it.
    pub fn glyph(&self, c: char) -> Option<Glyph> {
        let i = self.chars.iter().position(|&x| x == c)?;
        Some(Glyph {
            width: *self.widths.get(i)?,
            offset: *self.char_offsets.get(i)?,
            rect: *self.image_map.get(i)?,
        })
    }

    /// Kerning adjustment for a pair of characters, 0 when there is none.
    pub fn kern(&self, left: char, right: char) -> i32 {
        self.kerning
            .iter()
            .find(|(p, _)| *p == (left, right))
            .map(|(_, v)| *v)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Font {
    pub default_point_size: i32,
    /// `SetCharMap`: (from, to) pairs, for example no-break space to space.
    pub char_map: Vec<(char, char)>,
    pub layers: Vec<Layer>,
    /// Statements this parser does not understand, by command name.
    pub ignored_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontError {
    pub message: String,
}

impl FontError {
    fn new(message: impl Into<String>) -> FontError {
        FontError {
            message: message.into(),
        }
    }
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "font descriptor error: {}", self.message)
    }
}

impl std::error::Error for FontError {}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Num(f64),
    Word(String),
    Punct(char),
}

fn tokenize(text: &str) -> Result<Vec<Tok>, FontError> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        match c {
            '(' | ')' | ',' | ';' => {
                out.push(Tok::Punct(c));
                i += 1;
            }
            '\'' | '"' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() {
                        return Err(FontError::new("unterminated string"));
                    }
                    let d = chars[i];
                    if d == quote {
                        i += 1;
                        break;
                    }
                    if d == '\\' {
                        i += 1;
                        if i >= chars.len() {
                            return Err(FontError::new("unterminated escape"));
                        }
                        match chars[i] {
                            'x' => {
                                if i + 2 >= chars.len() {
                                    return Err(FontError::new("short \\x escape"));
                                }
                                let hex: String = chars[i + 1..=i + 2].iter().collect();
                                let v = u32::from_str_radix(&hex, 16)
                                    .map_err(|_| FontError::new(format!("bad escape \\x{hex}")))?;
                                let ch = char::from_u32(v)
                                    .ok_or_else(|| FontError::new("bad escape value"))?;
                                s.push(ch);
                                i += 3;
                                continue;
                            }
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            other => s.push(other),
                        }
                        i += 1;
                        continue;
                    }
                    s.push(d);
                    i += 1;
                }
                out.push(Tok::Str(s));
            }
            '-' | '0'..='9' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n: f64 = text
                    .parse()
                    .map_err(|_| FontError::new(format!("bad number {text}")))?;
                out.push(Tok::Num(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Word(chars[start..i].iter().collect()));
            }
            other => {
                return Err(FontError::new(format!("unexpected character {other:?}")));
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Num(f64),
    Str(String),
    Word(String),
    List(Vec<Value>),
}

fn parse_value(toks: &[Tok], i: &mut usize) -> Result<Value, FontError> {
    match toks.get(*i) {
        Some(Tok::Num(n)) => {
            *i += 1;
            Ok(Value::Num(*n))
        }
        Some(Tok::Str(s)) => {
            *i += 1;
            Ok(Value::Str(s.clone()))
        }
        Some(Tok::Word(w)) => {
            *i += 1;
            Ok(Value::Word(w.clone()))
        }
        Some(Tok::Punct('(')) => {
            *i += 1;
            let mut items = Vec::new();
            loop {
                match toks.get(*i) {
                    Some(Tok::Punct(')')) => {
                        *i += 1;
                        break;
                    }
                    Some(Tok::Punct(',')) => {
                        *i += 1;
                    }
                    Some(_) => items.push(parse_value(toks, i)?),
                    None => return Err(FontError::new("unterminated list")),
                }
            }
            Ok(Value::List(items))
        }
        other => Err(FontError::new(format!("unexpected token {other:?}"))),
    }
}

fn arg(args: &[Value], n: usize) -> Result<&Value, FontError> {
    args.get(n)
        .ok_or_else(|| FontError::new(format!("missing argument {}", n + 1)))
}

fn resolve<'a>(v: &'a Value, defs: &'a HashMap<String, Value>) -> Result<&'a Value, FontError> {
    match v {
        Value::Word(w) => defs
            .get(w)
            .ok_or_else(|| FontError::new(format!("undefined name {w}"))),
        other => Ok(other),
    }
}

fn as_f64(v: &Value) -> Result<f64, FontError> {
    match v {
        Value::Num(n) => Ok(*n),
        other => Err(FontError::new(format!("expected a number, got {other:?}"))),
    }
}

fn as_int(v: &Value) -> Result<i32, FontError> {
    Ok(as_f64(v)?.round() as i32)
}

fn as_str(v: &Value) -> Result<&str, FontError> {
    match v {
        Value::Str(s) => Ok(s),
        other => Err(FontError::new(format!("expected a string, got {other:?}"))),
    }
}

fn as_word(v: &Value) -> Result<&str, FontError> {
    match v {
        Value::Word(w) => Ok(w),
        other => Err(FontError::new(format!("expected a name, got {other:?}"))),
    }
}

fn as_list(v: &Value) -> Result<&[Value], FontError> {
    match v {
        Value::List(l) => Ok(l),
        other => Err(FontError::new(format!("expected a list, got {other:?}"))),
    }
}

fn char_of(v: &Value) -> Result<char, FontError> {
    let s = as_str(v)?;
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(c),
        _ => Err(FontError::new(format!("expected one character, got {s:?}"))),
    }
}

fn pair(v: &Value) -> Result<(i32, i32), FontError> {
    let l = as_list(v)?;
    if l.len() != 2 {
        return Err(FontError::new("expected an (x, y) pair"));
    }
    Ok((as_int(&l[0])?, as_int(&l[1])?))
}

fn quad(v: &Value) -> Result<Rect, FontError> {
    let l = as_list(v)?;
    if l.len() != 4 {
        return Err(FontError::new("expected an (x, y, w, h) rectangle"));
    }
    Ok(Rect {
        x: as_int(&l[0])?,
        y: as_int(&l[1])?,
        w: as_int(&l[2])?,
        h: as_int(&l[3])?,
    })
}

impl Font {
    /// Parse raw file bytes. A UTF-8 byte order mark means UTF-8; otherwise the
    /// bytes are read as Latin-1 (each byte is one character).
    pub fn parse_bytes(bytes: &[u8]) -> Result<Font, FontError> {
        if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
            Font::parse(&String::from_utf8_lossy(rest))
        } else {
            let text: String = bytes.iter().map(|&b| b as char).collect();
            Font::parse(&text)
        }
    }

    pub fn parse(text: &str) -> Result<Font, FontError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let toks = tokenize(text)?;

        let mut font = Font::default();
        let mut defs: HashMap<String, Value> = HashMap::new();

        for stmt in toks.split(|t| *t == Tok::Punct(';')) {
            if stmt.is_empty() {
                continue;
            }
            let cmd = match &stmt[0] {
                Tok::Word(w) => w.as_str(),
                other => {
                    return Err(FontError::new(format!(
                        "statement starts with {other:?}, expected a command"
                    )))
                }
            };
            let mut args: Vec<Value> = Vec::new();
            let mut i = 1usize;
            while i < stmt.len() {
                args.push(parse_value(stmt, &mut i)?);
            }

            match cmd {
                "SetDefaultPointSize" => font.default_point_size = as_int(arg(&args, 0)?)?,
                "SetCharMap" => {
                    let from = as_list(resolve(arg(&args, 0)?, &defs)?)?;
                    let to = as_list(resolve(arg(&args, 1)?, &defs)?)?;
                    for (a, b) in from.iter().zip(to) {
                        font.char_map.push((char_of(a)?, char_of(b)?));
                    }
                }
                "CreateLayer" => {
                    let name = as_word(arg(&args, 0)?)?.to_string();
                    font.layers.push(Layer {
                        name,
                        ..Layer::default()
                    });
                }
                "Define" => {
                    let name = as_word(arg(&args, 0)?)?.to_string();
                    defs.insert(name, arg(&args, 1)?.clone());
                }
                "LayerRequireTags" | "LayerExcludeTags" => {
                    let lname = as_word(arg(&args, 0)?)?;
                    let tags: Vec<String> = as_list(arg(&args, 1)?)?
                        .iter()
                        .map(|v| as_str(v).map(str::to_string))
                        .collect::<Result<_, _>>()?;
                    let layer = font
                        .layers
                        .iter_mut()
                        .find(|l| l.name == lname)
                        .ok_or_else(|| FontError::new(format!("unknown layer {lname}")))?;
                    if cmd == "LayerRequireTags" {
                        layer.require_tags = tags;
                    } else {
                        layer.exclude_tags = tags;
                    }
                }
                c if c.starts_with("LayerSet") => {
                    let prop = &c["LayerSet".len()..];
                    let lname = as_word(arg(&args, 0)?)?;
                    let layer = font
                        .layers
                        .iter_mut()
                        .find(|l| l.name == lname)
                        .ok_or_else(|| FontError::new(format!("unknown layer {lname}")))?;
                    match prop {
                        "Ascent" => layer.ascent = as_int(arg(&args, 1)?)?,
                        "AscentPadding" => layer.ascent_padding = as_int(arg(&args, 1)?)?,
                        "PointSize" => layer.point_size = as_int(arg(&args, 1)?)?,
                        "AlphaCorrection" => layer.alpha_correction = as_int(arg(&args, 1)?)?,
                        "Height" => layer.height = as_int(arg(&args, 1)?)?,
                        "BaseOrder" => layer.base_order = as_int(arg(&args, 1)?)?,
                        "Spacing" => layer.spacing = as_int(arg(&args, 1)?)?,
                        "DrawMode" => layer.draw_mode = as_int(arg(&args, 1)?)?,
                        "Offset" => layer.offset = pair(arg(&args, 1)?)?,
                        "ColorMult" => {
                            let l = as_list(arg(&args, 1)?)?;
                            if l.len() != 4 {
                                return Err(FontError::new("ColorMult needs 4 numbers"));
                            }
                            let mut m = [0f32; 4];
                            for (k, v) in l.iter().enumerate() {
                                m[k] = as_f64(v)? as f32;
                            }
                            layer.color_mult = Some(m);
                        }
                        "Image" => layer.image = as_str(arg(&args, 1)?)?.to_string(),
                        "CharWidths" => {
                            let cl = as_list(resolve(arg(&args, 1)?, &defs)?)?;
                            let wl = as_list(resolve(arg(&args, 2)?, &defs)?)?;
                            layer.chars = cl.iter().map(char_of).collect::<Result<Vec<_>, _>>()?;
                            layer.widths = wl.iter().map(as_int).collect::<Result<Vec<_>, _>>()?;
                        }
                        "CharOffsets" => {
                            let l = as_list(resolve(arg(&args, 2)?, &defs)?)?;
                            layer.char_offsets = l.iter().map(pair).collect::<Result<Vec<_>, _>>()?;
                        }
                        "ImageMap" => {
                            let l = as_list(resolve(arg(&args, 2)?, &defs)?)?;
                            layer.image_map = l.iter().map(quad).collect::<Result<Vec<_>, _>>()?;
                        }
                        "KerningPairs" => {
                            let pl = as_list(resolve(arg(&args, 1)?, &defs)?)?;
                            let vl = as_list(resolve(arg(&args, 2)?, &defs)?)?;
                            layer.kerning.clear();
                            for (p, v) in pl.iter().zip(vl) {
                                if let Value::Str(s) = p {
                                    let mut it = s.chars();
                                    if let (Some(a), Some(b), None) = (it.next(), it.next(), it.next())
                                    {
                                        layer.kerning.push(((a, b), as_int(v)?));
                                    }
                                }
                            }
                        }
                        _ => font.ignored_commands.push(c.to_string()),
                    }
                }
                other => font.ignored_commands.push(other.to_string()),
            }
        }
        Ok(font)
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}SetDefaultPointSize 120;\r\n\
SetCharMap ('\\xa0') (' ');\r\n\
\r\n\
CreateLayer MAIN;\r\n\
Define MAINCharList\r\n (' ', '+', '0');\r\n\
Define MAINWidthList\r\n (42, 82, 102);\r\n\
Define MAINKernPairs\r\n (\"0+\", \"+0\", \"bad\");\r\n\
Define MAINKernValues\r\n (9, -4, 1);\r\n\
LayerSetAscent MAIN 124;\r\n\
LayerSetOffset MAIN (0,-5);\r\n\
LayerSetSpacing MAIN -3;\r\n\
LayerSetColorMult MAIN (0.0,0.0,0.0,1.0);\r\n\
LayerSetCharWidths MAIN MAINCharList MAINWidthList;\r\n\
LayerSetCharOffsets MAIN MAINCharList ((0,0), (-7,72), (10,47));\r\n\
LayerSetKerningPairs MAIN MAINKernPairs MAINKernValues;\r\n\
LayerSetImage MAIN 'x_layer1';\r\n\
LayerSetImageMap MAIN MAINCharList ((0,0,0,0), (168,255,96,99), (1,1,83,127));\r\n\
LayerRequireTags MAIN (\"SHADOW\");\r\n\
SomethingNew 1 2 3;\r\n";

    #[test]
    fn parses_a_whole_descriptor() {
        let f = Font::parse(SAMPLE).unwrap();
        assert_eq!(f.default_point_size, 120);
        assert_eq!(f.char_map, vec![('\u{a0}', ' ')]);
        assert_eq!(f.layers.len(), 1);

        let l = f.layer("MAIN").unwrap();
        assert_eq!(l.ascent, 124);
        assert_eq!(l.offset, (0, -5));
        assert_eq!(l.spacing, -3);
        assert_eq!(l.color_mult, Some([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(l.image, "x_layer1");
        assert_eq!(l.chars, vec![' ', '+', '0']);
        assert_eq!(l.widths, vec![42, 82, 102]);
        assert_eq!(l.require_tags, vec!["SHADOW".to_string()]);
        assert_eq!(f.ignored_commands, vec!["SomethingNew".to_string()]);
    }

    #[test]
    fn glyph_and_kerning_lookups() {
        let f = Font::parse(SAMPLE).unwrap();
        let l = f.layer("MAIN").unwrap();
        let g = l.glyph('+').unwrap();
        assert_eq!(g.width, 82);
        assert_eq!(g.offset, (-7, 72));
        assert_eq!(
            g.rect,
            Rect {
                x: 168,
                y: 255,
                w: 96,
                h: 99
            }
        );
        assert!(l.glyph('z').is_none());

        // The malformed three-character pair is skipped, the others are kept.
        assert_eq!(l.kerning.len(), 2);
        assert_eq!(l.kern('0', '+'), 9);
        assert_eq!(l.kern('+', '0'), -4);
        assert_eq!(l.kern('a', 'b'), 0);
    }

    #[test]
    fn latin1_bytes_and_utf8_bom_both_work() {
        // A raw 0xA0 byte inside a character literal, as in the real files.
        let latin1 = b"SetCharMap ('\xa0') (' ');";
        let f = Font::parse_bytes(latin1).unwrap();
        assert_eq!(f.char_map, vec![('\u{a0}', ' ')]);

        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice(b"SetDefaultPointSize 32;");
        assert_eq!(Font::parse_bytes(&with_bom).unwrap().default_point_size, 32);
    }

    #[test]
    fn quote_escapes_inside_strings() {
        let f = Font::parse("SetCharMap ('\\'') ('\"');").unwrap();
        assert_eq!(f.char_map, vec![('\'', '"')]);
        // A semicolon inside quotes must not end the statement.
        let g = Font::parse("SetCharMap (';') (',');").unwrap();
        assert_eq!(g.char_map, vec![(';', ',')]);
    }

    #[test]
    fn errors_are_reported_not_panics() {
        assert!(Font::parse("CreateLayer;").is_err());
        assert!(Font::parse("LayerSetAscent NOPE 1;").is_err());
        assert!(Font::parse("Define X (1, 2;").is_err());
        assert!(Font::parse("SetDefaultPointSize 'x';").is_err());
        assert!(Font::parse("SetCharMap ('ab') ('c');").is_err());
    }
}
