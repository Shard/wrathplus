//! Bounded effective-enUS sample, not stock completeness or rendering parity.
use wrathplus_adt::{parse_adt, Mh2oVertexFormat, ParsedAdt};
use wrathplus_formats::Chain;
use std::{collections::BTreeMap, io::Cursor};

fn word(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

#[test]
fn installed_wrath_mh2o_selector_sample() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let mut counts = BTreeMap::new();
    for (map, tiles) in [
        (
            "Northrend",
            &[
                (11, 9),
                (30, 23),
                (49, 37),
                (30, 21),
                (31, 21),
                (30, 22),
                (31, 22),
            ][..],
        ),
        ("Azeroth", &[(35, 20), (37, 39), (34, 61)][..]),
        ("Kalimdor", &[(0, 0), (29, 31), (42, 55)][..]),
        ("Expansion01", &[(42, 6), (15, 33), (60, 44)][..]),
    ] {
        for &(x, y) in tiles {
            let bytes = chain
                .read_file(&format!("World\\Maps\\{map}\\{map}_{x}_{y}.adt"))
                .unwrap();
            let ParsedAdt::Root(root) = parse_adt(&mut Cursor::new(bytes.as_slice())).unwrap();
            // Independent top-level raw chunk walk, not production field offsets.
            let mut at = 0;
            let mut payload = None;
            while at + 8 <= bytes.len() {
                let end = at + 8 + word(&bytes, at + 4);
                assert!(end <= bytes.len());
                if &bytes[at..at + 4] == b"O2HM" {
                    payload = Some(&bytes[at + 8..end]);
                }
                at = end;
            }
            let Some(raw) = payload else {
                assert!(root.mh2o.is_none());
                continue;
            };
            let surfaces =
                wrathplus_formats::wrath_liquid_surface::build_wrath_tile_surfaces(&root).unwrap();
            let mut surface_index = 0;
            for chunk in &root.mcnk_chunks {
                let h = &chunk.header;
                let e = &root.mh2o.as_ref().unwrap().entries[(h.index_y * 16 + h.index_x) as usize];
                for layer in &e.layers {
                    let surface = &surfaces[surface_index];
                    surface_index += 1;
                    let step = wrathplus_formats::CHUNK_SIZE / 8.0;
                    assert_eq!(
                        surface.positions[0],
                        [
                            h.position[0] - f32::from(layer.offset_y) * step,
                            h.position[1] - f32::from(layer.offset_x) * step,
                            layer.heights[0],
                        ]
                    );
                    let parent = surface.parent.expect("tile surface parent provenance");
                    assert_eq!(parent.index, [h.index_x, h.index_y]);
                    assert_eq!(parent.origin, h.position);
                    assert_eq!(surface.layer.liquid_type, layer.liquid_type);
                    assert_eq!(surface.attributes, e.attributes);
                }
            }
            assert_eq!(surface_index, surfaces.len());
            let parsed = root.mh2o.as_ref().unwrap();
            for entry in 0..256 {
                let offset = word(raw, entry * 12);
                let count = word(raw, entry * 12 + 4);
                assert_eq!(parsed.entries[entry].layers.len(), count);
                for i in 0..count {
                    let h = offset + i * 24;
                    let selector = u16::from_le_bytes(raw[h + 2..h + 4].try_into().unwrap());
                    let o = word(raw, h + 20);
                    *counts.entry((selector, o == 0)).or_insert(0usize) += 1;
                    let layer = &parsed.entries[entry].layers[i];
                    let n = (raw[h + 14] as usize + 1) * (raw[h + 15] as usize + 1);
                    let surface =
                        wrathplus_formats::wrath_liquid_surface::build_wrath_liquid_surface(
                            layer,
                            parsed.entries[entry].attributes,
                            [0.0; 2],
                            [-1.0; 2],
                        )
                        .unwrap();
                    assert_eq!(surface.positions.len(), n);
                    let w = raw[h + 14] as usize;
                    let height = raw[h + 15] as usize;
                    let mask = word(raw, h + 16);
                    let mut expected_indices = Vec::new();
                    for cell in 0..w * height {
                        let present = mask == 0 || raw[mask + cell / 8] & (1 << (cell % 8)) != 0;
                        if present {
                            let base = ((cell / w) * (w + 1) + cell % w) as u32;
                            let stride = w as u32 + 1;
                            expected_indices.extend_from_slice(&[
                                base,
                                base + stride,
                                base + stride + 1,
                                base,
                                base + stride + 1,
                                base + 1,
                            ]);
                        }
                    }
                    assert_eq!(surface.indices, expected_indices);
                    for (j, p) in surface.positions.iter().enumerate() {
                        assert_eq!(p[0], -((raw[h + 13] as usize + j / (w + 1)) as f32));
                        assert_eq!(p[1], -((raw[h + 12] as usize + j % (w + 1)) as f32));
                        assert_eq!(p[2].to_bits(), layer.heights[j].to_bits());
                    }

                    assert!(layer.texture_coordinates.is_none()); // no S1 in this sample
                    if selector == 2 {
                        assert_eq!(layer.vertex_format, Mh2oVertexFormat::Depth);
                        assert_eq!(layer.heights, vec![0.0; n]);
                        let expected = if o == 0 {
                            vec![255; n]
                        } else {
                            raw[o..o + n].to_vec()
                        };
                        assert_eq!(layer.depths.as_ref().unwrap(), &expected);
                    } else {
                        assert_eq!(selector, 0);
                        assert_eq!(layer.vertex_format, Mh2oVertexFormat::HeightDepth);
                        assert_ne!(o, 0);
                        for j in 0..n {
                            assert_eq!(layer.heights[j].to_bits(), word(raw, o + j * 4) as u32);
                        }
                        assert_eq!(layer.depths.as_deref().unwrap(), &raw[o + 4 * n..o + 5 * n]);
                    }
                }
            }
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([((0, false), 147), ((2, false), 93), ((2, true), 1443)])
    );
}
