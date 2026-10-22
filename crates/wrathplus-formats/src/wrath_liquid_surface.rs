//! Lossless MH2O surface geometry; no material classification or renderer integration.
//!
//! Native build-12340 `0x7cdf80` iterates rows outside columns; `0x7ce1f0`
//! admits cells through the row-major mask, and `0x7ce5d0` emits the two triangles
//! below. Position placement is explicit: the caller supplies signed grid steps
//! rather than this module assuming a world/render transform or snapping policy.
use anyhow::{ensure, Result};
use wrathplus_adt::{Mh2oAttributes, Mh2oLayer, Mh2oVertexFormat};

/// Original parent MCNK provenance, not a reconstructed native renderer origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WrathLiquidParent {
    /// Tile-local `[index_x, index_y]`, independent of source-vector order.
    pub index: [u32; 2],
    /// Exact original header XYZ, without snapping or native-origin reconstruction.
    pub origin: [f32; 3],
}

/// A single layer, not merged with overlapping layers or coerced to Vanilla LiquidKind.
#[derive(Debug, Clone)]
pub struct WrathLiquidSurface {
    /// Present for tile-built surfaces; explicit-placement generic surfaces have none.
    pub parent: Option<WrathLiquidParent>,
    /// Complete source metadata, including raw DBC ID, selector, mask, depth bytes and UV words.
    pub layer: Mh2oLayer,
    /// Full 8×8 parent-MCNK bitmaps, not rectangle-local or normalized values.
    pub attributes: Option<Mh2oAttributes>,
    /// Caller-space XY and absolute source heights as Z, row-major across the rectangle.
    pub positions: Vec<[f32; 3]>,
    /// Triangle list; dry cells contribute no triangles. All source vertices remain present.
    pub indices: Vec<u32>,
}

/// Build one MH2O rectangle. `origin_xy` is the parent MCNK origin; `steps_xy`
/// is the signed distance per row along X and per column along Y respectively.
/// For raw WoW world coordinates both are negative. Heights remain absolute;
/// no parent Z is added, no UV/depth normalization or min-height fallback occurs.
///
/// Publicly constructed malformed layers and nonfinite placements are rejected
/// as a safety policy. This does not emulate native malformed-input behavior.
pub fn build_wrath_liquid_surface(
    layer: &Mh2oLayer,
    attributes: Option<Mh2oAttributes>,
    origin_xy: [f32; 2],
    steps_xy: [f32; 2],
) -> Result<WrathLiquidSurface> {
    ensure!(
        layer.width <= 8 && layer.height <= 8,
        "MH2O rectangle dimensions"
    );
    ensure!(
        u16::from(layer.offset_x) + u16::from(layer.width) <= 8
            && u16::from(layer.offset_y) + u16::from(layer.height) <= 8,
        "MH2O rectangle outside MCNK"
    );
    let cols = usize::from(layer.width) + 1;
    let rows = usize::from(layer.height) + 1;
    let n = cols * rows;
    ensure!(layer.heights.len() == n, "MH2O height count");
    ensure!(
        layer.exists.len() == usize::from(layer.width) * usize::from(layer.height),
        "MH2O cell count"
    );
    let uv = layer.vertex_format == Mh2oVertexFormat::HeightTextureCoordinates;
    ensure!(
        layer.texture_coordinates.as_ref().map(Vec::len) == uv.then_some(n),
        "MH2O UV count"
    );
    ensure!(
        layer.depths.as_ref().map(Vec::len) == (!uv).then_some(n),
        "MH2O depth count"
    );
    ensure!(
        origin_xy.into_iter().chain(steps_xy).all(f32::is_finite),
        "MH2O nonfinite placement"
    );
    ensure!(
        layer.heights.iter().all(|v| v.is_finite()),
        "MH2O nonfinite height"
    );
    let mut positions = Vec::with_capacity(n);
    for row in 0..rows {
        for col in 0..cols {
            let x = origin_xy[0] + (usize::from(layer.offset_y) + row) as f32 * steps_xy[0];
            let y = origin_xy[1] + (usize::from(layer.offset_x) + col) as f32 * steps_xy[1];
            ensure!(x.is_finite() && y.is_finite(), "MH2O placement overflow");
            positions.push([x, y, layer.heights[row * cols + col]]);
        }
    }
    let mut indices = Vec::new();
    for row in 0..usize::from(layer.height) {
        for col in 0..usize::from(layer.width) {
            if layer.exists[row * usize::from(layer.width) + col] {
                let base = (row * cols + col) as u32;
                let stride = cols as u32;
                indices.extend_from_slice(&[
                    base,
                    base + stride,
                    base + stride + 1,
                    base,
                    base + stride + 1,
                    base + 1,
                ]);
            }
        }
    }
    Ok(WrathLiquidSurface {
        parent: None,
        layer: layer.clone(),
        attributes,
        positions,
        indices,
    })
}

/// Build from original MCNK headers, not the filtered/snapped terrain-mesh vector.
/// Entry lookup is `index_y * 16 + index_x` (native `0x8a3070`). A chunk
/// without MCVT still contributes liquids. No MH2O means no Wrath surfaces.
pub fn build_wrath_tile_surfaces(root: &wrathplus_adt::RootAdt) -> Result<Vec<WrathLiquidSurface>> {
    let Some(mh2o) = &root.mh2o else {
        return Ok(Vec::new());
    };
    let mut surfaces = Vec::new();
    for chunk in &root.mcnk_chunks {
        let h = &chunk.header;
        ensure!(
            h.index_x < 16 && h.index_y < 16,
            "MH2O MCNK index outside tile"
        );
        let entry = mh2o
            .entries
            .get((h.index_y * 16 + h.index_x) as usize)
            .ok_or_else(|| anyhow::anyhow!("MH2O missing MCNK entry"))?;
        for layer in &entry.layers {
            let mut surface = build_wrath_liquid_surface(
                layer,
                entry.attributes,
                [h.position[0], h.position[1]],
                [-crate::terrain::UNIT_SIZE; 2],
            )?;
            surface.parent = Some(WrathLiquidParent {
                index: [h.index_x, h.index_y],
                origin: h.position,
            });
            surfaces.push(surface);
        }
    }
    Ok(surfaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layer() -> Mh2oLayer {
        Mh2oLayer {
            liquid_type: 65535,
            vertex_format: Mh2oVertexFormat::HeightDepth,
            min_height: -10.,
            max_height: 50.,
            offset_x: 2,
            offset_y: 3,
            width: 2,
            height: 1,
            exists: vec![false, true],
            heights: vec![10., 11., 12., 13., 14., 15.],
            depths: Some(vec![0, 1, 127, 128, 254, 255]),
            texture_coordinates: None,
        }
    }
    fn sparse_root() -> Box<wrathplus_adt::RootAdt> {
        let mut bytes = Vec::new();
        for (x, y, origin) in [(7u32, 2u32, 100.125f32), (1, 9, -50.25)] {
            let mut header = vec![0u8; 128];
            header[4..8].copy_from_slice(&x.to_le_bytes());
            header[8..12].copy_from_slice(&y.to_le_bytes());
            header[0x68..0x6c].copy_from_slice(&origin.to_le_bytes());
            header[0x6c..0x70].copy_from_slice(&(origin + 5.).to_le_bytes());
            header[0x70..0x74].copy_from_slice(&999f32.to_le_bytes());
            bytes.extend_from_slice(b"KNCM");
            bytes.extend_from_slice(&128u32.to_le_bytes());
            bytes.extend(header);
        }
        let wrathplus_adt::ParsedAdt::Root(root) =
            wrathplus_adt::parse_adt(&mut std::io::Cursor::new(bytes.as_slice())).unwrap();
        root
    }
    #[test]
    fn tile_uses_header_indices_and_unsnapped_origins_without_mcvt() {
        let mut root = sparse_root();
        let mut entries = vec![wrathplus_adt::Mh2oEntry::default(); 256];
        entries[2 * 16 + 7].layers.push(layer());
        let mut second = layer();
        second.liquid_type = 123;
        entries[9 * 16 + 1].layers.push(second);
        root.mh2o = Some(wrathplus_adt::Mh2oChunk { entries });
        let s = build_wrath_tile_surfaces(&root).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            s[0].parent,
            Some(WrathLiquidParent {
                index: [7, 2],
                origin: [100.125, 105.125, 999.],
            })
        );
        assert_eq!(
            s[1].parent,
            Some(WrathLiquidParent {
                index: [1, 9],
                origin: [-50.25, -45.25, 999.],
            })
        );
        for (i, origin) in [100.125f32, -50.25].into_iter().enumerate() {
            assert_eq!(
                s[i].positions[0],
                [
                    origin - 3. * crate::terrain::UNIT_SIZE,
                    origin + 5. - 2. * crate::terrain::UNIT_SIZE,
                    10.
                ]
            );
        }
        assert_eq!(s[1].layer.liquid_type, 123);
        root.mcnk_chunks.reverse();
        assert_eq!(
            build_wrath_tile_surfaces(&root).unwrap()[0]
                .layer
                .liquid_type,
            123
        );
        assert_eq!(
            build_wrath_tile_surfaces(&root).unwrap()[0]
                .parent
                .unwrap()
                .index,
            [1, 9]
        );
        root.mh2o = None;
        assert!(build_wrath_tile_surfaces(&root).unwrap().is_empty());
    }
    #[test]
    fn asymmetric_rectangle_mask_diagonal_winding_and_raw_values() {
        let l = layer();
        let attrs = Mh2oAttributes {
            fishable: 1 << 63,
            deep: 0x1234,
        };
        let s = build_wrath_liquid_surface(&l, Some(attrs), [100., 200.], [-4., -5.]).unwrap();
        assert_eq!(
            s.positions,
            vec![
                [88., 190., 10.],
                [88., 185., 11.],
                [88., 180., 12.],
                [84., 190., 13.],
                [84., 185., 14.],
                [84., 180., 15.]
            ]
        );
        assert_eq!(s.indices, [1, 4, 5, 1, 5, 2]);
        // First triangle has +Z winding when both signed steps are negative.
        let [a, b, c] = [s.positions[1], s.positions[4], s.positions[5]];
        assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.);
        assert_eq!(s.layer.liquid_type, 65535);
        assert_eq!(s.layer.depths, l.depths);
        assert_eq!(s.layer.exists, l.exists);
        assert_eq!(s.attributes, Some(attrs));
        assert_eq!(s.parent, None);
    }
    #[test]
    fn authored_uv_words_and_zero_height_depth_defaults_are_not_normalized() {
        let mut l = layer();
        l.vertex_format = Mh2oVertexFormat::HeightTextureCoordinates;
        l.depths = None;
        l.texture_coordinates = Some(vec![[65535, 32768]; 6]);
        let s = build_wrath_liquid_surface(&l, None, [0.; 2], [-1.; 2]).unwrap();
        assert_eq!(s.layer.texture_coordinates, l.texture_coordinates);
        l.vertex_format = Mh2oVertexFormat::Depth;
        l.texture_coordinates = None;
        l.heights = vec![0.; 6];
        l.depths = Some(vec![255; 6]);
        let s = build_wrath_liquid_surface(&l, None, [0.; 2], [-1.; 2]).unwrap();
        assert!(s.positions.iter().all(|p| p[2] == 0.));
        assert_eq!(s.layer.depths, Some(vec![255; 6]));
    }
    #[test]
    fn dry_cells_keep_vertices_but_emit_no_indices() {
        let mut l = layer();
        l.exists.fill(false);
        let s = build_wrath_liquid_surface(&l, None, [0.; 2], [-1.; 2]).unwrap();
        assert_eq!(s.positions.len(), 6);
        assert!(s.indices.is_empty());
    }
    #[test]
    fn malformed_public_layers_fail_closed() {
        let good = layer();
        for mutate in [
            |l: &mut Mh2oLayer| {
                l.heights.pop();
            },
            |l: &mut Mh2oLayer| {
                l.exists.pop();
            },
            |l: &mut Mh2oLayer| {
                l.depths = None;
            },
            |l: &mut Mh2oLayer| {
                l.offset_x = 8;
            },
            |l: &mut Mh2oLayer| {
                l.heights[0] = f32::NAN;
            },
        ] {
            let mut l = good.clone();
            mutate(&mut l);
            assert!(build_wrath_liquid_surface(&l, None, [0.; 2], [-1.; 2]).is_err());
        }
        assert!(build_wrath_liquid_surface(&good, None, [f32::INFINITY, 0.], [-1.; 2]).is_err());
    }
}
