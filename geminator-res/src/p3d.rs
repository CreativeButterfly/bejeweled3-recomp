//! Parser for `.p3d` 3D model files (the board, the gems and the tunnel).
//!
//! Layout (little-endian, worked out from the owner's own files; 10 of the 12
//! models parse with every byte accounted for):
//!
//! ```text
//! magic        00 EF BE 3D
//! u32          version (1 or 2)
//! u16 n, then n strings            names of the shapes
//! u16 n, then n entries            materials; in version 2 each entry has one
//!                                  extra byte before its string
//! u16 n, then n (key, value)       properties, for example
//!                                  texture0.fileName = "red_gem.png"
//! then one or more mesh blocks until the end of the file:
//!   u16  flags
//!   u32  vertex format (always 338 so far; it selects the 36-byte vertex)
//!   u16  vertex count
//!   vertex count * 36 bytes:  position 3 x f32, normal 3 x f32,
//!                             color 4 x u8, uv 2 x f32
//!   u16  triangle count
//!   triangle count * 3 * u16  indices
//! ```
//!
//! A string is a u16 length followed by that many bytes (Latin-1).
//!
//! Textures: the file name in `texture0.fileName` is a leftover from the
//! modeling tool (it may say `.png` or `.psd`); the real image is stored in the
//! PAK under `images/<size>/tex/<name without extension>`, see
//! [`Model::texture_stem`].
//!
//! Not understood yet: `temple.p3d` and `questtemple.p3d` do not parse with
//! this layout (probably several meshes with a different header).

use std::fmt;

const MAGIC: [u8; 4] = [0x00, 0xEF, 0xBE, 0x3D];
const VERTEX_FORMAT: u32 = 338;
const VERTEX_STRIDE: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub flags: u16,
    pub vertices: Vec<Vertex>,
    /// Triangle list: every three indices form one triangle.
    pub indices: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub version: u32,
    pub names: Vec<String>,
    pub materials: Vec<String>,
    pub properties: Vec<(String, String)>,
    pub meshes: Vec<Mesh>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    pub message: String,
}

impl ModelError {
    fn new(message: impl Into<String>) -> ModelError {
        ModelError {
            message: message.into(),
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model error: {}", self.message)
    }
}

impl std::error::Error for ModelError {}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ModelError> {
        let end = self
            .i
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or_else(|| ModelError::new(format!("unexpected end of file at byte {}", self.i)))?;
        let s = &self.b[self.i..end];
        self.i = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, ModelError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ModelError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn u32(&mut self) -> Result<u32, ModelError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn string(&mut self) -> Result<String, ModelError> {
        let n = self.u16()? as usize;
        let bytes = self.take(n)?;
        Ok(bytes.iter().map(|&b| b as char).collect())
    }

    fn remaining(&self) -> usize {
        self.b.len() - self.i
    }
}

fn f32_at(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

impl Model {
    pub fn parse(bytes: &[u8]) -> Result<Model, ModelError> {
        let mut r = Reader { b: bytes, i: 0 };
        if r.take(4)? != &MAGIC[..] {
            return Err(ModelError::new("not a .p3d file (bad magic number)"));
        }
        let version = r.u32()?;
        if version != 1 && version != 2 {
            return Err(ModelError::new(format!("unsupported version {version}")));
        }

        let mut names = Vec::new();
        for _ in 0..r.u16()? {
            names.push(r.string()?);
        }
        let mut materials = Vec::new();
        for _ in 0..r.u16()? {
            if version >= 2 {
                r.u8()?;
            }
            materials.push(r.string()?);
        }
        let mut properties = Vec::new();
        for _ in 0..r.u16()? {
            let key = r.string()?;
            let value = r.string()?;
            properties.push((key, value));
        }

        let mut meshes = Vec::new();
        while r.remaining() > 0 {
            let flags = r.u16()?;
            let format = r.u32()?;
            if format != VERTEX_FORMAT {
                return Err(ModelError::new(format!(
                    "unsupported vertex format {format} (expected {VERTEX_FORMAT})"
                )));
            }
            let vertex_count = r.u16()? as usize;
            let data = r.take(vertex_count * VERTEX_STRIDE)?;
            let vertices: Vec<Vertex> = data
                .chunks_exact(VERTEX_STRIDE)
                .map(|c| Vertex {
                    position: [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8)],
                    normal: [f32_at(c, 12), f32_at(c, 16), f32_at(c, 20)],
                    color: [c[24], c[25], c[26], c[27]],
                    uv: [f32_at(c, 28), f32_at(c, 32)],
                })
                .collect();

            let triangle_count = r.u16()? as usize;
            let idx_bytes = r.take(triangle_count * 3 * 2)?;
            let indices: Vec<u16> = idx_bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            if let Some(&bad) = indices.iter().find(|&&i| i as usize >= vertex_count) {
                return Err(ModelError::new(format!(
                    "index {bad} is out of range for {vertex_count} vertices"
                )));
            }
            meshes.push(Mesh {
                flags,
                vertices,
                indices,
            });
        }
        if meshes.is_empty() {
            return Err(ModelError::new("file has no mesh data"));
        }

        Ok(Model {
            version,
            names,
            materials,
            properties,
            meshes,
        })
    }

    /// The `texture0.fileName` property as written in the file.
    pub fn texture(&self) -> Option<&str> {
        self.properties
            .iter()
            .find(|(k, _)| k.as_str() == "texture0.fileName")
            .map(|(_, v)| v.as_str())
    }

    /// The texture name without its extension, lowercased. The image itself is
    /// at `images/<size>/tex/<stem>` in the PAK (`.jp2`, `.gif`, ...).
    pub fn texture_stem(&self) -> Option<String> {
        let t = self.texture()?;
        let stem = match t.rfind('.') {
            Some(i) => &t[..i],
            None => t,
        };
        Some(stem.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_str(b: &mut Vec<u8>, t: &str) {
        b.extend_from_slice(&(t.len() as u16).to_le_bytes());
        b.extend_from_slice(t.as_bytes());
    }

    /// One mesh with three vertices and one triangle.
    fn build(version: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&MAGIC);
        b.extend_from_slice(&version.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        push_str(&mut b, "shape1");
        b.extend_from_slice(&1u16.to_le_bytes());
        if version >= 2 {
            b.push(1);
        }
        push_str(&mut b, "mat1");
        b.extend_from_slice(&1u16.to_le_bytes());
        push_str(&mut b, "texture0.fileName");
        push_str(&mut b, "Red_Gem.png");

        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&VERTEX_FORMAT.to_le_bytes());
        b.extend_from_slice(&3u16.to_le_bytes());
        for k in 0..3 {
            for f in [k as f32, 0.0, 1.0, 0.0, 1.0, 0.0] {
                b.extend_from_slice(&f.to_le_bytes());
            }
            b.extend_from_slice(&[255, 128, 0, 255]);
            for f in [0.25f32, 0.75] {
                b.extend_from_slice(&f.to_le_bytes());
            }
        }
        b.extend_from_slice(&1u16.to_le_bytes());
        for i in [0u16, 1, 2] {
            b.extend_from_slice(&i.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_version_1_and_2() {
        for version in [1u32, 2] {
            let m = Model::parse(&build(version)).unwrap();
            assert_eq!(m.version, version);
            assert_eq!(m.names, vec!["shape1".to_string()]);
            assert_eq!(m.materials, vec!["mat1".to_string()]);
            assert_eq!(m.texture(), Some("Red_Gem.png"));
            assert_eq!(m.texture_stem(), Some("red_gem".to_string()));

            assert_eq!(m.meshes.len(), 1);
            let mesh = &m.meshes[0];
            assert_eq!(mesh.indices, vec![0, 1, 2]);
            let v = mesh.vertices[2];
            assert_eq!(v.position, [2.0, 0.0, 1.0]);
            assert_eq!(v.normal, [0.0, 1.0, 0.0]);
            assert_eq!(v.color, [255, 128, 0, 255]);
            assert_eq!(v.uv, [0.25, 0.75]);
        }
    }

    #[test]
    fn rejects_bad_input() {
        let good = build(1);

        let mut bad_magic = good.clone();
        bad_magic[0] = 0x01;
        assert!(Model::parse(&bad_magic).is_err());

        assert!(Model::parse(&build(7)).is_err());

        // Cut off at every possible length: never a panic, always an error.
        for n in 0..good.len() {
            assert!(Model::parse(&good[..n]).is_err(), "length {n}");
        }

        let mut trailing = good.clone();
        trailing.extend_from_slice(&[1, 2, 3]);
        assert!(Model::parse(&trailing).is_err());

        let mut bad_index = good.clone();
        let n = bad_index.len();
        bad_index[n - 2] = 9;
        assert!(Model::parse(&bad_index).is_err());
    }

    #[test]
    fn texture_stem_without_an_extension() {
        let mut m = Model::parse(&build(1)).unwrap();
        m.properties = vec![("texture0.fileName".to_string(), "NoExt".to_string())];
        assert_eq!(m.texture_stem(), Some("noext".to_string()));
        m.properties.clear();
        assert_eq!(m.texture_stem(), None);
    }
}
