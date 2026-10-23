//! Skin submesh and batch records, shared by the external `.skin` reader.

/// One skin submesh: its triangle range and geoset id.
pub struct SkinSection {
    /// `skinSectionId`, the geoset id `group*100 + variant` that character geoset selection
    /// toggles; 0 on a single-geoset model.
    pub id: u16,
    pub triangle_start: u16,
    pub triangle_count: u16,
}

/// One skin draw batch (section + material + texture-combo references).
pub struct SkinBatch {
    /// Batch flags (texUnit `+0x00`), raw; the renderer ignores them.
    pub flags: u16,
    /// Shader or render-order selector (texUnit `+0x02`), raw; the renderer ignores it.
    pub shader_id: u16,
    pub skin_section_index: u16,
    pub texture_combo_index: u16,
    /// Index into [`crate::M2Model::texture_unit_lookup`], the stage's coordinate source (texUnit
    /// `+0x12`, `0x70b8a4`).
    pub texture_coord_combo_index: u16,
    pub material_index: u16,
    /// Index into [`crate::M2Model::color_alpha_tracks`], `0xffff` for none (texUnit `+0x08`).
    pub color_index: u16,
    /// Index into [`crate::M2Model::transparency_lookup`] (texUnit `+0x14`).
    pub weight_combo_index: u16,
    /// Texture count (texUnit `+0x0e`); at 0 the transparency factor is not applied.
    pub texture_count: u16,
    /// Index into [`crate::M2Model::texture_transform_lookup`] (texUnit `+0x16`, `0x70b897`).
    pub texture_transform_combo_index: u16,
}
