//! Build 12340's GM survey tables: `GMSurveyCurrentSurvey.dbc` (the survey per client locale id),
//! `GMSurveySurveys.dbc` (ten question ids), `GMSurveyQuestions.dbc` and `GMSurveyAnswers.dbc`
//! (`GMSurveyQuestion`/`GMSurveyAnswer`, `0x5ac530`/`0x5ac5b0`). The localized text uses the
//! selected archive slot only.

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::lfg::{localized, table};
use crate::{Chain, DataProfile};

/// One `GMSurveyAnswers` row: its 0-based order within the question, the question, the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmSurveyAnswer {
    pub id: u32,
    pub sort: u32,
    pub question: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GmSurveyCatalog {
    /// Client locale id → survey id.
    pub current: HashMap<u32, u32>,
    /// Survey id → its ten question ids, 0 for an empty slot.
    pub surveys: HashMap<u32, [u32; 10]>,
    pub questions: HashMap<u32, String>,
    pub answers: Vec<GmSurveyAnswer>,
}

pub fn parse_gm_survey(
    current: &[u8],
    surveys: &[u8],
    questions: &[u8],
    answers: &[u8],
    locale: &str,
) -> Result<GmSurveyCatalog> {
    let slot = crate::cfg_categories::locale_slot(locale)?;
    let c = table::<2>(current).context("GMSurveyCurrentSurvey")?;
    let s = table::<11>(surveys).context("GMSurveySurveys")?;
    let q = table::<18>(questions).context("GMSurveyQuestions")?;
    let a = table::<20>(answers).context("GMSurveyAnswers")?;
    Ok(GmSurveyCatalog {
        current: c.rows.iter().map(|r| (r[0], r[1])).collect(),
        surveys: s
            .rows
            .iter()
            .map(|r| (r[0], std::array::from_fn(|i| r[1 + i])))
            .collect(),
        questions: q
            .rows
            .iter()
            .map(|r| Ok((r[0], localized(q.strings, &r[1..17], slot)?)))
            .collect::<Result<_>>()?,
        answers: a
            .rows
            .iter()
            .map(|r| {
                Ok(GmSurveyAnswer {
                    id: r[0],
                    sort: r[1],
                    question: r[2],
                    text: localized(a.strings, &r[3..19], slot)?,
                })
            })
            .collect::<Result<_>>()?,
    })
}

pub fn load_gm_survey(chain: &mut Chain) -> Result<GmSurveyCatalog> {
    let DataProfile::Wrath { locale } = chain.profile() else {
        anyhow::bail!("the GM survey requires Wrath data");
    };
    let locale = locale.clone();
    let mut read = |name| {
        chain
            .read_file(&format!("DBFilesClient\\{name}.dbc"))
            .with_context(|| format!("reading {name}"))
    };
    parse_gm_survey(
        &read("GMSurveyCurrentSurvey")?,
        &read("GMSurveySurveys")?,
        &read("GMSurveyQuestions")?,
        &read("GMSurveyAnswers")?,
        &locale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_survey_resolves_its_questions_and_answers() {
        let strings = b"\0Was the GM helpful?\0Very\0";
        let mut survey = [0u32; 11];
        survey[0] = 9;
        survey[1] = 35;
        let mut question = [0u32; 18];
        question[0] = 35;
        question[1] = 1;
        let mut answer = [0u32; 20];
        answer[0] = 104;
        answer[1] = 0;
        answer[2] = 35;
        answer[3] = 21;
        let cat = parse_gm_survey(
            &wdbc(&[[0, 9]], strings),
            &wdbc(&[survey], strings),
            &wdbc(&[question], strings),
            &wdbc(&[answer], strings),
            "enUS",
        )
        .unwrap();
        assert_eq!(cat.current.get(&0), Some(&9));
        assert_eq!(cat.surveys[&9][0], 35);
        assert_eq!(cat.questions[&35], "Was the GM helpful?");
        assert_eq!(cat.answers[0].text, "Very");
    }
}
