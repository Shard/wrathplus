//! MCLQ liquid surfaces to per-chunk liquid meshes, in raw WoW coords (+X north, +Y west, +Z up).
//!
//! An MCNK carries one 804-byte MCLQ block per set liquid header bit, each a 9×9 absolute-height
//! grid and an 8×8 cell-flag grid. Each wet cell is two triangles; a dry cell (nibble `0xf`,
//! corners at the FLT_MAX sentinel) is skipped. The cell nibble is the liquid type (`0x6ba970`,
//! `0x68d9b0`); the header bits only count the blocks.
//!
//! 3.3.5 ships its terrain liquid as MH2O ([`crate::wrath_liquid_surface`]); this reader is the
//! lane for a tile without it. The installed cases its 1.12 tests pinned now run on MH2O in
//! `wrathplus-world`'s `liquid::wrath_real_data`.

use wrathplus_adt::MclqChunk;

use crate::terrain::{snap_to_lattice, UNIT_SIZE};

/// A liquid's animated texture set and render path; ADT liquid is only Still, Ocean or Magma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiquidKind {
    /// Still water, `XTextures\river\lake_a.*` (30 frames): lakes, ponds, slow rivers, WMO canals.
    Still,
    /// Rapids, `XTextures\river\fast_a.*` (16 frames), nibble 8 in the name table `0x86a000`. No
    /// shipped MCLQ or MLIQ cell carries 8, and the ADT path cannot reach it.
    Rapids,
    /// Ocean, `XTextures\ocean\ocean_h.*` (30 frames).
    Ocean,
    /// Magma, `XTextures\lava\lava.*`: opaque and unlit, but fogged like every liquid batch, since
    /// no liquid setup overrides the device's fog-on default (`0x593d18`). Both paths reach it; its
    /// ADT UVs are authored per vertex ([`wrathplus_adt::LiquidVertex::texcoords`]).
    Magma,
    /// Slime, `XTextures\slime\slime.*`: magma's render path with its own texture (one handler,
    /// `0x6b68f0`). WMO only (nibbles 3 and 7): the ADT dispatch `0x68de40` has no slime queue.
    Slime,
}

impl LiquidKind {
    /// The kind a WMO tile nibble selects: it indexes the animated-texture name table `0x86a000`
    /// directly, with 4, 6 and 7 as variants of 0, 2 and 3.
    pub fn from_nibble(nibble: u8) -> Option<LiquidKind> {
        match nibble & 0xf {
            0 | 4 => Some(LiquidKind::Still),
            1 => Some(LiquidKind::Ocean),
            2 | 6 => Some(LiquidKind::Magma),
            3 | 7 => Some(LiquidKind::Slime),
            8 => Some(LiquidKind::Rapids),
            _ => None, // 5, 9..=0xf: empty name-table slots and the 0xf hole
        }
    }

    /// The kind an ADT cell nibble selects, by its class `nibble & 3`: the reference draws ADT
    /// liquid in three queues with fixed textures, `lake_a` (`0x6851b0`, `0x68db9d`), `ocean_h`
    /// (`0x685010`, `0x68dabb`) and `lava` (`0x6855a0`, `0x68dcab`), and none for slime. The WMO
    /// path binds the raw nibble instead (`0x68aac0`), as [`Self::from_nibble`] does.
    pub fn from_adt_nibble(nibble: u8) -> Option<LiquidKind> {
        match nibble & 3 {
            0 => Some(LiquidKind::Still),
            1 => Some(LiquidKind::Ocean),
            2 => Some(LiquidKind::Magma),
            _ => None, // class 3, slime: no ADT queue
        }
    }

    /// Whether this kind is opaque and unlit, its texture the body colour (magma, slime); water and
    /// ocean take the `ocean0_s.bls` depth-swatch path. Unlit is not unfogged (`0x593d18`).
    pub fn is_fullbright(self) -> bool {
        matches!(self, LiquidKind::Magma | LiquidKind::Slime)
    }
}

/// One liquid surface, an MCNK's MCLQ or a WMO group's MLIQ: a regular vertex grid in raw WoW
/// coords and the triangles over its wet cells. Queries read the grid, as the reference samples
/// liquid height bilinearly over the containing cell's corners (`0x6b7500`). Drawn two-sided, as
/// every reference liquid pass turns culling off (`0x6851da`, `0x6b6302`).
#[derive(Debug, Clone)]
pub struct LiquidMesh {
    /// Vertex counts `[cols, rows]`, cols fastest: `[9, 9]` for MCLQ, the header's
    /// `xverts`/`yverts` for MLIQ. Every per-vertex array is this grid, row-major.
    pub grid: [u32; 2],
    /// Per-cell coverage over `(cols−1)·(rows−1)`: the nibble says liquid and, for MCLQ, the
    /// corners are real. The containment truth, since the grid's bounds span dry cells.
    pub wet: Vec<bool>,
    /// Per cell, the MLIQ tile flag's `0x80`: another group claims the cell too. The reference
    /// draws it only for the group winning a per-frame 2-colouring of the portal graph, so the
    /// translucent overlap draws once; applied in `wrathplus_assets::wmo`, `false` on the ADT path.
    pub shared: Vec<bool>,
    /// Vertex positions in WoW yards, `cols·rows` of them.
    pub positions: Vec<[f32; 3]>,
    /// Texture UVs: generated at ¼ per cell for water and ocean (WMO liquid in model space, same
    /// period); ADT magma's are authored per vertex (`u = s · 3/256`), since the reference's lava
    /// batch is single-stage with a reset texture matrix.
    pub uvs: Vec<[f32; 2]>,
    /// Per-vertex swatch coord V (0..1) from the MCLQ depth byte: `clamp(byte/42)` for river and
    /// lake, `clamp(byte/255)` for ocean. The reference indexes the depth swatch with this one V
    /// for both the body colour and the opacity ramp (`colorTex.a = 127 + 2·row`). 0 on ADT magma,
    /// whose union bytes are texture coords and whose vert-fill (`0x68d890`) reads no ramp.
    pub depths: Vec<f32>,
    /// Triangle indices, 6 per wet cell in row-major order, derived from [`Self::wet`].
    pub indices: Vec<u32>,
    /// The sound nibble, class `& 3` and FluidSpeed `& 0xc`: the majority wet cell's (ADT) or the
    /// `0x6ba970` one (WMO), keyed through `SoundWaterType.dbc` (`0x462a40`). Kept beside `kind`,
    /// which merges the river speeds (nibbles 0 and 4) that the sound table splits.
    pub sound_nibble: u8,
    /// WMO only: the MLIQ header's `materialId`, an index into the root's MOMT, whose `diffColor`
    /// is the reference's interior water body colour. `None` on the ADT path.
    pub material_id: Option<u16>,
    pub kind: LiquidKind,
}

/// 9×9 vertex grid per MCNK.
const GRID: usize = 9;
/// 8×8 cell grid per MCNK.
const CELLS: usize = 8;
/// Cell-flag low nibble meaning "dry / do not render".
const DRY_NIBBLE: u8 = 0x0f;
/// The reference's scale on a magma vertex's authored `u16` texcoords, `0x3c400000` (3/256):
/// `u = (s as i32 as f64 · scale) as f32`, bit-exact to `0x68d890`.
const MAGMA_TEXCOORD_SCALE: f32 = f32::from_bits(0x3c40_0000);
/// Verts under no liquid carry FLT_MAX (`0x7F7FFFFF`), which is finite, so gate on magnitude.
const HEIGHT_SENTINEL: f32 = 1.0e9;
/// The river and lake depth byte at which V saturates, `clamp(byte/42)`: the LUT `0xc81768`, built
/// by `0x68c4c0` as `clamp((d/9)/4.6667)` (`0x81028c`, `0x810380`) and read by the river fill
/// `0x68d790`. About 5 yd of water.
const RIVER_DEPTH_V_SATURATION: f32 = 42.0;
/// The ocean depth byte at which V saturates, `clamp(byte/255)`: the ocean's own LUT `0xc7fcd8`,
/// built beside the river's (`0x68c57c`) and read by the ocean fill `0x68d690` at `0x68d718`, as
/// the river fill reads its own at `0x68d818`. The divisors differ because the bytes do: over every
/// Azeroth and Kalimdor MCLQ block (`examples/liquid_depth_census`) the sea is authored at 1.72
/// byte/yd against the river's 8.96, and 83.4 % of ocean vertices sit at 255.
const OCEAN_DEPTH_V_SATURATION: f32 = 255.0;

/// `WOW_OCEAN_DEPTH_DIV=<divisor>` overrides [`OCEAN_DEPTH_V_SATURATION`] for one run, read once;
/// 42 puts the river's ramp on the sea for a side-by-side at a shoreline.
fn ocean_depth_v_divisor() -> f32 {
    static DIV: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *DIV.get_or_init(|| {
        std::env::var("WOW_OCEAN_DEPTH_DIV")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|d| *d > 0.0)
            .unwrap_or(OCEAN_DEPTH_V_SATURATION)
    })
}

/// Mesh one MCLQ block at the MCNK header's raw WoW `position`; `None` when no cell is wet or the
/// cells are slime, which has no ADT queue.
pub(crate) fn build_liquid_mesh(mclq: &MclqChunk, position: [f32; 3]) -> Option<LiquidMesh> {
    if mclq.vertices.len() < GRID * GRID || mclq.tile_flags.len() < CELLS * CELLS {
        return None;
    }

    // ── Cells first: they decide the kind. ──
    let mut indices = Vec::with_capacity(CELLS * CELLS * 6);
    let mut wet = vec![false; CELLS * CELLS];
    // The majority nibble over drawn cells is both the render kind and the sound class.
    let mut nibble_counts = [0u32; 16];
    for row in 0..CELLS {
        for col in 0..CELLS {
            let flag = mclq.tile_flags[row * CELLS + col] & 0x0f;
            if flag == DRY_NIBBLE {
                continue;
            }
            let tl = (row * GRID + col) as u32;
            let tr = tl + 1;
            let bl = ((row + 1) * GRID + col) as u32;
            let br = bl + 1;
            // A wet cell with a sentinel or NaN corner is a data anomaly; skip it.
            if [tl, tr, bl, br].iter().any(|&i| {
                let raw = mclq.vertices[i as usize].height;
                !raw.is_finite() || raw.abs() >= HEIGHT_SENTINEL
            }) {
                continue;
            }
            nibble_counts[flag as usize] += 1;
            // One gate for `wet` and the triangles, so the queries and the drawing agree.
            wet[row * CELLS + col] = true;
            indices.extend_from_slice(&[tl, bl, br, tl, br, tr]);
        }
    }
    if indices.is_empty() {
        return None;
    }

    // The kind is the cells' nibble, not the header bits; a shipped block is one class throughout.
    let sound_nibble = nibble_counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(n, _)| n as u8)
        .unwrap_or(0);
    let kind = LiquidKind::from_adt_nibble(sound_nibble)?;

    let depth_v_div = match kind {
        LiquidKind::Ocean => ocean_depth_v_divisor(),
        _ => RIVER_DEPTH_V_SATURATION,
    };
    // Magma's UVs are authored and its depth unread; the rest generate UVs and ramp the depth byte.
    let authored_uvs = kind == LiquidKind::Magma;
    let [wx, wy, _wz] = position;

    // Rows step south (−X) and columns east (−Y) like the MCVT outer grid, on the terrain's
    // lattice; Z is the MCLQ absolute height.
    let mut positions = Vec::with_capacity(GRID * GRID);
    let mut uvs = Vec::with_capacity(GRID * GRID);
    let mut depths = Vec::with_capacity(GRID * GRID);
    for n in 0..GRID * GRID {
        let row = (n / GRID) as f32;
        let col = (n % GRID) as f32;
        // A dry vert's sentinel would stretch the AABB to ~1e38 and get a partly wet chunk culled;
        // it takes `min_height`, unreferenced.
        let raw = mclq.vertices[n].height;
        let h = if raw.is_finite() && raw.abs() < HEIGHT_SENTINEL {
            raw
        } else {
            mclq.min_height
        };
        positions.push([
            snap_to_lattice(wx - row * UNIT_SIZE),
            snap_to_lattice(wy - col * UNIT_SIZE),
            h,
        ]);
        if authored_uvs {
            // Unsigned: the reference widens the `u16` to `i32`, so 0xffff is 65535, never −1.
            let [s, t] = mclq.vertices[n].texcoords();
            let scale = f64::from(MAGMA_TEXCOORD_SCALE);
            uvs.push([
                ((f64::from(s)) * scale) as f32,
                ((f64::from(t)) * scale) as f32,
            ]);
            depths.push(0.0);
        } else {
            uvs.push([col * 0.25, row * 0.25]);
            depths.push((mclq.vertices[n].depth_byte() as f32 / depth_v_div).clamp(0.0, 1.0));
        }
    }

    Some(LiquidMesh {
        grid: [GRID as u32, GRID as u32],
        shared: vec![false; wet.len()],
        wet,
        positions,
        uvs,
        depths,
        indices,
        sound_nibble,
        material_id: None, // ADT liquid has no MOMT to index
        kind,
    })
}

impl LiquidMesh {
    /// Drop the cells `keep` rejects (indexed like [`Self::wet`]) and rebuild [`Self::indices`];
    /// the vertex grid stays whole. Applies [`Self::shared`] once the model has every group.
    pub fn retain_cells(&mut self, keep: impl Fn(usize) -> bool) {
        let (cols, rows) = (self.grid[0] as usize, self.grid[1] as usize);
        let (xt, yt) = (cols.saturating_sub(1), rows.saturating_sub(1));
        let mut dropped = false;
        for c in 0..self.wet.len() {
            if self.wet[c] && !keep(c) {
                self.wet[c] = false;
                dropped = true;
            }
        }
        if !dropped {
            return;
        }
        self.indices.clear();
        for ty in 0..yt {
            for tx in 0..xt {
                if !self.wet[ty * xt + tx] {
                    continue;
                }
                let tl = (ty * cols + tx) as u32;
                let (tr, bl) = (tl + 1, ((ty + 1) * cols + tx) as u32);
                let br = bl + 1;
                self.indices.extend_from_slice(&[tl, bl, br, tl, br, tr]);
            }
        }
    }
}
