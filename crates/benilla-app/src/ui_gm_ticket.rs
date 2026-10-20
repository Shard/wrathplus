//! The GM ticket flow behind the stock Help window: the events it hears and the packets it sends.
//! The client holds no ticket: `SMSG_GMTICKET_GETTICKET` is the only truth, and every answer fires
//! `UPDATE_TICKET`, an unchanged one too, as the window re-polls `GetGMTicket()` every 10 minutes.
//! 1.12 has no response channel, so vmangos appends a GM's reply to the ticket text
//! (`GMTicketMgr.cpp:124-136`).
//!
//! Every send is fire-and-forget and never retried: vmangos answers some refusals with silence
//! (`GMTicketHandler.cpp:91,106-113`), and a third `CMSG_GMTICKET_UPDATETEXT` in one world tick
//! trips its anti-flood kick (`WorldSession.cpp:1316-1342`).
//!
//! Build 12340's GM survey: `SMSG_GM_TICKET_STATUS_UPDATE` status 3 publishes the locale's current
//! survey (`GMSurveyCurrentSurvey` → `GMSurveySurveys` → questions and answers) and fires
//! `GMSURVEY_DISPLAY`; the window's `GMSurveySubmit` sends `CMSG_GMSURVEY_SUBMIT`.

use benilla_ui::script::{GmTicketIntent, GmTicketWrite, ScriptValue, UiScript};
use bevy::prelude::*;

use benilla_assets::coords::bevy_to_wow;

use crate::net::{ClientCommand, NetCommands};
use crate::player::Player;
use crate::ui_script::{UiFeed, UiInput, VmMemo};

/// Spell 7355 "Stuck", what `Stuck()` casts: the only spell with `SPELL_EFFECT_STUCK` (84), which
/// moves the player to their last safe position (vmangos `SpellEffects.cpp:4697`).
const SPELL_STUCK: u32 = 7355;

/// What the last `SMSG_GMTICKET_GETTICKET` said, and how many have arrived: the feed fires on the
/// count, so the window's poll is answered even when nothing changed.
#[derive(Resource, Default)]
pub(crate) struct GmTicketState {
    /// The open ticket; `None` is the ordinary "no ticket" answer.
    ticket: Option<Box<benilla_protocol::messages::GmTicket>>,
    /// One per `SMSG_GMTICKET_GETTICKET`, wrapping.
    answers: u32,
    /// Re-asks the engine owes ([`Self::note_write_landed`]), sent by [`drain_gm_ticket`].
    engine_reasks: u32,
    /// The last queue status, with its own answer count. Signed as in the reference, which copies
    /// the wire dword verbatim (`0x418e95`), pushes it unmodified (`0x5e467b`) and loads it signed
    /// for Lua (`0x704fa6`), so `HelpFrame`'s `arg1 == -1` arm fires on `0xFFFFFFFF`.
    queue_status: i32,
    queue_answers: u32,
    /// Status update 3, a GM's survey offer, counted: each fires `GMSURVEY_DISPLAY`.
    survey_offers: u32,
}

/// Build 12340's GM survey tables.
#[derive(Resource)]
pub(crate) struct GmSurveyTables {
    catalog: benilla_formats::gm_survey::GmSurveyCatalog,
    /// The client's locale id, `GMSurveyCurrentSurvey`'s key.
    locale_id: u32,
}

impl GmSurveyTables {
    /// The current survey (`0x5ac530`'s first hop) with its questions and answers.
    fn survey(&self) -> benilla_ui::script::WrathGmSurvey {
        let c = &self.catalog;
        let survey = c.current.get(&self.locale_id).copied().unwrap_or(0);
        let slots = c.surveys.get(&survey).copied().unwrap_or_default();
        benilla_ui::script::WrathGmSurvey {
            survey,
            questions: slots
                .iter()
                .map(|&id| benilla_ui::script::WrathGmSurveyQuestion {
                    id,
                    text: c.questions.get(&id).cloned().unwrap_or_default(),
                    answers: c
                        .answers
                        .iter()
                        .filter(|a| id != 0 && a.question == id)
                        .map(|a| (a.sort, a.text.clone()))
                        .collect(),
                })
                .collect(),
        }
    }
}

/// The client locale ids `GMSurveyCurrentSurvey` keys on. Labelled: the order is the build's
/// locale index (`0xc5de9c`), enGB sharing enUS's.
fn locale_id(locale: &str) -> u32 {
    match locale {
        "koKR" => 1,
        "frFR" => 2,
        "deDE" => 3,
        "zhCN" => 4,
        "zhTW" => 5,
        "esES" => 6,
        "esMX" => 7,
        "ruRU" => 8,
        _ => 0,
    }
}

impl GmTicketState {
    /// `SMSG_GMTICKET_GETTICKET`: replace the ticket and count the answer, an empty one too.
    pub(crate) fn answer(&mut self, ticket: Option<Box<benilla_protocol::messages::GmTicket>>) {
        self.ticket = ticket;
        self.answers = self.answers.wrapping_add(1);
    }

    /// A write landed: re-ask for the ticket, as the reference engine does, once per push and
    /// unguarded, on create-ok (2) and update-ok (4) at `0x5e4479` and on status update 1 at
    /// `0x5e7932`. Without it a filed ticket stays unseen until the 10-minute poll.
    pub(crate) fn note_write_landed(&mut self) {
        self.engine_reasks = self.engine_reasks.saturating_add(1);
    }

    fn take_reasks(&mut self) -> u32 {
        std::mem::take(&mut self.engine_reasks)
    }

    /// `SMSG_GMTICKETSYSTEMSTATUS`: the queue's state, counted the same way.
    pub(crate) fn answer_queue(&mut self, status: i32) {
        self.queue_status = status;
        self.queue_answers = self.queue_answers.wrapping_add(1);
    }

    /// Session end: a reconnect must not show the previous character's ticket.
    pub(crate) fn clear_session(&mut self) {
        *self = Self::default();
    }
}

/// Per VM ([`VmMemo`]): a `/reload`'s new VM is fed the categories, ticket and status afresh.
#[derive(Resource, Default)]
struct GmTicketFeedState {
    vm: VmMemo<FedTicket>,
}

#[derive(Default)]
struct FedTicket {
    answers: u32,
    queue_answers: u32,
    categories_pushed: bool,
    survey_offers: u32,
}

/// Fire `UPDATE_TICKET` and `UPDATE_GM_STATUS` for unheard answers; push the categories once.
fn feed_gm_ticket(
    script: Option<NonSendMut<UiScript>>,
    state: Res<GmTicketState>,
    categories: Option<Res<GmTicketCategories>>,
    mut feed: ResMut<GmTicketFeedState>,
    survey: Option<Res<GmSurveyTables>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let fed = feed.vm.get(&script);

    if !fed.categories_pushed {
        if let Some(categories) = categories.as_deref() {
            script.set_gm_ticket_categories(categories.0.clone());
            fed.categories_pushed = true;
        }
    }

    if fed.queue_answers != state.queue_answers {
        fed.queue_answers = state.queue_answers;
        // `arg1` is the status as a number: `HelpFrame` tests `== 1` for up and `== -1` to show
        // `HELP_TICKET_QUEUE_DISABLED`; vmangos sends only 0 or 1, passed through unmapped.
        script.fire_event(
            "UPDATE_GM_STATUS",
            vec![ScriptValue::Int(i64::from(state.queue_status))],
        );
    }

    if fed.answers != state.answers {
        fed.answers = state.answers;
        script.fire_event("UPDATE_TICKET", update_ticket_args(state.ticket.as_deref()));
    }

    // Build 12340: a survey offer publishes the current survey and fires `GMSURVEY_DISPLAY`.
    if fed.survey_offers != state.survey_offers {
        fed.survey_offers = state.survey_offers;
        if let Some(tables) = survey.as_deref() {
            script.set_wrath_gm_survey(std::sync::Arc::new(tables.survey()));
            script.fire_event("GMSURVEY_DISPLAY", vec![]);
        }
    }
}

/// `UPDATE_TICKET`'s arguments: category, text, the three day figures, assigned, opened. The wire
/// has the text first and the reference's handler reorders as it fires, so the decoder keeps wire
/// order. No ticket is a single `0`: `HelpFrameOpenTicket_OnEvent` tests `arg1 and arg1 ~= 0`
/// (`HelpFrame.lua:403`), and its else branch resets the form (`:468-475`).
fn update_ticket_args(ticket: Option<&benilla_protocol::messages::GmTicket>) -> Vec<ScriptValue> {
    let Some(t) = ticket else {
        return vec![ScriptValue::Int(0)];
    };
    vec![
        ScriptValue::Int(i64::from(t.category)),
        ScriptValue::Str(t.text.clone()),
        // Days, unclamped: a negative figure is the server's "unknown", which the window shows as
        // `GM_TICKET_UNAVAILABLE` (`HelpFrame.lua:442`).
        ScriptValue::Number(f64::from(t.ticket_age)),
        ScriptValue::Number(f64::from(t.oldest_ticket_age)),
        ScriptValue::Number(f64::from(t.update_time)),
        ScriptValue::Int(i64::from(t.assigned_to_gm)),
        ScriptValue::Int(i64::from(t.opened_by_gm)),
    ]
}

/// Turn the window's clicks into packets.
fn drain_gm_ticket(
    script: Option<NonSendMut<UiScript>>,
    mut reask: ResMut<GmTicketState>,
    commands: Res<NetCommands>,
    player: Option<Res<Player>>,
    map: Option<Res<benilla_world::world_map::CurrentMap>>,
) {
    let Some(mut script) = script else {
        return;
    };

    // The engine's re-asks first: they answer a push that arrived before this frame's input.
    for _ in 0..reask.take_reasks() {
        let _ = commands.0.send(ClientCommand::GmTicketGet);
    }

    // `GMSurveySubmit`: `CMSG_GMSURVEY_SUBMIT` with each answered slot.
    for submit in script.take_wrath_gm_survey_submits() {
        use benilla_protocol::wrath_help::{Command, SurveyAnswer};
        let _ = commands
            .0
            .send(ClientCommand::NativeHelp(Command::SurveySubmit {
                survey: submit.survey,
                answers: submit
                    .answers
                    .into_iter()
                    .map(|(question, rank, comment)| SurveyAnswer {
                        question,
                        rank,
                        comment,
                    })
                    .collect(),
                comment: submit.comment,
            }));
    }

    for _ in 0..script.take_stuck_casts() {
        let _ = commands.0.send(ClientCommand::CastSpell {
            spell_id: SPELL_STUCK,
            target: None,
        });
    }

    let intents = script.take_gm_ticket_intents();
    if intents.is_empty() {
        return;
    }
    // Where the player stands at send time, in WoW coordinates: where `.ticket go` takes the GM.
    let pos = player.map(|p| bevy_to_wow(p.pos)).unwrap_or([0.0; 3]);
    let map = map.map(|m| m.0).unwrap_or(0);
    // In call order: `DeleteGMTicket(); GetGMTicket()` must not become get-then-delete.
    for intent in intents {
        let _ = commands.0.send(match intent {
            GmTicketIntent::Ask => ClientCommand::GmTicketGet,
            GmTicketIntent::AskStatus => ClientCommand::GmTicketSystemStatus,
            GmTicketIntent::Delete => ClientCommand::GmTicketDelete,
            // Build 12340 only: the globals behind these exist in the Wrath dialect alone.
            GmTicketIntent::ReportLag { lag_type } => {
                ClientCommand::NativeHelp(benilla_protocol::wrath_help::Command::ReportLag {
                    lag_type,
                    map,
                    pos,
                })
            }
            GmTicketIntent::NeedMoreHelp { text } => {
                ClientCommand::NativeHelp(benilla_protocol::wrath_help::Command::Create {
                    map,
                    pos,
                    text,
                    need_response: true,
                    need_more_help: true,
                })
            }
            GmTicketIntent::Write(write) => client_command_for(write, map, pos),
        });
    }
}

/// One drained write to its packet. The window's own `hasTicket` picks create or update, never our
/// belief, which can be 10 minutes stale. 3.3.5 carries no category (AzerothCore
/// `HandleGMTicketCreateOpcode` reads map, position, text and the two flags), so the write's is not
/// sent.
fn client_command_for(write: GmTicketWrite, map: u32, pos: [f32; 3]) -> ClientCommand {
    if write.is_new {
        ClientCommand::GmTicketCreate {
            map,
            pos,
            text: write.text,
        }
    } else {
        ClientCommand::GmTicketUpdate { text: write.text }
    }
}

/// `GMTicketCategory.dbc` as the `(id, name)` pairs `GetGMTicketCategories()` returns.
#[derive(Resource)]
pub(crate) struct GmTicketCategories(pub(crate) Vec<(u32, String)>);

/// Startup, after the MPQ chain opens: a bare `Startup` races the open and can leave the category
/// list empty for the whole session.
fn load_gm_ticket_dbc(
    mut commands: Commands,
    world_assets: Option<Res<benilla_assets::WorldAssets>>,
) {
    use benilla_assets::LockRecover;
    let Some(world_assets) = world_assets else {
        return;
    };
    let mut chain = world_assets.chain.lock_recover();
    match benilla_formats::load_gm_ticket_categories(&mut chain) {
        Ok(cat) => {
            info!(
                "gm ticket: GMTicketCategory.dbc loaded ({} categories)",
                cat.len()
            );
            commands.insert_resource(GmTicketCategories(
                cat.categories()
                    .iter()
                    .map(|c| (c.id, c.name.clone()))
                    .collect(),
            ));
        }
        Err(e) => warn!("gm ticket: GMTicketCategory.dbc failed to load: {e:#}"),
    }
    let benilla_formats::DataProfile::Wrath { locale } = chain.profile();
    let locale_id = locale_id(locale);
    match benilla_formats::gm_survey::load_gm_survey(&mut chain) {
        Ok(catalog) => commands.insert_resource(GmSurveyTables { catalog, locale_id }),
        Err(e) => warn!("gm survey: DBCs failed to load: {e:#}"),
    }
}

/// The GM ticket's packet handlers.
pub(crate) mod net {
    use benilla_protocol::{SessionEvent, SessionEventKind};
    use bevy::prelude::*;

    use crate::net::NetHandlerApp;

    use super::GmTicketState;

    /// Register the ticket's handlers, called from [`super::UiGmTicketPlugin`].
    pub(super) fn register(app: &mut App) {
        use SessionEventKind as K;
        app.net_handler(K::GmTicket, on_ticket)
            .net_handler(K::GmTicketSystemStatus, on_system_status)
            .net_handler(K::GmTicketStatusUpdate, on_status_update)
            .net_handler(K::GmTicketCreated, on_written)
            .net_handler(K::GmTicketUpdated, on_written)
            .net_handler(K::GmTicketDeleted, on_deleted)
            .net_handler(K::Disconnected, on_session_end);
    }

    /// Every GETTICKET answer, asked for or pushed after a GM's `.ticket` command
    /// (`GMTicketMgr.cpp:153-159`): the wire does not tell them apart.
    fn on_ticket(In(ev): In<SessionEvent>, mut ticket: ResMut<GmTicketState>) {
        if let SessionEvent::GmTicket { ticket: answer } = ev {
            ticket.answer(answer);
        }
    }

    fn on_system_status(In(ev): In<SessionEvent>, mut ticket: ResMut<GmTicketState>) {
        if let SessionEvent::GmTicketSystemStatus { status } = ev {
            ticket.answer_queue(status);
        }
    }

    /// `SMSG_GM_TICKET_STATUS_UPDATE`, which vmangos never sends; cmangos does, so it is parsed.
    fn on_status_update(In(ev): In<SessionEvent>, mut ticket: ResMut<GmTicketState>) {
        if let SessionEvent::GmTicketStatusUpdate { status } = ev {
            status_update(status, &mut ticket);
        }
    }

    /// The create and update responses, which no 1.12 FrameXML handler reads.
    fn on_written(In(ev): In<SessionEvent>, mut ticket: ResMut<GmTicketState>) {
        match ev {
            SessionEvent::GmTicketCreated { response } => {
                write_response("create", response, 2, &mut ticket)
            }
            SessionEvent::GmTicketUpdated { response } => {
                write_response("update", response, 4, &mut ticket)
            }
            _ => {}
        }
    }

    fn on_deleted(In(ev): In<SessionEvent>) {
        if let SessionEvent::GmTicketDeleted { response: code } = ev {
            response("delete", code);
        }
    }

    /// The ticket is the character's and the next login may be another, so the answer counts
    /// reset too and the new session's first answer fires.
    fn on_session_end(In(_): In<SessionEvent>, mut ticket: ResMut<GmTicketState>) {
        ticket.clear_session();
    }

    /// `SMSG_GMTICKET_CREATE` 2 (created) and `SMSG_GMTICKET_UPDATETEXT` 4 (saved) re-ask for the
    /// ticket, as the reference engine does at `0x5e4479`. Other codes are logged only: which
    /// `ERR_TICKET_*` string the reference shows for each is untraced.
    pub(crate) fn write_response(
        what: &str,
        response: u32,
        success: u32,
        ticket: &mut super::GmTicketState,
    ) {
        if response == success {
            debug!("gm ticket: {what} landed ({response}) — re-asking for the ticket");
            ticket.note_write_landed();
        } else {
            debug!("gm ticket: {what} answered {response}");
        }
    }

    /// `SMSG_GM_TICKET_STATUS_UPDATE`: 1 (updated) re-asks, the reference's `0x5e7932`; 2 (closed)
    /// shows on the next poll; 3 offers the GM survey (`GMSURVEY_DISPLAY`).
    pub(crate) fn status_update(status: u32, ticket: &mut super::GmTicketState) {
        match status {
            1 => {
                debug!("gm ticket: a GM updated the ticket — re-asking");
                ticket.note_write_landed();
            }
            2 => debug!("gm ticket: a GM closed the ticket"),
            3 => {
                debug!("gm ticket: a GM survey was offered");
                ticket.survey_offers = ticket.survey_offers.wrapping_add(1);
            }
            other => debug!("gm ticket: unknown status update {other}"),
        }
    }

    /// `SMSG_GMTICKET_DELETETICKET`: logged only, as the reference's `0x5e4479` re-asks on create
    /// and update alone.
    pub(crate) fn response(what: &str, response: u32) {
        debug!("gm ticket: {what} answered {response}");
    }
}

/// The GM trouble-ticket flow: the answer feed and the window's sends.
pub(crate) struct UiGmTicketPlugin;

impl Plugin for UiGmTicketPlugin {
    fn build(&self, app: &mut App) {
        net::register(app);
        app.init_resource::<GmTicketState>()
            .init_resource::<GmTicketFeedState>()
            .add_systems(
                Startup,
                load_gm_ticket_dbc.after(benilla_assets::AssetSet::Open),
            )
            .add_systems(
                Update,
                (
                    feed_gm_ticket.in_set(UiFeed),
                    drain_gm_ticket.after(UiInput),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::messages::GmTicket;

    fn ticket() -> Box<GmTicket> {
        Box::new(GmTicket {
            text: "Stuck in a rock.".into(),
            category: 1,
            ticket_age: 0.25,
            oldest_ticket_age: 2.5,
            update_time: 0.01,
            assigned_to_gm: 2,
            opened_by_gm: 1,
        })
    }

    /// The event leads with the category, the packet with the text.
    #[test]
    fn the_event_args_lead_with_the_category_not_the_text() {
        let t = ticket();
        let args = update_ticket_args(Some(&t));
        assert_eq!(args[0], ScriptValue::Int(1), "arg1 is the category");
        assert_eq!(
            args[1],
            ScriptValue::Str("Stuck in a rock.".into()),
            "arg2 is the description"
        );
        assert_eq!(args.len(), 7, "arg1..arg7, the shipped window's whole read");
        assert_eq!(args[5], ScriptValue::Int(2), "arg6 assignedToGM");
        assert_eq!(args[6], ScriptValue::Int(1), "arg7 openedByGM");
    }

    /// The window's form reset hangs on `arg1 and arg1 ~= 0`.
    #[test]
    fn the_no_ticket_answer_is_a_single_zero() {
        assert_eq!(update_ticket_args(None), vec![ScriptValue::Int(0)]);
    }

    /// The window's `arg4 < 0 or arg5 < 0` test shows "unavailable".
    #[test]
    fn an_unknown_wait_time_reaches_lua_as_a_negative_not_a_zero() {
        let mut t = ticket();
        t.oldest_ticket_age = -1.0;
        t.update_time = -1.0;
        let args = update_ticket_args(Some(&t));
        assert_eq!(args[3], ScriptValue::Number(-1.0));
        assert_eq!(args[4], ScriptValue::Number(-1.0));
    }

    /// The window's 10-minute poll must be re-fed each time.
    #[test]
    fn a_repeated_identical_answer_still_counts_as_an_answer() {
        let mut state = GmTicketState::default();
        state.answer(Some(ticket()));
        let first = state.answers;
        state.answer(Some(ticket()));
        assert_ne!(state.answers, first, "the poll must re-fire UPDATE_TICKET");

        // A repeated empty answer counts too: "still no ticket" is the common case.
        state.answer(None);
        let empty = state.answers;
        state.answer(None);
        assert_ne!(state.answers, empty);
    }

    /// The reference's `0x5e4479` re-ask; a refusal has nothing new to fetch.
    #[test]
    fn a_landed_write_makes_the_engine_reask_and_a_refused_one_does_not() {
        let mut state = GmTicketState::default();
        net::write_response("create", 2, 2, &mut state);
        net::write_response("update", 4, 4, &mut state);
        assert_eq!(state.take_reasks(), 2, "one re-ask per landed write");
        assert_eq!(state.take_reasks(), 0, "drained means drained");

        net::write_response("create", 3, 2, &mut state); // CREATE_ERROR
        net::write_response("update", 5, 4, &mut state); // UPDATE_ERROR
        assert_eq!(state.take_reasks(), 0, "a refusal has nothing to fetch");
    }

    /// vmangos sends only 0 or 1, so no live run reaches the window's -1 arm.
    #[test]
    fn a_minus_one_queue_status_survives_to_lua_as_minus_one() {
        let mut state = GmTicketState::default();
        state.answer_queue(-1);
        assert_eq!(state.queue_status, -1);
        assert_eq!(i64::from(state.queue_status), -1_i64);
    }

    #[test]
    fn a_dead_session_forgets_the_ticket_and_both_answer_counts() {
        let mut state = GmTicketState::default();
        state.answer(Some(ticket()));
        state.answer_queue(1);
        state.clear_session();
        assert!(state.ticket.is_none());
        assert_eq!(state.answers, 0);
        assert_eq!(state.queue_answers, 0);
    }

    #[test]
    fn the_windows_verb_picks_the_opcode() {
        let write = |category, is_new| GmTicketWrite {
            category,
            text: "gone".into(),
            is_new,
        };
        assert!(matches!(
            client_command_for(write(4, true), 1, [1.0, 2.0, 3.0]),
            ClientCommand::GmTicketCreate { map: 1, .. }
        ));
        assert!(matches!(
            client_command_for(write(4, false), 1, [1.0, 2.0, 3.0]),
            ClientCommand::GmTicketUpdate { .. }
        ));
    }
}
