//! `Movie.dbc` (build 12340: id, file name, volume; AzerothCore `MovieEntryfmt`), the table
//! `SMSG_TRIGGER_MOVIE` names a row of.

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};
use std::collections::HashMap;

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

/// One movie: its file name (extensionless in the shipped rows; empty for some) and volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movie {
    pub file: String,
    pub volume: u32,
}

#[derive(Debug, Default)]
pub struct Movies(HashMap<u32, Movie>);

impl Movies {
    pub fn get(&self, id: u32) -> Option<&Movie> {
        self.0.get(&id)
    }
}

impl From<HashMap<u32, Movie>> for Movies {
    fn from(rows: HashMap<u32, Movie>) -> Self {
        Self(rows)
    }
}

fn movies(bytes: &[u8]) -> Result<Movies> {
    let mut schema = Schema::new("Movie");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("Filename", FieldType::String));
    schema.add_field(SchemaField::new("Volume", FieldType::UInt32));
    let rs = parse(bytes, schema, "Movie")?;
    rs.records()
        .iter()
        .map(|r| {
            Ok((
                u32_at(r, 0).context("Movie id")?,
                Movie {
                    file: str_at(&rs, r, 1).unwrap_or_default(),
                    volume: u32_at(r, 2).context("Movie volume")?,
                },
            ))
        })
        .collect::<Result<HashMap<_, _>>>()
        .map(Movies)
}

pub fn load_movies(chain: &mut Chain) -> Result<Movies> {
    movies(
        &chain
            .read_file("DBFilesClient\\Movie.dbc")
            .context("reading Movie")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_keeps_its_file_and_volume_and_an_empty_name_reads_empty() {
        let strings = b"\0Interface\\Cinematics\\WOW_Intro\0";
        let mut b = b"WDBC".to_vec();
        for n in [2u32, 3, 12, strings.len() as u32] {
            b.extend(n.to_le_bytes());
        }
        for v in [2u32, 1, 100, 14, 0, 100] {
            b.extend(v.to_le_bytes());
        }
        b.extend(strings);
        let m = movies(&b).unwrap();
        assert_eq!(
            m.get(2),
            Some(&Movie {
                file: "Interface\\Cinematics\\WOW_Intro".into(),
                volume: 100
            })
        );
        assert_eq!(m.get(14).map(|m| m.file.as_str()), Some(""));
        assert!(m.get(3).is_none());
    }
}
