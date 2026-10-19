//! CPU-only build-12340 render inputs; no material classification or asset loading.
//! Positions stay in caller-supplied raw WoW space. This is not renderer integration.
use benilla_formats::wrath_liquid_surface::WrathLiquidSurface;

/// Bounded allocation policy (bytes, excluding any terminator), not a native path limit.
pub const MAX_TEXTURE_FILENAME_BYTES: usize = 1023;

/// Accept a literal path or exactly one `%d`, expanded in source order over 1..=30.
/// This is deliberately not printf: widths, escapes and all other percent syntax fail.
/// Empty, NUL-containing and oversized paths fail rather than truncate. The bound
/// applies to both the input and every expanded filename; UTF-8 boundaries are preserved.
pub fn texture_filenames(pattern: &str) -> Result<Vec<String>, String> {
    if pattern.is_empty() || pattern.len() > MAX_TEXTURE_FILENAME_BYTES {
        return Err("empty or overlong liquid texture filename".into());
    }
    if pattern.contains('\0') {
        return Err("NUL in liquid texture filename".into());
    }
    let Some(at) = pattern.find('%') else {
        return Ok(vec![pattern.to_owned()]);
    };
    let tail = &pattern[at..];
    if !tail.starts_with("%d") || tail[2..].contains('%') {
        return Err("liquid texture filename requires exactly one %d".into());
    }
    let (prefix, suffix) = (&pattern[..at], &tail[2..]);
    // Longest replacement is two ASCII digits, the same length as `%d`.
    Ok((1..=30).map(|i| format!("{prefix}{i}{suffix}")).collect())
}

/// Native 0x79b870 permits material selectors 0/2 to read the table indexed by
/// LiquidType word 41 (+0xa4); other material selectors produce depth zero.
/// Only indices 0/1 are established. Unknown words fail even on a zero-depth
/// selector: validating metadata must not silently assign meaning to new words.
///
/// Native 0x79e3c0 constants: 1/9 at 0xa3fa24 (f32 bits 0x3de38e39),
/// threshold at 0xa3fae0, reciprocal at 0xa3fadc. The normalized table retains
/// the native i*scale*(1/(255*scale)) evaluation, not a substituted i/255.
/// f64 intermediates approximate x87 extended precision, NOT bit-exact emulation.
pub fn depth_lut(material_selector: u32, type_word_41: u32) -> Result<[f32; 256], String> {
    if type_word_41 > 1 {
        return Err(format!("unsupported LiquidType word 41: {type_word_41}"));
    }
    if !matches!(material_selector, 0 | 2) {
        return Ok([0.; 256]);
    }
    let ninth = f64::from(f32::from_bits(0x3de3_8e39));
    let threshold = f64::from(4.666_666_5_f32);
    let reciprocal = f64::from(0.214_285_72_f32);
    let scale = f64::from(0.583_333_3_f32);
    let inverse = 1.0 / (255.0 * scale);
    Ok(std::array::from_fn(|i| {
        if type_word_41 == 0 {
            let x = i as f64 * ninth;
            if x > threshold {
                1.0
            } else {
                (x * reciprocal) as f32
            }
        } else {
            (i as f64 * scale * inverse) as f32
        }
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct WrathMeshInputs {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub depths: Vec<f32>,
    pub indices: Vec<u32>,
}

/// Build parallel CPU vertex arrays without axis swaps, origin shifts or LiquidKind.
/// `generated_uv_positions` are the caller-audited native batch-transformed positions,
/// of 0x7d4ab0 -> 0x7ce390. For terrain, 0x7cf200 builds T(firstOrigin),
/// and 0x8a575a supplies it to 0x7d4790, restoring native world positions.
/// The caller retains responsibility for native-vs-header origin rounding.
/// The MH2O format, not the material selector, chooses authored vs generated UVs
/// as copied to the runtime surface at +8 by 0x7c5690.
/// Material render selection and this geometry-format decision stay independent. Constants: 0xa40410 (XY multiplier
/// bits 0x3d75c290) and 0xa40414 (unsigned UV words times 3/256).
///
/// The two selectors are independent: e.g. material selector 2 can consume authored
/// UV-format geometry while yielding zero depth because that payload has no bytes.
/// Publicly constructed malformed surfaces fail closed as a safety policy.
pub fn mesh_inputs(
    surface: &WrathLiquidSurface,
    material_selector: u32,
    type_word_41: u32,
    generated_uv_positions: &[[f32; 3]],
) -> Result<WrathMeshInputs, String> {
    let layer = &surface.layer;
    if layer.width > 8
        || layer.height > 8
        || u16::from(layer.offset_x) + u16::from(layer.width) > 8
        || u16::from(layer.offset_y) + u16::from(layer.height) > 8
    {
        return Err("MH2O rectangle outside MCNK".into());
    }
    let n = (usize::from(layer.width) + 1) * (usize::from(layer.height) + 1);
    if surface.positions.len() != n
        || layer.heights.len() != n
        || layer.exists.len() != usize::from(layer.width) * usize::from(layer.height)
    {
        return Err("MH2O geometry length mismatch".into());
    }
    if !layer.heights.iter().all(|v| v.is_finite()) {
        return Err("nonfinite MH2O height".into());
    }
    // benilla-adt's fieldless enum is declared HeightDepth, HeightTextureCoordinates,
    // Depth, in that order. It is not re-exported by formats, nor a direct world
    // dependency. This cast refers to that Rust enum order, NOT a material selector.
    let authored_uv = layer.vertex_format as u8 == 1;
    if layer.texture_coordinates.as_ref().map(Vec::len) != authored_uv.then_some(n)
        || layer.depths.as_ref().map(Vec::len) != (!authored_uv).then_some(n)
    {
        return Err("MH2O UV/depth payload does not match vertex format".into());
    }
    if generated_uv_positions.len() != n
        || !generated_uv_positions
            .iter()
            .flatten()
            .all(|v| v.is_finite())
    {
        return Err("invalid transformed MH2O UV positions".into());
    }
    let lut = depth_lut(material_selector, type_word_41)?;
    let mut inputs = vertex_inputs(
        &surface.positions,
        &surface.indices,
        layer.texture_coordinates.as_deref(),
        layer.depths.as_deref(),
        &lut,
    )?;
    if !authored_uv {
        let scale = f32::from_bits(0x3d75_c290);
        inputs.uvs = generated_uv_positions
            .iter()
            .map(|p| [p[0] * scale, p[1] * scale])
            .collect();
    }
    Ok(inputs)
}

fn vertex_inputs(
    positions: &[[f32; 3]],
    indices: &[u32],
    uv_words: Option<&[[u16; 2]]>,
    depth_bytes: Option<&[u8]>,
    lut: &[f32; 256],
) -> Result<WrathMeshInputs, String> {
    let n = positions.len();
    if uv_words.is_some_and(|v| v.len() != n) || depth_bytes.is_some_and(|v| v.len() != n) {
        return Err("liquid vertex payload length mismatch".into());
    }
    if !positions.iter().flatten().all(|v| v.is_finite()) {
        return Err("nonfinite liquid position".into());
    }
    if !indices.len().is_multiple_of(3) || indices.iter().any(|&i| i as usize >= n) {
        return Err("invalid liquid triangle indices".into());
    }
    if !lut.iter().all(|v| v.is_finite()) {
        return Err("nonfinite liquid depth LUT".into());
    }
    let xy_scale = f32::from_bits(0x3d75_c290);
    let uvs = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            uv_words.map_or([p[0] * xy_scale, p[1] * xy_scale], |words| {
                words[i].map(|v| f32::from(v) * (3.0 / 256.0))
            })
        })
        .collect();
    let depths = (0..n)
        .map(|i| depth_bytes.map_or(0., |bytes| lut[usize::from(bytes[i])]))
        .collect();
    Ok(WrathMeshInputs {
        positions: positions.to_vec(),
        uvs,
        depths,
        indices: indices.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_and_thirty_ordered_frames() {
        assert_eq!(texture_filenames("a\\b.blp").unwrap(), ["a\\b.blp"]);
        let frames = texture_filenames("水\\frame%d.blp").unwrap();
        assert_eq!(frames.len(), 30);
        assert_eq!(frames[0], "水\\frame1.blp");
        assert_eq!(frames[9], "水\\frame10.blp");
        assert_eq!(frames[29], "水\\frame30.blp");
    }

    #[test]
    fn unsafe_filename_syntax_and_bounds_fail() {
        for s in ["", "%", "%%", "%s", "%02d", "%d%d", "%d%%", "a\0b", "é%é"] {
            assert!(texture_filenames(s).is_err(), "{s:?}");
        }
        let limit = "a".repeat(MAX_TEXTURE_FILENAME_BYTES);
        assert!(texture_filenames(&limit).is_ok());
        assert!(texture_filenames(&(limit.clone() + "a")).is_err());
        let patterned = "a".repeat(MAX_TEXTURE_FILENAME_BYTES - 2) + "%d";
        assert_eq!(
            texture_filenames(&patterned).unwrap()[29].len(),
            limit.len()
        );
        assert!(texture_filenames(&(patterned + "a")).is_err());
    }

    #[test]
    fn native_tables_and_selector_gate_are_independent() {
        let ramp = depth_lut(0, 0).unwrap();
        let normalized = depth_lut(2, 1).unwrap();
        assert_eq!(ramp[0], 0.);
        assert!((ramp[1] - 1. / 42.).abs() < 1e-7);
        assert!(ramp[41] < 1.);
        assert_eq!(ramp[42], 1.);
        assert_eq!(ramp[255], 1.);
        assert_eq!(normalized[0], 0.);
        assert_eq!(normalized[255], 1.);
        for (i, value) in normalized.iter().enumerate() {
            assert!((value - i as f32 / 255.).abs() < 1e-7);
        }
        for word in 0..=1 {
            assert_eq!(depth_lut(0, word), depth_lut(2, word));
            for selector in [1, 3, u32::MAX] {
                assert_eq!(depth_lut(selector, word).unwrap(), [0.; 256]);
            }
        }
        for selector in [0, 1, 2, u32::MAX] {
            assert!(depth_lut(selector, 2).is_err());
            assert!(depth_lut(selector, u32::MAX).is_err());
        }
    }

    #[test]
    fn raw_positions_generated_uv_and_byte_indexed_depth() {
        let positions = [[-100., 25., -7.], [1., 2., 3.], [4., 5., 6.]];
        let bytes = [0, 42, 255];
        let lut = depth_lut(0, 0).unwrap();
        let mesh = vertex_inputs(&positions, &[0, 1, 2], None, Some(&bytes), &lut).unwrap();
        assert_eq!(mesh.positions, positions);
        assert_eq!(mesh.indices, [0, 1, 2]);
        assert_eq!(
            mesh.uvs[0],
            [
                -100. * f32::from_bits(0x3d75_c290),
                25. * f32::from_bits(0x3d75_c290)
            ]
        );
        assert_eq!(mesh.depths, [0., 1., 1.]);
        let zero = depth_lut(1, 0).unwrap();
        assert_eq!(
            vertex_inputs(&positions, &[], None, Some(&bytes), &zero)
                .unwrap()
                .depths,
            [0.; 3]
        );
    }

    #[test]
    fn authored_unsigned_uvs_do_not_invent_depth_or_material() {
        let words = [[65535, 32768], [256, 0], [1, 2]];
        let positions = [[0.; 3]; 3];
        for selector in [0, 1, 2, 99] {
            let lut = depth_lut(selector, 1).unwrap();
            let mesh = vertex_inputs(&positions, &[], Some(&words), None, &lut).unwrap();
            assert_eq!(mesh.uvs[0], [65535. * 3. / 256., 384.]);
            assert_eq!(mesh.uvs[1], [3., 0.]);
            assert_eq!(mesh.depths, [0.; 3]);
        }
    }

    #[test]
    fn malformed_vertex_arrays_fail_closed() {
        let good = [[0.; 3]; 3];
        let lut = depth_lut(0, 0).unwrap();
        assert!(vertex_inputs(&good, &[0], None, None, &lut).is_err());
        assert!(vertex_inputs(&good, &[0, 1, 3], None, None, &lut).is_err());
        assert!(vertex_inputs(&good, &[], Some(&[[0; 2]]), None, &lut).is_err());
        assert!(vertex_inputs(&good, &[], None, Some(&[0]), &lut).is_err());
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad = good;
            bad[1][2] = value;
            assert!(vertex_inputs(&bad, &[], None, None, &lut).is_err());
        }
        let mut bad_lut = lut;
        bad_lut[0] = f32::NAN;
        assert!(vertex_inputs(&good, &[], None, None, &bad_lut).is_err());
    }
}
