//! `SpellDisplay`'s empty row and its aura-vision mask.

use super::SpellDisplay;

/// Written out so `equipped_item_class` defaults to -1, no requirement: 0 is a real class.
impl Default for SpellDisplay {
    fn default() -> Self {
        SpellDisplay {
            id: 0,
            name: String::new(),
            rank: None,
            icon: None,
            visual: 0,
            speed: 0.0,
            attributes: 0,
            school: 0,
            mechanic: 0,
            attributes_ex: 0,
            attributes_ex2: 0,
            modal_next_spell: 0,
            attributes_ex3: 0,
            attributes_ex4: 0,
            attributes_ex5: 0,
            attributes_ex7: 0,
            attributes_ex6: 0,
            school_mask: 0,
            spell_family: 0,
            spell_family_flags: 0,
            spell_family_flags_2: 0,
            effect_spell_class_mask: [[0; 3]; 3],
            prevention_type: 0,
            passive: false,
            cast_ui: 0,
            effects: [0, 0, 0],
            open_lock: None,
            base_level: 0,
            max_level: 0,
            spell_level: 0,
            dispel: 0,
            category: 0,
            category_wildcard: false,
            category_weapon_speed: false,
            recovery_ms: 0,
            interrupt_flags: 0,
            aura_interrupt_flags: 0,
            channel_interrupt_flags: 0,
            category_recovery_ms: 0,
            start_recovery_category: 0,
            start_recovery_ms: 0,
            power_type: 0,
            mana_cost: 0,
            mana_cost_pct: 0,
            mana_cost_per_level: 0,
            mana_per_second: 0,
            rune_cost_id: 0,
            power_display_id: 0,
            range_index: 0,
            targets: 0,
            target_creature_type: 0,
            implicit_target_a1: 0,
            stances: 0,
            stances_not: 0,
            caster_aura_state: 0,
            target_aura_state: 0,
            totems: [0; 2],
            reagents: [(0, 0); 8],
            equipped_item_class: -1,
            equipped_item_subclass_mask: 0,
            equipped_item_inventory_type_mask: 0,
            requires_spell_focus: 0,
            shapeshift_form: None,
            stance_bar_order: 0,
            active_icon: None,
            active_icon_id: 0,
            description: None,
            aura_description: None,
            duration_index: 0,
            casting_time_index: 0,
            proc_flags: 0,
            proc_chance: 0,
            proc_charges: 0,
            stack_amount: 0,
            max_target_level: 0,
            max_affected_targets: 0,
            effect_base_points: [0; 3],
            effect_die_sides: [0; 3],
            effect_base_dice: [0; 3],
            effect_dice_per_level: [0; 3],
            effect_real_points_per_level: [0.0; 3],
            effect_amplitude: [0; 3],
            effect_apply_aura: [0; 3],
            effect_implicit_target_a: [0; 3],
            effect_implicit_target_b: [0; 3],
            effect_mechanic: [0; 3],
            effect_radius_index: [0; 3],
            effect_chain_targets: [0; 3],
            effect_multiple_value: [0.0; 3],
            damage_multiplier: [0.0; 3],
            effect_points_per_combo_point: [0.0; 3],
            effect_trigger_spell: [0; 3],
            effect_item_type: [0; 3],
            effect_misc_value: [0; 3],
            effect_misc_value_b: [0; 3],
            required_aura_vision: 0,
        }
    }
}

impl SpellDisplay {
    /// The aura-vision bits a viewer needs before this aura's visuals are armed on anyone
    /// (`0x7fe3e0`, gated at `0x724820` against `PLAYER_FIELD_BYTES2` byte 3): bit
    /// `RequiredAuraVision − 1`, `0x20` for a stealth-detect effect (aura 17) and `0x40` for an
    /// invisibility-detect one (aura 19) of type 0 or 10. Zero: always armed.
    pub fn aura_vision_mask(&self) -> u32 {
        let mut mask = match self.required_aura_vision {
            0 => 0,
            v => 1 << ((v - 1) & 31),
        };
        for (aura, misc) in self.effect_apply_aura.iter().zip(self.effect_misc_value) {
            match aura {
                17 => mask |= 0x20,
                19 if matches!(misc, 0 | 10) => mask |= 0x40,
                _ => {}
            }
        }
        mask
    }
}
