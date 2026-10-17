//! Parsers for two data formats in the game's PAK:
//!
//! * [`manifest`]: `properties/resources.xml`. It looks like XML but is not
//!   valid XML (unquoted attribute values, attributes without values), so it
//!   has its own lenient parser.
//! * [`font`]: the font descriptor scripts under `fonts/<size>/*.txt`.
//! * [`p3d`]: the `.p3d` 3D models (board, gems, tunnel).
//!
//! Everything here reads data the player already owns. Nothing is bundled.

pub mod font;
pub mod manifest;
pub mod p3d;

pub use font::{Font, FontError, Glyph, Layer, Rect};
pub use manifest::{Group, Manifest, ParseError, Resource, ResourceKind};
pub use p3d::{Mesh, Model, ModelError, Vertex};
