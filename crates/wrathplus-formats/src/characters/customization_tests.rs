use super::*;

fn char_sections_fixture(profile: &DataProfile, flags: u32) -> wrathplus_dbc::RecordSet {
    let mut fields = [0u32; 10];
    fields[0] = 0x0102_0304;
    fields[1] = 1;
    fields[2] = 0;
    let columns = crate::characters::char_section_columns(profile);
    fields[columns.section_type] = 3;
    fields[columns.variation] = 71;
    fields[columns.color] = 9;
    fields[columns.flags] = flags;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.push(0);
    parse(
        &bytes,
        crate::characters::char_sections_schema(profile),
        "synthetic CharSections",
    )
    .expect("parse synthetic CharSections")
}

#[test]
fn wrath_char_sections_require_the_player_flag() {
    let wrath = DataProfile::Wrath {
        locale: "enUS".to_string(),
    };
    let sentinel = (1, 0, 3, 71, 9);

    assert!(
        !available_sections_from_records(&char_sections_fixture(&wrath, 0), &wrath)
            .contains(&sentinel)
    );
    assert!(
        available_sections_from_records(&char_sections_fixture(&wrath, 1), &wrath)
            .contains(&sentinel)
    );
}

/// Build 12340's `ChrRaces`: clientFileString is column 11 (`Scourge`, `BloodElf`), the name
/// 14, and the customization tokens 65/66/67, as `GetHairCustomization` (`+0x4c`) and
/// `GetFacialHairCustomization` (`+0x44` by sex) read them.
#[test]
fn chr_races_reads_the_12340_file_strings_names_and_tokens() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("chain");
    let c = CharCreateCatalog::load(&mut chain).expect("catalog");
    assert_eq!(c.race_file(5), Some("Scourge"));
    assert_eq!(c.race_file(10), Some("BloodElf"));
    assert_eq!(c.race_name(4, 0), Some("Night Elf"));
    assert_eq!(c.race_name(5, 1), Some("Undead"));
    assert_eq!(c.hair_customization(6), Some("HORNS"));
    assert_eq!(c.facial_hair_customization(8, 0), Some("TUSKS"));
    assert_eq!(c.facial_hair_customization(1, 1), Some("PIERCINGS"));
    assert_eq!(c.facial_hair_customization(1, 0), Some("NORMAL"));
}
