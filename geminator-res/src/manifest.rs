//! Parser for `properties/resources.xml`.
//!
//! The file looks like XML but is not well-formed: some attribute values are
//! unquoted (`x=159`) and some attributes have no value at all (`nobits`), so
//! strict XML parsers fail. This parser accepts both.
//!
//! Structure (observed): a `ResourceManifest` containing `Resources` groups
//! (one per resolution set, such as `1200_Main` with `res=1200`) and
//! `CompositeResources` groups. Inside a group are the resource elements:
//! `Image`, `Sound`, `Font`, `PIEffect`, `PopAnim`, `RenderEffect`, `File`.
//! The same resource ID is usually defined once per resolution group.
//!
//! Aliases: some entries (in the shared `Init_Common` and `LoadingThread_Common`
//! groups, which have no `res`) have a path of the form `!ref:OTHER_ID`. That
//! means "use whatever `OTHER_ID` is at the current resolution". Use
//! [`Manifest::resolve`] to follow them. A resource ID may also be defined only
//! in a shared group for one resolution, so lookups with a resolution fall back
//! to groups that have none.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    Image,
    Sound,
    Font,
    PIEffect,
    PopAnim,
    RenderEffect,
    File,
    Other(String),
}

impl ResourceKind {
    fn from_tag(tag: &str) -> ResourceKind {
        match tag {
            "Image" => ResourceKind::Image,
            "Sound" => ResourceKind::Sound,
            "Font" => ResourceKind::Font,
            "PIEffect" => ResourceKind::PIEffect,
            "PopAnim" => ResourceKind::PopAnim,
            "RenderEffect" => ResourceKind::RenderEffect,
            "File" => ResourceKind::File,
            other => ResourceKind::Other(other.to_string()),
        }
    }
}

/// One resource element. Attributes without a value (flags such as `nobits`)
/// are stored with an empty string.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub kind: ResourceKind,
    pub attrs: BTreeMap<String, String>,
}

impl Resource {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    /// The resource ID, or an empty string when there is none (`File` entries).
    pub fn id(&self) -> &str {
        self.attr("id").unwrap_or("")
    }

    /// File path inside the PAK. Images and sounds are stored without their
    /// extension.
    pub fn path(&self) -> Option<&str> {
        self.attr("path")
    }

    pub fn has_flag(&self, name: &str) -> bool {
        self.attrs.contains_key(name)
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        self.attr(name)?.trim().parse().ok()
    }

    /// `(cols, rows)` when the image is a sprite sheet cut into a grid.
    pub fn grid(&self) -> Option<(u32, u32)> {
        let cols = self.attr("cols").and_then(|v| v.trim().parse::<u32>().ok());
        let rows = self.attr("rows").and_then(|v| v.trim().parse::<u32>().ok());
        if cols.is_none() && rows.is_none() {
            return None;
        }
        Some((cols.unwrap_or(1), rows.unwrap_or(1)))
    }

    /// The `x` and `y` attributes. Probably a draw offset (unconfirmed).
    pub fn offset(&self) -> Option<(i32, i32)> {
        Some((self.int("x")?, self.int("y")?))
    }

    /// `origpath`: this entry reuses another image's file.
    pub fn alias_of(&self) -> Option<&str> {
        self.attr("origpath")
    }

    /// For `!ref:OTHER_ID` paths, the ID this entry points to.
    pub fn reference(&self) -> Option<&str> {
        self.path()?.strip_prefix("!ref:")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// `Resources` or `CompositeResources`.
    pub tag: String,
    pub attrs: BTreeMap<String, String>,
    pub resources: Vec<Resource>,
}

impl Group {
    pub fn id(&self) -> &str {
        self.attrs.get("id").map(String::as_str).unwrap_or("")
    }

    /// The resolution set, such as `"1200"`. `None` for groups without one.
    pub fn res(&self) -> Option<&str> {
        self.attrs.get("res").map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Manifest {
    pub version: Option<String>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "manifest parse error on line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

fn line_of(text: &str, at: usize) -> usize {
    text.as_bytes()[..at.min(text.len())]
        .iter()
        .filter(|&&c| c == b'\n')
        .count()
        + 1
}

impl Manifest {
    /// Parse manifest text from raw file bytes (UTF-8, lossy).
    pub fn parse_bytes(bytes: &[u8]) -> Result<Manifest, ParseError> {
        Manifest::parse(&String::from_utf8_lossy(bytes))
    }

    pub fn parse(text: &str) -> Result<Manifest, ParseError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let b = text.as_bytes();
        let err = |at: usize, msg: &str| ParseError {
            line: line_of(text, at),
            message: msg.to_string(),
        };

        let mut manifest = Manifest::default();
        // (tag, is_group) for every element that has been opened and not closed.
        let mut stack: Vec<(String, bool)> = Vec::new();
        let mut current: Option<usize> = None;
        let mut i = 0usize;

        while i < b.len() {
            if b[i] != b'<' {
                i += 1;
                continue;
            }
            if text[i..].starts_with("<!--") {
                match text[i + 4..].find("-->") {
                    Some(e) => {
                        i += 4 + e + 3;
                        continue;
                    }
                    None => return Err(err(i, "unterminated comment")),
                }
            }
            if text[i..].starts_with("<?") {
                match text[i..].find("?>") {
                    Some(e) => {
                        i += e + 2;
                        continue;
                    }
                    None => return Err(err(i, "unterminated declaration")),
                }
            }
            if i + 1 < b.len() && b[i + 1] == b'/' {
                match text[i..].find('>') {
                    Some(e) => i += e + 1,
                    None => return Err(err(i, "unterminated closing tag")),
                }
                if let Some((_, was_group)) = stack.pop() {
                    if was_group {
                        current = None;
                    }
                }
                continue;
            }

            // Opening tag.
            let tag_at = i;
            i += 1;
            let name_start = i;
            while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
                i += 1;
            }
            let tag = text[name_start..i].to_string();

            let mut attrs: BTreeMap<String, String> = BTreeMap::new();
            let mut self_closing = false;
            loop {
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i >= b.len() {
                    return Err(err(tag_at, "unterminated tag"));
                }
                if b[i] == b'>' {
                    i += 1;
                    break;
                }
                if b[i] == b'/' {
                    if i + 1 < b.len() && b[i + 1] == b'>' {
                        self_closing = true;
                        i += 2;
                        break;
                    }
                    i += 1;
                    continue;
                }

                // Attribute name.
                let ns = i;
                while i < b.len()
                    && !b[i].is_ascii_whitespace()
                    && b[i] != b'='
                    && b[i] != b'>'
                    && b[i] != b'/'
                {
                    i += 1;
                }
                let key = text[ns..i].to_string();

                let mut j = i;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && b[j] == b'=' {
                    j += 1;
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                        let q = b[j];
                        let vs = j + 1;
                        let mut k = vs;
                        while k < b.len() && b[k] != q {
                            k += 1;
                        }
                        if k >= b.len() {
                            return Err(err(tag_at, "unterminated attribute value"));
                        }
                        if !key.is_empty() {
                            attrs.insert(key, text[vs..k].to_string());
                        }
                        i = k + 1;
                    } else {
                        let vs = j;
                        let mut k = j;
                        while k < b.len()
                            && !b[k].is_ascii_whitespace()
                            && b[k] != b'>'
                            && !(b[k] == b'/' && k + 1 < b.len() && b[k + 1] == b'>')
                        {
                            k += 1;
                        }
                        if !key.is_empty() {
                            attrs.insert(key, text[vs..k].to_string());
                        }
                        i = k;
                    }
                } else {
                    if i == ns {
                        // Nothing consumed: skip one byte so we always make progress.
                        i += 1;
                    } else if !key.is_empty() {
                        attrs.insert(key, String::new());
                    }
                }
            }

            let is_group = tag == "Resources" || tag == "CompositeResources";
            if tag == "ResourceManifest" {
                manifest.version = attrs.get("version").cloned();
            } else if is_group {
                manifest.groups.push(Group {
                    tag: tag.clone(),
                    attrs,
                    resources: Vec::new(),
                });
                if !self_closing {
                    current = Some(manifest.groups.len() - 1);
                }
            } else if let Some(gi) = current {
                manifest.groups[gi].resources.push(Resource {
                    kind: ResourceKind::from_tag(&tag),
                    attrs,
                });
            }
            if !self_closing {
                stack.push((tag, is_group));
            }
        }
        Ok(manifest)
    }

    /// Every resource with the group it belongs to.
    pub fn iter(&self) -> impl Iterator<Item = (&Group, &Resource)> {
        self.groups
            .iter()
            .flat_map(|g| g.resources.iter().map(move |r| (g, r)))
    }

    fn find_where(&self, id: &str, keep: impl Fn(&Group) -> bool) -> Option<&Resource> {
        self.groups
            .iter()
            .filter(|g| keep(g))
            .flat_map(|g| g.resources.iter())
            .find(|r| r.id() == id)
    }

    /// Look a resource up by ID. With `res` set (for example `Some("1200")`),
    /// groups of that resolution set are searched first, then groups that have
    /// no resolution (shared groups). With `None`, the first definition wins.
    pub fn find(&self, id: &str, res: Option<&str>) -> Option<&Resource> {
        match res {
            None => self.find_where(id, |_| true),
            Some(want) => self
                .find_where(id, |g| g.res() == Some(want))
                .or_else(|| self.find_where(id, |g| g.res().is_none())),
        }
    }

    /// Like [`Manifest::find`], but follows `!ref:` aliases to the final
    /// resource. `None` when the ID or any target is missing, or when the
    /// aliases form a loop.
    pub fn resolve(&self, id: &str, res: Option<&str>) -> Option<&Resource> {
        let mut current = self.find(id, res)?;
        for _ in 0..8 {
            match current.reference() {
                Some(target) => current = self.find(target, res)?,
                None => return Some(current),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<!-- generated -->
<ResourceManifest version=2>
  <Resources id=1200_Main res=1200 parent=Main>
    <Image id=IMAGE_A path="images\1200\a" x=-3 y=7 cols=2 rows=3 nobits nopal a8r8g8b8 time=1319591295 />
    <Image id=IMAGE_B path="images\1200\b" origpath="images\1200\a"/>
    <Sound id=SOUND_X path="sounds\x" volume=0.500000 />
    <File id="" path="ambient\Coastal.ogg" />
  </Resources>
  <Resources id=600_Main res=600 parent=Main>
    <Image id=IMAGE_A path="images\600\a" />
  </Resources>
  <Resources id=2DOnly>
  </Resources>
  <CompositeResources id=All>
    <Group id=1200_Main />
  </CompositeResources>
</ResourceManifest>
"#;

    #[test]
    fn parses_unquoted_values_and_flags() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(m.version.as_deref(), Some("2"));
        assert_eq!(m.groups.len(), 4);

        let a = m.find("IMAGE_A", Some("1200")).unwrap();
        assert_eq!(a.path(), Some("images\\1200\\a"));
        assert_eq!(a.offset(), Some((-3, 7)));
        assert_eq!(a.grid(), Some((2, 3)));
        assert!(a.has_flag("nobits") && a.has_flag("nopal") && a.has_flag("a8r8g8b8"));
        assert!(!a.has_flag("origpath"));
        assert_eq!(a.int("time"), Some(1319591295));
    }

    #[test]
    fn lookup_respects_the_resolution_set() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(
            m.find("IMAGE_A", Some("600")).unwrap().path(),
            Some("images\\600\\a")
        );
        // Without a resolution the first definition wins.
        assert_eq!(
            m.find("IMAGE_A", None).unwrap().path(),
            Some("images\\1200\\a")
        );
        assert!(m.find("IMAGE_A", Some("768")).is_none());
        assert!(m.find("NOPE", None).is_none());
    }

    #[test]
    fn kinds_aliases_and_other_elements() {
        let m = Manifest::parse(SAMPLE).unwrap();
        let b = m.find("IMAGE_B", Some("1200")).unwrap();
        assert_eq!(b.alias_of(), Some("images\\1200\\a"));
        assert_eq!(b.grid(), None);

        let s = m.find("SOUND_X", None).unwrap();
        assert_eq!(s.kind, ResourceKind::Sound);
        assert_eq!(s.attr("volume"), Some("0.500000"));

        let files: Vec<_> = m.iter().filter(|(_, r)| r.kind == ResourceKind::File).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.id(), "");

        let composite = m.groups.iter().find(|g| g.tag == "CompositeResources").unwrap();
        assert_eq!(composite.resources.len(), 1);
        assert_eq!(composite.resources[0].kind, ResourceKind::Other("Group".to_string()));
    }

    #[test]
    fn groups_without_resolution_have_none() {
        let m = Manifest::parse(SAMPLE).unwrap();
        let g = m.groups.iter().find(|g| g.id() == "2DOnly").unwrap();
        assert_eq!(g.res(), None);
        assert!(g.resources.is_empty());
    }

    const REFS: &str = r#"<ResourceManifest version=2>
  <Resources id=600_Init res=600>
    <Font id=FONT_REAL path="fonts\600\real.txt" />
  </Resources>
  <Resources id=1200_Init res=1200>
  </Resources>
  <Resources id=Init_Common>
    <Font id=FONT_REAL path="fonts\1200\real.txt" />
    <Font id=FONT_ALIAS path="!ref:FONT_REAL" />
    <Font id=FONT_LOOP_A path="!ref:FONT_LOOP_B" />
    <Font id=FONT_LOOP_B path="!ref:FONT_LOOP_A" />
    <Font id=FONT_DANGLING path="!ref:NOPE" />
  </Resources>
</ResourceManifest>"#;

    #[test]
    fn aliases_resolve_at_the_requested_resolution() {
        let m = Manifest::parse(REFS).unwrap();
        let alias = m.find("FONT_ALIAS", Some("600")).unwrap();
        assert_eq!(alias.reference(), Some("FONT_REAL"));

        // 600 has its own definition, so it wins over the shared one.
        assert_eq!(
            m.resolve("FONT_ALIAS", Some("600")).unwrap().path(),
            Some("fonts\\600\\real.txt")
        );
        // 1200 has no definition of its own, so the shared group is used.
        assert_eq!(
            m.resolve("FONT_ALIAS", Some("1200")).unwrap().path(),
            Some("fonts\\1200\\real.txt")
        );
        assert!(m.resolve("FONT_ALIAS", None).is_some());
        // A plain resource resolves to itself.
        assert_eq!(
            m.resolve("FONT_REAL", Some("600")).unwrap().path(),
            Some("fonts\\600\\real.txt")
        );
    }

    #[test]
    fn loops_and_dangling_aliases_give_none() {
        let m = Manifest::parse(REFS).unwrap();
        assert!(m.resolve("FONT_LOOP_A", Some("600")).is_none());
        assert!(m.resolve("FONT_DANGLING", Some("600")).is_none());
        assert!(m.resolve("MISSING", None).is_none());
    }

    #[test]
    fn broken_input_gives_a_line_number() {
        let e = Manifest::parse("<ResourceManifest>\n<Resources id=\"x\n").unwrap_err();
        assert_eq!(e.line, 2);
    }

    #[test]
    fn handles_a_byte_order_mark_and_empty_input() {
        let m = Manifest::parse("\u{feff}<ResourceManifest version=2></ResourceManifest>").unwrap();
        assert_eq!(m.version.as_deref(), Some("2"));
        assert!(Manifest::parse("").unwrap().groups.is_empty());
    }
}
