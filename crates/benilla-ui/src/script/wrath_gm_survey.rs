//! Build 12340's GM survey (`Blizzard_GMSurveyUI`): `GMSurveyQuestion` (`0x5ac750`),
//! `GMSurveyAnswer` (`0x5ac7d0`), `GMSurveyAnswerSubmit` (`0x5ac390` → `0x5ac260`),
//! `GMSurveyCommentSubmit` (`0x5ac480`) and `GMSurveySubmit` (`0x5ad230`), over the survey App
//! publishes when a GM's ticket status offers one (`GMSURVEY_DISPLAY`). Evidence: one headless
//! Ghidra round (`/tmp/azb-app-mig/ghidra/survey*.log`).
//!
//! The survey is `GMSurveyCurrentSurvey`'s row for the client's locale; its ten slots name
//! `GMSurveyQuestions` rows, and answer `n` of a question is the `GMSurveyAnswers` row with that
//! question and sort index `n - 1`. The answers stay client-side until the submit.
//!
//! Labelled: `GMSurveySubmit`'s body (`0x5ad230`) is not decompiled; it sends every slot that names
//! a question with its rank and comment, then clears them.

use std::sync::Arc;

use mlua::{Lua, MultiValue, Value};

use super::{Model, UiScript};

/// One slot of the survey.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathGmSurveyQuestion {
    /// The `GMSurveyQuestions` id, 0 for an empty slot.
    pub id: u32,
    pub text: String,
    /// `(sort index, text)`, the answers to this question.
    pub answers: Vec<(u32, String)>,
}

/// The offered survey: its id and ten slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathGmSurvey {
    pub survey: u32,
    pub questions: Vec<WrathGmSurveyQuestion>,
}

/// `GMSurveySubmit`'s request: the survey, each named slot's `(question, rank, comment)` and the
/// overall comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrathGmSurveySubmit {
    pub survey: u32,
    pub answers: Vec<(u32, u8, String)>,
    pub comment: String,
}

#[derive(Default)]
pub(crate) struct State {
    survey: Arc<WrathGmSurvey>,
    /// `0xc1e8c4` ranks and `0xc1e8d8` comments, by slot.
    ranks: [u8; 10],
    comments: [Option<String>; 10],
    comment: Option<String>,
    submits: Vec<WrathGmSurveySubmit>,
}

impl UiScript {
    /// Publish the offered survey; App then fires `GMSURVEY_DISPLAY`.
    pub fn set_wrath_gm_survey(&mut self, survey: Arc<WrathGmSurvey>) {
        self.model_mut().wrath_gm_survey.survey = survey;
    }

    /// The `GMSurveySubmit` calls since the last drain.
    pub fn take_wrath_gm_survey_submits(&mut self) -> Vec<WrathGmSurveySubmit> {
        std::mem::take(&mut self.model_mut().wrath_gm_survey.submits)
    }
}

fn number(lua: &Lua, args: &MultiValue, i: usize) -> Option<i64> {
    args.get(i)
        .and_then(|v| lua.coerce_number(v.clone()).ok().flatten())
        .map(|n| n.round() as i64)
}

fn text(lua: &Lua, args: &MultiValue, i: usize) -> Option<String> {
    match args.get(i)? {
        v @ (Value::String(_) | Value::Integer(_) | Value::Number(_)) => lua
            .coerce_string(v.clone())
            .ok()
            .flatten()
            .map(|s| s.to_string_lossy()),
        _ => None,
    }
}

fn slot(survey: &WrathGmSurvey, index: i64) -> Option<&WrathGmSurveyQuestion> {
    let k = usize::try_from(index - 1).ok().filter(|&k| k < 10)?;
    survey.questions.get(k).filter(|q| q.id != 0)
}

fn opt(lua: &Lua, s: Option<String>) -> mlua::Result<Value> {
    Ok(match s {
        Some(s) => Value::String(lua.create_string(&s)?),
        None => Value::Nil,
    })
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "GMSurveyQuestion",
        lua.create_function(|lua, args: MultiValue| {
            let Some(index) = number(lua, &args, 0) else {
                return Err(mlua::Error::runtime("Usage: GMSurveyGetQuestion(index)"));
            };
            let q = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                slot(&model.wrath_gm_survey.survey, index).map(|q| q.text.clone())
            };
            opt(lua, q)
        })?,
    )?;
    g.set(
        "GMSurveyAnswer",
        lua.create_function(|lua, args: MultiValue| {
            let (Some(q), Some(a)) = (number(lua, &args, 0), number(lua, &args, 1)) else {
                return Err(mlua::Error::runtime(
                    "Usage: GMSurveyGetAnswer(questionIndex, answerIndex)",
                ));
            };
            let answer = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                slot(&model.wrath_gm_survey.survey, q).and_then(|q| {
                    q.answers
                        .iter()
                        .find(|(sort, _)| i64::from(*sort) == a - 1)
                        .map(|(_, t)| t.clone())
                })
            };
            opt(lua, answer)
        })?,
    )?;
    g.set(
        "GMSurveyAnswerSubmit",
        lua.create_function(|lua, args: MultiValue| {
            let (Some(q), Some(rank), Some(comment)) = (
                number(lua, &args, 0),
                number(lua, &args, 1),
                text(lua, &args, 2),
            ) else {
                return Err(mlua::Error::runtime(
                    "Usage: GMSurveyAnswerSubmit(question, rank, comment)",
                ));
            };
            let Some(k) = usize::try_from(q - 1).ok().filter(|&k| k < 10) else {
                return Err(mlua::Error::runtime(
                    "GMSurveyAnswerSubmit: Questions limited from 1 to 10",
                ));
            };
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let state = &mut model.wrath_gm_survey;
            state.ranks[k] = rank as u8;
            state.comments[k] = Some(comment).filter(|c| !c.is_empty());
            Ok(())
        })?,
    )?;
    g.set(
        "GMSurveyCommentSubmit",
        lua.create_function(|lua, args: MultiValue| {
            let Some(comment) = text(lua, &args, 0) else {
                return Err(mlua::Error::runtime(
                    "Usage: GMSurveyCommentSubmit(comment)",
                ));
            };
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .wrath_gm_survey
                .comment = Some(comment).filter(|c| !c.is_empty());
            Ok(())
        })?,
    )?;
    g.set(
        "GMSurveySubmit",
        lua.create_function(|lua, ()| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let state = &mut model.wrath_gm_survey;
            let survey = state.survey.clone();
            let answers = survey
                .questions
                .iter()
                .enumerate()
                .take(10)
                .filter(|(_, q)| q.id != 0)
                .map(|(k, q)| {
                    (
                        q.id,
                        state.ranks[k],
                        state.comments[k].clone().unwrap_or_default(),
                    )
                })
                .collect();
            let request = WrathGmSurveySubmit {
                survey: survey.survey,
                answers,
                comment: state.comment.take().unwrap_or_default(),
            };
            state.ranks = [0; 10];
            state.comments = Default::default();
            state.submits.push(request);
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ScriptDialect;

    #[test]
    fn the_survey_reads_questions_answers_and_submits_them() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let mut questions = vec![WrathGmSurveyQuestion::default(); 10];
        questions[0] = WrathGmSurveyQuestion {
            id: 35,
            text: "Was the GM helpful?".into(),
            answers: vec![(0, "Very".into()), (1, "Somewhat".into())],
        };
        s.set_wrath_gm_survey(Arc::new(WrathGmSurvey {
            survey: 9,
            questions,
        }));
        assert_eq!(
            s.eval::<String>("return GMSurveyQuestion(1)").unwrap(),
            "Was the GM helpful?"
        );
        assert!(s.eval::<bool>("return GMSurveyQuestion(2) == nil").unwrap());
        assert_eq!(
            s.eval::<String>("return GMSurveyAnswer(1, 2)").unwrap(),
            "Somewhat"
        );
        assert!(s
            .eval::<bool>("return GMSurveyAnswer(1, 3) == nil")
            .unwrap());
        assert!(s.run("GMSurveyAnswerSubmit(11, 1, '')").is_err());
        s.run(
            "GMSurveyAnswerSubmit(1, 4, 'fast') GMSurveyCommentSubmit('thanks') GMSurveySubmit()",
        )
        .unwrap();
        assert_eq!(
            s.take_wrath_gm_survey_submits(),
            [WrathGmSurveySubmit {
                survey: 9,
                answers: vec![(35, 4, "fast".into())],
                comment: "thanks".into(),
            }]
        );
    }
}
