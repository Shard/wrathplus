//! A reader for 1.12.1's monolithic root ADTs: IFF chunks (a reversed magic, a `u32` size, the
//! payload) holding the asset names, the placements and 256 `MCNK` terrain chunks.

use std::io::Cursor;

use benilla_bytes::{chunks, ByteExt};

/// A terrain vertex normal: three signed bytes, named `x, z, y` in disk order.
#[derive(Debug, Clone, Copy)]
pub struct VertexNormal {
    pub x: i8,
    pub z: i8,
    pub y: i8,
}

impl VertexNormal {
    /// `[x, y, z] / 127`, which is disk bytes `[b0, b2, b1]`; the terrain mesher swaps them back.
    pub fn to_normalized(&self) -> [f32; 3] {
        [
            f32::from(self.x) / 127.0,
            f32::from(self.y) / 127.0,
            f32::from(self.z) / 127.0,
        ]
    }
}

/// MCVT: 145 heights (9×9 outer + 8×8 inner, stride-17 interleave).
#[derive(Debug, Clone)]
pub struct McvtChunk {
    pub heights: Vec<f32>,
}

/// MCNR: one normal per MCVT vertex.
#[derive(Debug, Clone)]
pub struct McnrChunk {
    pub normals: Vec<VertexNormal>,
}

/// MCLY per-layer flags.
#[derive(Debug, Clone, Copy)]
pub struct MclyFlags {
    pub value: u32,
}

impl MclyFlags {
    /// Bit 9 (0x200): the layer's MCAL alpha map is RLE-compressed.
    pub fn alpha_map_compressed(&self) -> bool {
        self.value & 0x200 != 0
    }
}

/// MCLY: one texture layer (16 bytes on disk).
#[derive(Debug, Clone)]
pub struct MclyLayer {
    pub texture_id: u32,
    pub flags: MclyFlags,
    pub offset_in_mcal: u32,
    pub effect_id: u32,
}

/// MCLY: the chunk's texture layers (0–4; layer 0 is the opaque base).
#[derive(Debug, Clone)]
pub struct MclyChunk {
    pub layers: Vec<MclyLayer>,
}

/// MCAL: raw alpha-map bytes (indexed per layer via [`MclyLayer::offset_in_mcal`]).
#[derive(Debug, Clone)]
pub struct McalChunk {
    pub data: Vec<u8>,
}

/// MCSH: a 64×64 1-bit baked shadow map (512 bytes).
#[derive(Debug, Clone)]
pub struct McshChunk {
    pub shadow_map: Vec<u8>,
}

impl McshChunk {
    /// Whether texel `(x, y)` (column, row; both 0..63) is shadowed.
    pub fn is_shadowed(&self, x: usize, y: usize) -> bool {
        if x >= 64 || y >= 64 {
            return false;
        }
        let byte = self.shadow_map.get(y * 8 + x / 8).copied().unwrap_or(0);
        (byte >> (x % 8)) & 1 != 0
    }
}

/// One MCLQ liquid vertex, 8 bytes: 4 union bytes and an absolute height. The union follows the
/// cell's liquid type: a depth byte and flow for water and ocean, a UV pair for magma and slime.
#[derive(Debug, Clone, Copy)]
pub struct LiquidVertex {
    pub union_data: [u8; 4],
    pub height: f32,
}

impl LiquidVertex {
    /// The water or ocean depth byte (union byte 0).
    pub fn depth_byte(&self) -> u8 {
        self.union_data[0]
    }

    /// A magma block's authored `(s, t)`, two little-endian `u16`s the reference's lava fill takes
    /// as its UV (`0x68d890`: `u = (s as i32 as f64 · 3/256) as f32`, same for `t`). The field is
    /// world-continuous, so lava tiles seamlessly across MCNK borders.
    pub fn texcoords(&self) -> [u16; 2] {
        [
            u16::from_le_bytes([self.union_data[0], self.union_data[1]]),
            u16::from_le_bytes([self.union_data[2], self.union_data[3]]),
        ]
    }
}

/// One MCLQ liquid block: a 9×9 absolute-height grid and an 8×8 cell-flag grid. An MCNK carries
/// one per set liquid header bit, back to back (`0x6af7a3`–`0x6af7cb`): two at the 28 shipped river
/// mouths. The liquid type is each cell's flag nibble (`0x6ba970`, `0x68d9b0`), not a header bit.
#[derive(Debug, Clone)]
pub struct MclqChunk {
    pub min_height: f32,
    pub max_height: f32,
    pub vertices: Vec<LiquidVertex>,
    /// Per-cell flags, row-major 8×8: the low nibble is the liquid type (`0xf` dry), `0x80`
    /// shared. `0x40`, fishable, is unread: the reference's only reader (`0x69b5d0`) has no
    /// caller, and the server decides fishability.
    pub tile_flags: [u8; 64],
}

/// Build-12340 MH2O payload layouts (native accessors 0x8a3130/60/b0).
/// Later combined height/UV/depth layouts are not supported by this client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mh2oVertexFormat {
    HeightDepth,
    HeightTextureCoordinates,
    Depth,
}

impl Mh2oVertexFormat {
    fn from_raw(raw: u16) -> Option<Self> {
        match raw {
            0 => Some(Self::HeightDepth),
            1 => Some(Self::HeightTextureCoordinates),
            2 => Some(Self::Depth),
            _ => None,
        }
    }
}

/// One Wrath MH2O liquid layer. Cell coverage is local to the layer's `width × height` rectangle;
/// vertex arrays are `(width + 1) × (height + 1)`, row-major. `liquid_type` is a
/// `LiquidType.dbc` id: rendering requires the parent loader to resolve its sound/material class.
#[derive(Debug, Clone)]
pub struct Mh2oLayer {
    pub liquid_type: u16,
    pub vertex_format: Mh2oVertexFormat,
    pub min_height: f32,
    pub max_height: f32,
    pub offset_x: u8,
    pub offset_y: u8,
    pub width: u8,
    pub height: u8,
    pub exists: Vec<bool>,
    /// Absolute liquid heights. Selector 2 is zero; selectors 0/1 require a payload.
    /// Missing height data is rejected rather than synthesizing `min_height`.
    pub heights: Vec<f32>,
    pub depths: Option<Vec<u8>>,
    pub texture_coordinates: Option<Vec<[u16; 2]>>,
}

/// Per-MCNK MH2O attributes, both bitmaps in the MCNK's full 8×8 cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mh2oAttributes {
    pub fishable: u64,
    pub deep: u64,
}

/// MH2O data for one MCNK, indexed in the same order as the tile's MCNK chunks.
#[derive(Debug, Clone, Default)]
pub struct Mh2oEntry {
    pub layers: Vec<Mh2oLayer>,
    pub attributes: Option<Mh2oAttributes>,
}

/// A bounded Wrath MH2O chunk: exactly 256 MCNK entries when present.
#[derive(Debug, Clone)]
pub struct Mh2oChunk {
    pub entries: Vec<Mh2oEntry>,
}

/// MCNK header flag bit 1: a chunk a mover may not enter, the bands along mountain rims such as
/// Searing Gorge's. The reference's header walk copies it to its chunk flag `0x40` (`0x6af5f0`).
pub const MCNK_IMPASSABLE: u32 = 0x2;

/// The MCNK header fields the renderer reads. The names mislead: `pred_tex` and `no_effect_doodad`
/// are the two halves of the predominant-texture map at `+0x40`, and `unknown_8bytes` is the real
/// noEffectDoodad at `+0x50`; the consumer reassembles them.
#[derive(Debug, Clone)]
pub struct McnkHeader {
    pub flags: u32,
    pub index_x: u32,
    pub index_y: u32,
    /// `AreaTable.dbc` id of the chunk's zone or subzone (`+0x34`).
    pub area_id: u32,
    pub holes_low_res: u16,
    pub pred_tex: [u8; 8],
    pub no_effect_doodad: [u8; 8],
    pub unknown_8bytes: [u8; 8],
    pub position: [f32; 3],
}

impl McnkHeader {
    /// Whether the raw header sets [`MCNK_IMPASSABLE`], which covers the whole 33.333 yd chunk.
    pub fn impassable(&self) -> bool {
        self.flags & MCNK_IMPASSABLE != 0
    }
}

/// One MCNK terrain chunk.
#[derive(Debug, Clone)]
pub struct McnkChunk {
    pub header: McnkHeader,
    pub heights: Option<McvtChunk>,
    pub normals: Option<McnrChunk>,
    pub layers: Option<MclyChunk>,
    pub alpha: Option<McalChunk>,
    pub shadow: Option<McshChunk>,
    /// The MCLQ liquid blocks in disk order: none on a dry chunk, two at a river mouth.
    pub liquids: Vec<MclqChunk>,
}

/// MDDF: a placed M2 doodad (36 bytes).
#[derive(Debug, Clone)]
pub struct DoodadPlacement {
    pub name_id: u32,
    pub unique_id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: u16,
    pub flags: u16,
}

/// MODF: a placed WMO (64 bytes).
#[derive(Debug, Clone)]
pub struct WmoPlacement {
    pub name_id: u32,
    pub unique_id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub flags: u16,
    pub doodad_set: u16,
    /// `WMOAreaTable.NameSetID`: which name and audio variant of the WMO this placement is
    /// (Northshire Abbey and Tyr's Hand Abbey are one WMO).
    pub name_set: u16,
}

/// A parsed vanilla (monolithic) root ADT.
#[derive(Debug, Clone)]
pub struct RootAdt {
    pub textures: Vec<String>,
    pub models: Vec<String>,
    pub wmos: Vec<String>,
    pub doodad_placements: Vec<DoodadPlacement>,
    pub wmo_placements: Vec<WmoPlacement>,
    pub mcnk_chunks: Vec<McnkChunk>,
    /// Wrath's tile-level multi-layer liquid data. Vanilla files have no MH2O.
    pub mh2o: Option<Mh2oChunk>,
}

/// A parsed ADT; vanilla has only the monolithic root.
#[derive(Debug, Clone)]
pub enum ParsedAdt {
    Root(Box<RootAdt>),
}

/// ADT parse error.
#[derive(Debug)]
pub enum Error {
    Truncated(&'static str),
    Invalid(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Truncated(w) => write!(f, "truncated ADT: {w}"),
            Error::Invalid(w) => write!(f, "invalid ADT: {w}"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// The NUL-terminated string at `offset`: an MMDX or MWMO name by its MMID or MWID offset.
fn cstring_at(blob: &[u8], offset: usize) -> String {
    let end = blob[offset.min(blob.len())..]
        .iter()
        .position(|&c| c == 0)
        .map(|p| offset + p)
        .unwrap_or(blob.len());
    String::from_utf8_lossy(&blob[offset.min(blob.len())..end]).into_owned()
}

/// Parse a vanilla monolithic ADT; the cursor's position is ignored.
pub fn parse_adt(cursor: &mut Cursor<&[u8]>) -> Result<ParsedAdt> {
    let b: &[u8] = cursor.get_ref();

    let mut textures = Vec::new();
    let mut mmdx = Vec::new();
    let mut mmid: Vec<u32> = Vec::new();
    let mut mwmo = Vec::new();
    let mut mwid: Vec<u32> = Vec::new();
    let mut doodad_placements = Vec::new();
    let mut wmo_placements = Vec::new();
    let mut mcnk_chunks = Vec::new();
    let mut mh2o = None;

    for (magic, data) in chunks(b) {
        match &magic {
            b"XETM" => textures = split_strings(data),       // MTEX
            b"XDMM" => mmdx = data.to_vec(),                 // MMDX (blob)
            b"DIMM" => mmid = read_u32_list(data, "MMID")?,  // MMID (offsets)
            b"OMWM" => mwmo = data.to_vec(),                 // MWMO (blob)
            b"DIWM" => mwid = read_u32_list(data, "MWID")?,  // MWID (offsets)
            b"FDDM" => doodad_placements = read_mddf(data)?, // MDDF
            b"FDOM" => wmo_placements = read_modf(data)?,    // MODF
            b"KNCM" => mcnk_chunks.push(read_mcnk(data)?),   // MCNK
            b"O2HM" => mh2o = Some(read_mh2o(data)?),        // MH2O
            _ => {}
        }
    }

    // MDDF/MODF `name_id` indexes the MMID/MWID offset tables.
    let models = mmid
        .iter()
        .map(|&o| cstring_at(&mmdx, o as usize))
        .collect();
    let wmos = mwid
        .iter()
        .map(|&o| cstring_at(&mwmo, o as usize))
        .collect();

    Ok(ParsedAdt::Root(Box::new(RootAdt {
        textures,
        models,
        wmos,
        doodad_placements,
        wmo_placements,
        mcnk_chunks,
        mh2o,
    })))
}

/// Split a NUL-separated, NUL-terminated string blob, dropping empties.
fn split_strings(blob: &[u8]) -> Vec<String> {
    blob.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

fn read_u32_list(data: &[u8], name: &'static str) -> Result<Vec<u32>> {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|c| c.u32_at(0).ok_or(Error::Truncated(name)))
        .collect()
}

fn read_mddf(data: &[u8]) -> Result<Vec<DoodadPlacement>> {
    let mut out = Vec::new();
    for r in data.as_chunks::<36>().0 {
        out.push(DoodadPlacement {
            name_id: r.u32_at(0).ok_or(Error::Truncated("MDDF"))?,
            unique_id: r.u32_at(4).ok_or(Error::Truncated("MDDF"))?,
            position: [
                r.f32_at(8).ok_or(Error::Truncated("MDDF"))?,
                r.f32_at(12).ok_or(Error::Truncated("MDDF"))?,
                r.f32_at(16).ok_or(Error::Truncated("MDDF"))?,
            ],
            rotation: [
                r.f32_at(20).ok_or(Error::Truncated("MDDF"))?,
                r.f32_at(24).ok_or(Error::Truncated("MDDF"))?,
                r.f32_at(28).ok_or(Error::Truncated("MDDF"))?,
            ],
            scale: r.u16_at(32).ok_or(Error::Truncated("MDDF"))?,
            flags: r.u16_at(34).ok_or(Error::Truncated("MDDF"))?,
        });
    }
    Ok(out)
}

fn read_modf(data: &[u8]) -> Result<Vec<WmoPlacement>> {
    let mut out = Vec::new();
    for r in data.as_chunks::<64>().0 {
        out.push(WmoPlacement {
            name_id: r.u32_at(0).ok_or(Error::Truncated("MODF"))?,
            unique_id: r.u32_at(4).ok_or(Error::Truncated("MODF"))?,
            position: [
                r.f32_at(8).ok_or(Error::Truncated("MODF"))?,
                r.f32_at(12).ok_or(Error::Truncated("MODF"))?,
                r.f32_at(16).ok_or(Error::Truncated("MODF"))?,
            ],
            rotation: [
                r.f32_at(20).ok_or(Error::Truncated("MODF"))?,
                r.f32_at(24).ok_or(Error::Truncated("MODF"))?,
                r.f32_at(28).ok_or(Error::Truncated("MODF"))?,
            ],
            // 0x20..0x38: the bounding box (6 f32), unread.
            flags: r.u16_at(56).ok_or(Error::Truncated("MODF"))?,
            doodad_set: r.u16_at(58).ok_or(Error::Truncated("MODF"))?,
            name_set: r.u16_at(60).ok_or(Error::Truncated("MODF"))?,
            // 0x3E: a u16, unread.
        });
    }
    Ok(out)
}

/// Parse one MCNK: the 128-byte header, then the sub-chunks at the header's offsets.
fn read_mcnk(data: &[u8]) -> Result<McnkChunk> {
    if data.len() < 128 {
        return Err(Error::Truncated("MCNK header"));
    }
    let h = &data[..128];
    let header = McnkHeader {
        flags: h.u32_at(0x00).ok_or(Error::Truncated("MCNK header"))?,
        index_x: h.u32_at(0x04).ok_or(Error::Truncated("MCNK header"))?,
        index_y: h.u32_at(0x08).ok_or(Error::Truncated("MCNK header"))?,
        area_id: h.u32_at(0x34).ok_or(Error::Truncated("MCNK header"))?,
        holes_low_res: h.u16_at(0x3C).ok_or(Error::Truncated("MCNK header"))?,
        pred_tex: h
            .bytes_at(0x40, 8)
            .ok_or(Error::Truncated("MCNK header"))?
            .try_into()
            .unwrap(),
        no_effect_doodad: h
            .bytes_at(0x48, 8)
            .ok_or(Error::Truncated("MCNK header"))?
            .try_into()
            .unwrap(),
        unknown_8bytes: h
            .bytes_at(0x50, 8)
            .ok_or(Error::Truncated("MCNK header"))?
            .try_into()
            .unwrap(),
        position: [
            h.f32_at(0x68).ok_or(Error::Truncated("MCNK header"))?,
            h.f32_at(0x6C).ok_or(Error::Truncated("MCNK header"))?,
            h.f32_at(0x70).ok_or(Error::Truncated("MCNK header"))?,
        ],
    };

    let mut chunk = McnkChunk {
        header,
        heights: None,
        normals: None,
        layers: None,
        alpha: None,
        shadow: None,
        liquids: Vec::new(),
    };

    // Sub-chunks sit at the header's offsets, not in sequence (AhnQiraj's are not contiguous).
    // An offset may count from the MCNK magic or from its data, so the reversed magic is checked
    // at both; yields the payload's `(start, size)`.
    let locate = |ofs: u32, magic: &[u8; 4]| -> Option<(usize, usize)> {
        if ofs == 0 {
            return None;
        }
        let ofs = ofs as usize;
        for hdr in [Some(ofs), ofs.checked_sub(8)].into_iter().flatten() {
            if data.bytes_at(hdr, 4) == Some(magic.as_slice()) {
                let size = data.u32_at(hdr + 4)?;
                return Some((hdr + 8, size as usize));
            }
        }
        None
    };
    // `locate` proves `start` in bounds; `len` is unvalidated, so the end saturates, then clamps.
    let slice = |start: usize, len: usize| &data[start..start.saturating_add(len).min(data.len())];

    if let Some((p, n)) = locate(
        h.u32_at(0x14).ok_or(Error::Truncated("MCNK header"))?,
        b"TVCM",
    )
    .filter(|&(_, n)| n > 0)
    {
        let sub = slice(p, n);
        let mut heights = Vec::new();
        for i in 0..145.min(sub.len() / 4) {
            heights.push(sub.f32_at(i * 4).ok_or(Error::Truncated("MCVT"))?);
        }
        chunk.heights = Some(McvtChunk { heights });
    }
    if let Some((p, n)) = locate(
        h.u32_at(0x18).ok_or(Error::Truncated("MCNK header"))?,
        b"RNCM",
    )
    .filter(|&(_, n)| n > 0)
    {
        let sub = slice(p, n);
        let mut normals = Vec::new();
        for i in 0..145.min(sub.len() / 3) {
            normals.push(VertexNormal {
                x: sub.u8_at(i * 3).ok_or(Error::Truncated("MCNR"))? as i8,
                z: sub.u8_at(i * 3 + 1).ok_or(Error::Truncated("MCNR"))? as i8,
                y: sub.u8_at(i * 3 + 2).ok_or(Error::Truncated("MCNR"))? as i8,
            });
        }
        chunk.normals = Some(McnrChunk { normals });
    }
    if let Some((p, n)) = locate(
        h.u32_at(0x1C).ok_or(Error::Truncated("MCNK header"))?,
        b"YLCM",
    )
    .filter(|&(_, n)| n > 0)
    {
        let mut layers = Vec::new();
        for l in slice(p, n).as_chunks::<16>().0 {
            layers.push(MclyLayer {
                texture_id: l.u32_at(0).ok_or(Error::Truncated("MCLY"))?,
                flags: MclyFlags {
                    value: l.u32_at(4).ok_or(Error::Truncated("MCLY"))?,
                },
                offset_in_mcal: l.u32_at(8).ok_or(Error::Truncated("MCLY"))?,
                effect_id: l.u32_at(12).ok_or(Error::Truncated("MCLY"))?,
            });
        }
        chunk.layers = Some(MclyChunk { layers });
    }
    // MCAL's length is the header's `size_alpha`, not the sub-chunk's own size: they differ on some
    // files (AhnQiraj).
    if let Some((p, _)) = locate(
        h.u32_at(0x24).ok_or(Error::Truncated("MCNK header"))?,
        b"LACM",
    ) {
        let size_alpha = h.u32_at(0x28).ok_or(Error::Truncated("MCNK header"))? as usize;
        chunk.alpha = Some(McalChunk {
            data: slice(p, size_alpha).to_vec(),
        });
    }
    if let Some((p, n)) = locate(
        h.u32_at(0x2C).ok_or(Error::Truncated("MCNK header"))?,
        b"HSCM",
    )
    .filter(|&(_, n)| n > 0)
    {
        let sub = slice(p, n);
        let mut shadow_map = vec![0u8; 512];
        let k = sub.len().min(512);
        shadow_map[..k].copy_from_slice(&sub[..k]);
        chunk.shadow = Some(McshChunk { shadow_map });
    }
    // MCLQ's length is the header's `size_liquid` too: its own declared size is always 0.
    if let Some((p, _)) = locate(
        h.u32_at(0x60).ok_or(Error::Truncated("MCNK header"))?,
        b"QLCM",
    ) {
        let size_liquid = h.u32_at(0x64).ok_or(Error::Truncated("MCNK header"))? as usize;
        chunk.liquids = read_mclq_blocks(slice(p, size_liquid), chunk.header.flags);
    }

    Ok(chunk)
}

/// Bytes per MCLQ block: `{f32 min, f32 max, 81×8B verts, 64B cell flags, u32 nFlow, 2×40B flow}`.
const MCLQ_BLOCK: usize = 0x324;
/// Liquid header bits 2–5: how many are set is how many MCLQ blocks there are.
const MCNK_LIQUID_BITS: u32 = 0x3c;

/// Read the packed MCLQ blocks, stopping at the first one the payload cannot hold. A chunk with an
/// MCLQ sub-chunk but no liquid bit set still yields one block; the reference creates one per set
/// bit only (`0x6af760`).
fn read_mclq_blocks(sub: &[u8], mcnk_flags: u32) -> Vec<MclqChunk> {
    let want = (mcnk_flags & MCNK_LIQUID_BITS).count_ones().max(1) as usize;
    let mut out = Vec::with_capacity(want);
    for i in 0..want {
        let Some(rest) = sub.get(i * MCLQ_BLOCK..) else {
            break;
        };
        match read_mclq_block(rest) {
            Some(b) => out.push(b),
            None => break,
        }
    }
    out
}

/// Too-short MCLQ data is no liquid, not an error. The minimum stops short of [`MCLQ_BLOCK`]: the
/// trailing flow data is unread.
fn read_mclq_block(sub: &[u8]) -> Option<MclqChunk> {
    const NEED: usize = 8 + 81 * 8 + 64;
    if sub.len() < NEED {
        return None;
    }
    let min_height = sub.f32_at(0)?;
    let max_height = sub.f32_at(4)?;
    let mut vertices = Vec::with_capacity(81);
    for i in 0..81 {
        let o = 8 + i * 8;
        vertices.push(LiquidVertex {
            union_data: sub.bytes_at(o, 4)?.try_into().ok()?,
            height: sub.f32_at(o + 4)?,
        });
    }
    let tile_flags = sub.bytes_at(8 + 81 * 8, 64)?.try_into().ok()?;
    Some(MclqChunk {
        min_height,
        max_height,
        vertices,
        tile_flags,
    })
}

const MH2O_ENTRY_COUNT: usize = 16 * 16;
const MH2O_HEADER_SIZE: usize = 12;
const MH2O_INSTANCE_SIZE: usize = 24;

/// Parse MH2O using offsets relative to its payload. Every multiplication and offset is checked
/// before slicing, so corrupt layer counts and dimensions cannot wrap or allocate from file data.
fn read_mh2o(data: &[u8]) -> Result<Mh2oChunk> {
    let headers_len = MH2O_ENTRY_COUNT * MH2O_HEADER_SIZE;
    if data.len() < headers_len {
        return Err(Error::Truncated("MH2O headers"));
    }
    let mut entries = Vec::with_capacity(MH2O_ENTRY_COUNT);
    for index in 0..MH2O_ENTRY_COUNT {
        let h = &data[index * MH2O_HEADER_SIZE..][..MH2O_HEADER_SIZE];
        let instances_offset = h.u32_at(0).ok_or(Error::Truncated("MH2O header"))? as usize;
        let layer_count = h.u32_at(4).ok_or(Error::Truncated("MH2O header"))? as usize;
        let attributes_offset = h.u32_at(8).ok_or(Error::Truncated("MH2O header"))? as usize;
        if layer_count == 0 {
            entries.push(Mh2oEntry::default());
            continue;
        }
        if instances_offset == 0 {
            return Err(Error::Invalid("MH2O layers have no instance offset"));
        }
        let instances_len = layer_count
            .checked_mul(MH2O_INSTANCE_SIZE)
            .ok_or(Error::Invalid("MH2O instance count overflow"))?;
        let instances_end = instances_offset
            .checked_add(instances_len)
            .ok_or(Error::Invalid("MH2O instance offset overflow"))?;
        let instances = data
            .get(instances_offset..instances_end)
            .ok_or(Error::Truncated("MH2O instances"))?;
        let attributes = if attributes_offset == 0 {
            None
        } else {
            let end = attributes_offset
                .checked_add(16)
                .ok_or(Error::Invalid("MH2O attributes offset overflow"))?;
            let a = data
                .get(attributes_offset..end)
                .ok_or(Error::Truncated("MH2O attributes"))?;
            Some(Mh2oAttributes {
                fishable: u64::from_le_bytes(a[0..8].try_into().unwrap()),
                deep: u64::from_le_bytes(a[8..16].try_into().unwrap()),
            })
        };
        let mut layers = Vec::with_capacity(layer_count.min(8));
        for raw in instances.as_chunks::<MH2O_INSTANCE_SIZE>().0 {
            layers.push(read_mh2o_layer(data, raw)?);
        }
        entries.push(Mh2oEntry { layers, attributes });
    }
    Ok(Mh2oChunk { entries })
}

fn read_mh2o_layer(data: &[u8], raw: &[u8; MH2O_INSTANCE_SIZE]) -> Result<Mh2oLayer> {
    let liquid_type = raw.u16_at(0).ok_or(Error::Truncated("MH2O instance"))?;
    let vertex_format =
        Mh2oVertexFormat::from_raw(raw.u16_at(2).ok_or(Error::Truncated("MH2O instance"))?)
            .ok_or(Error::Invalid("MH2O vertex format"))?;
    let min_height = raw.f32_at(4).ok_or(Error::Truncated("MH2O instance"))?;
    let max_height = raw.f32_at(8).ok_or(Error::Truncated("MH2O instance"))?;
    let (offset_x, offset_y, width, height) = (raw[12], raw[13], raw[14], raw[15]);
    if width == 0
        || height == 0
        || usize::from(offset_x) + usize::from(width) > 8
        || usize::from(offset_y) + usize::from(height) > 8
    {
        return Err(Error::Invalid("MH2O liquid rectangle"));
    }
    let mask_offset = raw.u32_at(16).ok_or(Error::Truncated("MH2O instance"))? as usize;
    let vertex_offset = raw.u32_at(20).ok_or(Error::Truncated("MH2O instance"))? as usize;
    let cell_count = usize::from(width)
        .checked_mul(usize::from(height))
        .ok_or(Error::Invalid("MH2O cell count overflow"))?;
    let vertex_count = (usize::from(width) + 1)
        .checked_mul(usize::from(height) + 1)
        .ok_or(Error::Invalid("MH2O vertex count overflow"))?;
    let exists = if mask_offset == 0 {
        vec![true; cell_count]
    } else {
        let byte_count = cell_count.div_ceil(8);
        let end = mask_offset
            .checked_add(byte_count)
            .ok_or(Error::Invalid("MH2O bitmap offset overflow"))?;
        let mask = data
            .get(mask_offset..end)
            .ok_or(Error::Truncated("MH2O exists bitmap"))?;
        (0..cell_count)
            .map(|bit| mask[bit / 8] & (1 << (bit % 8)) != 0)
            .collect()
    };

    // Native height accessor has no offset-zero fallback for selectors 0/1.
    // Fail closed instead of reading the MH2O header as floats or inventing a plane.
    if vertex_offset == 0 && vertex_format != Mh2oVertexFormat::Depth {
        return Err(Error::Invalid("MH2O missing height payload"));
    }
    let mut heights = vec![0.0; vertex_count];
    // Native 0x8a3160 explicitly returns 255 for selector 2 with offset zero.
    let mut depths = (vertex_format == Mh2oVertexFormat::Depth).then(|| vec![255; vertex_count]);
    let mut texture_coordinates = None;
    if vertex_offset != 0 {
        let block_len = vertex_count
            .checked_mul(4)
            .ok_or(Error::Invalid("MH2O vertex data overflow"))?;
        let has_heights = vertex_format != Mh2oVertexFormat::Depth;
        let has_uvs = vertex_format == Mh2oVertexFormat::HeightTextureCoordinates;
        let has_depths = matches!(
            vertex_format,
            Mh2oVertexFormat::HeightDepth | Mh2oVertexFormat::Depth
        );
        let total_len = (if has_heights { block_len } else { 0usize })
            .checked_add(if has_uvs { block_len } else { 0 })
            .and_then(|n| n.checked_add(if has_depths { vertex_count } else { 0 }))
            .ok_or(Error::Invalid("MH2O vertex data overflow"))?;
        let end = vertex_offset
            .checked_add(total_len)
            .ok_or(Error::Invalid("MH2O vertex offset overflow"))?;
        let vertex_data = data
            .get(vertex_offset..end)
            .ok_or(Error::Truncated("MH2O vertex data"))?;
        let mut cursor = 0;
        if has_heights {
            for (i, height) in heights.iter_mut().enumerate() {
                *height = vertex_data
                    .f32_at(i * 4)
                    .ok_or(Error::Truncated("MH2O heights"))?;
            }
            cursor += block_len;
        }
        if has_uvs {
            let uv_data = &vertex_data[cursor..cursor + block_len];
            texture_coordinates = Some(
                (0..vertex_count)
                    .map(|i| {
                        Ok([
                            uv_data
                                .u16_at(i * 4)
                                .ok_or(Error::Truncated("MH2O texture coordinates"))?,
                            uv_data
                                .u16_at(i * 4 + 2)
                                .ok_or(Error::Truncated("MH2O texture coordinates"))?,
                        ])
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
            cursor += block_len;
        }
        if has_depths {
            depths = Some(vertex_data[cursor..cursor + vertex_count].to_vec());
        }
    }
    Ok(Mh2oLayer {
        liquid_type,
        vertex_format,
        min_height,
        max_height,
        offset_x,
        offset_y,
        width,
        height,
        exists,
        heights,
        depths,
        texture_coordinates,
    })
}

mod combined_alpha;
pub use combined_alpha::CombinedAlphaMap;

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one IFF chunk: reversed magic + `u32` size + payload.
    fn chunk(magic: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(magic);
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    /// An MCNK header and one MCVT after it, the `0x14` offset pointing at MCVT's magic, 128 in.
    fn mcnk_with_mcvt() -> Vec<u8> {
        let mut data = vec![0u8; 128];
        data[0x14..0x18].copy_from_slice(&128u32.to_le_bytes());
        let heights: Vec<u8> = (0..145u32).flat_map(|i| (i as f32).to_le_bytes()).collect();
        data.extend(chunk(b"TVCM", &heights));
        data
    }

    fn parse(bytes: &[u8]) -> Result<ParsedAdt> {
        parse_adt(&mut Cursor::new(bytes))
    }

    #[test]
    fn root_parses_a_minimal_well_formed_chunk_walk() {
        let mtex = b"tex1.blp\0tex2.blp\0";
        let mmdx = b"model.m2\0";
        let mmid = 0u32.to_le_bytes();
        let mwmo = b"world.wmo\0";
        let mwid = 0u32.to_le_bytes();

        let mut mddf = vec![0u8; 36];
        mddf[32..34].copy_from_slice(&7u16.to_le_bytes()); // scale

        let mut modf = vec![0u8; 64];
        modf[58..60].copy_from_slice(&3u16.to_le_bytes()); // doodad_set

        let mut b = chunk(b"XETM", mtex);
        b.extend(chunk(b"XDMM", mmdx));
        b.extend(chunk(b"DIMM", &mmid));
        b.extend(chunk(b"OMWM", mwmo));
        b.extend(chunk(b"DIWM", &mwid));
        b.extend(chunk(b"FDDM", &mddf));
        b.extend(chunk(b"FDOM", &modf));
        b.extend(chunk(b"KNCM", &mcnk_with_mcvt()));

        let ParsedAdt::Root(root) = parse(&b).expect("parses");
        assert_eq!(root.textures, vec!["tex1.blp", "tex2.blp"]);
        assert_eq!(root.models, vec!["model.m2"]);
        assert_eq!(root.wmos, vec!["world.wmo"]);
        assert_eq!(root.doodad_placements.len(), 1);
        assert_eq!(root.doodad_placements[0].scale, 7);
        assert_eq!(root.wmo_placements.len(), 1);
        assert_eq!(root.wmo_placements[0].doodad_set, 3);
        assert_eq!(root.mcnk_chunks.len(), 1);
        let heights = &root.mcnk_chunks[0]
            .heights
            .as_ref()
            .expect("MCVT parsed")
            .heights;
        assert_eq!(heights.len(), 145);
        assert_eq!(heights[1], 1.0);
    }

    #[test]
    fn partial_trailing_mddf_record_is_dropped_not_a_panic() {
        // Shorter than the 36-byte stride: `as_chunks` drops the remainder.
        let b = chunk(b"FDDM", &[0u8; 10]);
        let ParsedAdt::Root(root) = parse(&b).expect("parses");
        assert!(root.doodad_placements.is_empty());
    }

    #[test]
    fn truncated_mcnk_header_is_an_error_not_a_panic() {
        let b = chunk(b"KNCM", &[0u8; 40]);
        assert!(matches!(parse(&b), Err(Error::Truncated("MCNK header"))));
    }

    #[test]
    fn corrupt_mcnk_subchunk_offset_skips_cleanly_never_panics() {
        // ofs = 3: the minus-8 candidate underflows and 3 holds no magic.
        let mut data = vec![0u8; 128];
        data[0x14..0x18].copy_from_slice(&3u32.to_le_bytes());
        let b = chunk(b"KNCM", &data);
        let ParsedAdt::Root(root) = parse(&b).expect("parses despite corrupt MCVT offset");
        assert!(root.mcnk_chunks[0].heights.is_none());

        // ofs points past the end of the buffer entirely.
        let mut data = vec![0u8; 128];
        data[0x14..0x18].copy_from_slice(&u32::MAX.to_le_bytes());
        let b = chunk(b"KNCM", &data);
        let ParsedAdt::Root(root) = parse(&b).expect("parses despite out-of-range MCVT offset");
        assert!(root.mcnk_chunks[0].heights.is_none());
    }

    #[test]
    fn short_mclq_is_treated_as_no_liquid_not_an_error() {
        let mut data = vec![0u8; 128];
        data[0x60..0x64].copy_from_slice(&128u32.to_le_bytes()); // MCLQ offset
        data[0x64..0x68].copy_from_slice(&4u32.to_le_bytes()); // size_liquid, far short of NEED
        data.extend(chunk(b"QLCM", &[0u8; 4]));
        let b = chunk(b"KNCM", &data);
        let ParsedAdt::Root(root) = parse(&b).expect("parses");
        assert!(root.mcnk_chunks[0].liquids.is_empty());
    }

    /// Each block keeps its own cell flags, so the river and the ocean tell apart by nibble.
    #[test]
    fn a_two_bit_mcnk_yields_two_mclq_blocks_in_disk_order() {
        let block = |height: f32, nibble: u8| {
            let mut b = vec![0u8; MCLQ_BLOCK];
            b[0..4].copy_from_slice(&height.to_le_bytes());
            b[4..8].copy_from_slice(&height.to_le_bytes());
            for i in 0..81 {
                b[8 + i * 8 + 4..8 + i * 8 + 8].copy_from_slice(&height.to_le_bytes());
            }
            b[8 + 81 * 8..8 + 81 * 8 + 64].fill(nibble);
            b
        };
        let mut payload = block(5.0, 4); // the river, first on disk
        payload.extend(block(0.0, 1)); // the ocean, second

        let mut data = vec![0u8; 128];
        data[0x00..0x04].copy_from_slice(&0x0cu32.to_le_bytes()); // liquid bits 2 and 3
        data[0x60..0x64].copy_from_slice(&128u32.to_le_bytes());
        data[0x64..0x68].copy_from_slice(&((payload.len() + 8) as u32).to_le_bytes());
        data.extend(chunk(b"QLCM", &payload));
        let b = chunk(b"KNCM", &data);
        let ParsedAdt::Root(root) = parse(&b).expect("parses");

        let liquids = &root.mcnk_chunks[0].liquids;
        assert_eq!(liquids.len(), 2, "one block per set liquid bit");
        assert_eq!(liquids[0].tile_flags[0] & 0xf, 4, "block 0 is the river");
        assert_eq!(liquids[0].min_height, 5.0);
        assert_eq!(liquids[1].tile_flags[0] & 0xf, 1, "block 1 is the ocean");
        assert_eq!(liquids[1].min_height, 0.0);
    }

    /// `0x68d890` reads the magma pair as `u16 → i32`, so `0xffff` is 65535, never −1.
    #[test]
    fn magma_union_bytes_read_as_two_unsigned_u16_texcoords() {
        let v = LiquidVertex {
            union_data: [0xbd, 0x00, 0xe8, 0x00],
            height: 1.0,
        };
        assert_eq!(v.texcoords(), [189, 232]); // a real Burning Steppes vertex
        assert_eq!(v.depth_byte(), 0xbd, "the same bytes, read the water way");
        let max = LiquidVertex {
            union_data: [0xff, 0xff, 0xff, 0xff],
            height: 1.0,
        };
        assert_eq!(max.texcoords(), [65535, 65535], "unsigned, not −1");
    }

    fn mh2o_with_layer(format: u16, width: u8, height: u8, vertex_data: &[u8]) -> Vec<u8> {
        let mut payload = vec![0u8; MH2O_ENTRY_COUNT * MH2O_HEADER_SIZE];
        let instance_offset = payload.len();
        payload[0..4].copy_from_slice(&(instance_offset as u32).to_le_bytes());
        payload[4..8].copy_from_slice(&1u32.to_le_bytes());
        let mut instance = [0u8; MH2O_INSTANCE_SIZE];
        instance[0..2].copy_from_slice(&7u16.to_le_bytes());
        instance[2..4].copy_from_slice(&format.to_le_bytes());
        instance[4..8].copy_from_slice(&12.5f32.to_le_bytes());
        instance[8..12].copy_from_slice(&18.0f32.to_le_bytes());
        instance[12..16].copy_from_slice(&[1, 2, width, height]);
        let mask_offset = instance_offset + MH2O_INSTANCE_SIZE;
        instance[16..20].copy_from_slice(&(mask_offset as u32).to_le_bytes());
        instance[20..24].copy_from_slice(&((mask_offset + 1) as u32).to_le_bytes());
        payload.extend_from_slice(&instance);
        payload.push(0b0000_0010);
        payload.extend_from_slice(vertex_data);
        chunk(b"O2HM", &payload)
    }

    #[test]
    fn mh2o_reads_masks_and_all_wrath_vertex_formats() {
        for format in 0..=2u16 {
            let vertex_count = 6usize; // width 2, height 1
            let mut vertex_data = Vec::new();
            if format != 2 {
                for n in 0..vertex_count {
                    vertex_data.extend_from_slice(&(20.0 + n as f32).to_le_bytes());
                }
            }
            if format == 1 {
                for n in 0..vertex_count as u16 {
                    vertex_data.extend_from_slice(&(n + 100).to_le_bytes());
                    vertex_data.extend_from_slice(&(n + 200).to_le_bytes());
                }
            }
            if format == 0 || format == 2 {
                vertex_data.extend(10..10 + vertex_count as u8);
            }
            let ParsedAdt::Root(root) =
                parse(&mh2o_with_layer(format, 2, 1, &vertex_data)).unwrap();
            let layer = &root.mh2o.as_ref().unwrap().entries[0].layers[0];
            assert_eq!(layer.exists, [false, true]);
            assert_eq!(layer.heights.len(), vertex_count);
            assert_eq!(layer.heights[0], if format == 2 { 0.0 } else { 20.0 });
            assert_eq!(layer.depths.is_some(), matches!(format, 0 | 2));
            assert_eq!(layer.texture_coordinates.is_some(), format == 1);
        }
    }

    #[test]
    fn mh2o_zero_payload_defaults_only_for_depth() {
        for format in 0..=2 {
            let mut bytes = mh2o_with_layer(format, 2, 1, &[]);
            let instance = 8 + MH2O_ENTRY_COUNT * MH2O_HEADER_SIZE;
            bytes[instance + 16..instance + 24].fill(0);
            if format < 2 {
                assert!(matches!(
                    parse(&bytes),
                    Err(Error::Invalid("MH2O missing height payload"))
                ));
            } else {
                let ParsedAdt::Root(root) = parse(&bytes).unwrap();
                let layer = &root.mh2o.unwrap().entries[0].layers[0];
                assert_eq!(layer.heights, [0.0; 6]);
                assert_eq!(layer.depths.as_deref(), Some([255; 6].as_slice()));
                assert_eq!(layer.exists, [true; 2]);
                assert!(layer.texture_coordinates.is_none());
            }
        }
    }

    #[test]
    fn mh2o_rejects_later_vertex_layouts() {
        for format in [3, 4, u16::MAX] {
            assert!(matches!(
                parse(&mh2o_with_layer(format, 2, 1, &[0; 54])),
                Err(Error::Invalid("MH2O vertex format"))
            ));
        }
    }

    #[test]
    fn mh2o_rejects_truncation_and_overflowing_rectangles() {
        let short = chunk(b"O2HM", &[0u8; 12]);
        assert!(matches!(
            parse(&short),
            Err(Error::Truncated("MH2O headers"))
        ));

        let bad_rect = mh2o_with_layer(2, 8, 1, &[0u8; 18]);
        assert!(matches!(
            parse(&bad_rect),
            Err(Error::Invalid("MH2O liquid rectangle"))
        ));

        let mut payload = vec![0u8; MH2O_ENTRY_COUNT * MH2O_HEADER_SIZE];
        payload[0..4].copy_from_slice(&3072u32.to_le_bytes());
        payload[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let overflow = chunk(b"O2HM", &payload);
        assert!(matches!(
            parse(&overflow),
            Err(Error::Truncated("MH2O instances")) | Err(Error::Invalid(_))
        ));
    }
}
