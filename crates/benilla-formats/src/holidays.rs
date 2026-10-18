//! Build-12340 `Holidays.dbc` with its `HolidayNames.dbc` and `HolidayDescriptions.dbc` text, for
//! the calendar's holiday rows (`CalendarGetHolidayInfo` reads the name `0x5b9430`, the
//! description `0x5b9490` and the row's texture). Every word is kept; the localized text uses the
//! selected archive slot only.

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::lfg::{localized, table, text};
use crate::{Chain, DataProfile};

/// `MAX_HOLIDAY_DURATIONS`, `MAX_HOLIDAY_DATES`, `MAX_HOLIDAY_FLAGS`.
pub const DURATIONS: usize = 10;
pub const DATES: usize = 26;
pub const FLAGS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holiday {
    raw: [u32; 55],
    /// From `HolidayNames.dbc`; empty when the row names none.
    pub name: String,
    /// From `HolidayDescriptions.dbc`; empty when the row names none.
    pub description: String,
    pub texture: String,
}

impl Holiday {
    pub fn raw_columns(&self) -> &[u32; 55] {
        &self.raw
    }
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    /// Hours per stage.
    pub fn durations(&self) -> &[u32] {
        &self.raw[1..1 + DURATIONS]
    }
    /// Packed start times, 0 past the last.
    pub fn dates(&self) -> &[u32] {
        &self.raw[11..11 + DATES]
    }
    pub fn region(&self) -> u32 {
        self.raw[37]
    }
    pub fn looping(&self) -> u32 {
        self.raw[38]
    }
    pub fn calendar_flags(&self) -> &[u32] {
        &self.raw[39..39 + FLAGS]
    }
    pub fn name_id(&self) -> u32 {
        self.raw[49]
    }
    pub fn description_id(&self) -> u32 {
        self.raw[50]
    }
    pub fn priority(&self) -> u32 {
        self.raw[52]
    }
    pub fn filter_type(&self) -> i32 {
        self.raw[53] as i32
    }
    pub fn flags(&self) -> u32 {
        self.raw[54]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HolidayCatalog(Vec<Holiday>);

impl HolidayCatalog {
    pub fn rows(&self) -> &[Holiday] {
        &self.0
    }
    pub fn get(&self, id: u32) -> Option<&Holiday> {
        self.0.iter().find(|h| h.id() == id)
    }
}

/// An `ID, text[16], mask` table as id → the slot's text.
fn names(bytes: &[u8], slot: usize) -> Result<HashMap<u32, String>> {
    let t = table::<18>(bytes)?;
    t.rows
        .iter()
        .map(|raw| Ok((raw[0], localized(t.strings, &raw[1..17], slot)?)))
        .collect()
}

pub fn parse_holidays(
    holidays: &[u8],
    names_dbc: &[u8],
    descriptions_dbc: &[u8],
    locale: &str,
) -> Result<HolidayCatalog> {
    let slot = crate::cfg_categories::locale_slot(locale)?;
    let names_by_id = names(names_dbc, slot).context("HolidayNames")?;
    let descriptions = names(descriptions_dbc, slot).context("HolidayDescriptions")?;
    let t = table::<55>(holidays)?;
    let mut rows = Vec::with_capacity(t.rows.len());
    for raw in t.rows {
        rows.push(Holiday {
            name: names_by_id.get(&raw[49]).cloned().unwrap_or_default(),
            description: descriptions.get(&raw[50]).cloned().unwrap_or_default(),
            texture: text(t.strings, raw[51])?,
            raw,
        });
    }
    Ok(HolidayCatalog(rows))
}

/// Use the caller's boot-resolved chain; every table is required.
pub fn load_holidays(chain: &mut Chain) -> Result<HolidayCatalog> {
    let DataProfile::Wrath { locale } = chain.profile() else {
        anyhow::bail!("holidays require Wrath data");
    };
    let locale = locale.clone();
    let mut read = |name| {
        chain
            .read_file(&format!("DBFilesClient\\{name}.dbc"))
            .with_context(|| format!("reading {name}"))
    };
    parse_holidays(
        &read("Holidays")?,
        &read("HolidayNames")?,
        &read("HolidayDescriptions")?,
        &locale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WDBC of `rows`, each `N` words, over `strings`.
    fn wdbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
        let mut b = b"WDBC".to_vec();
        for w in [
            rows.len() as u32,
            N as u32,
            (N * 4) as u32,
            strings.len() as u32,
        ] {
            b.extend(w.to_le_bytes());
        }
        for row in rows {
            for w in row {
                b.extend(w.to_le_bytes());
            }
        }
        b.extend(strings);
        b
    }

    #[test]
    fn a_holiday_takes_its_name_description_and_texture() {
        let strings = b"\0Fireworks\0Calendar_Fireworks\0The sky lights up.\0";
        let mut row = [0u32; 55];
        row[0] = 62;
        row[1] = 18;
        row[11] = 526_449_024;
        row[49] = 12;
        row[50] = 11;
        row[51] = 11;
        row[53] = u32::MAX;
        let mut name = [0u32; 18];
        name[0] = 12;
        name[1] = 1;
        let mut desc = [0u32; 18];
        desc[0] = 11;
        desc[1] = 30;
        let catalog = parse_holidays(
            &wdbc(&[row], strings),
            &wdbc(&[name], strings),
            &wdbc(&[desc], strings),
            "enUS",
        )
        .unwrap();
        let h = catalog.get(62).unwrap();
        assert_eq!(
            (h.name.as_str(), h.description.as_str(), h.texture.as_str()),
            ("Fireworks", "The sky lights up.", "Calendar_Fireworks")
        );
        assert_eq!(
            (h.durations()[0], h.dates()[0], h.filter_type()),
            (18, 526_449_024, -1)
        );
    }
}
