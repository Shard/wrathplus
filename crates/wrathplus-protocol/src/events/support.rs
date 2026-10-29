//! Supporting types carried by decoded session events.

/// One native talent spec; talent ranks are zero-based, glyphs are GlyphProperties.dbc IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentGroup {
    pub talents: Vec<(u32, u8)>,
    pub glyphs: Vec<u16>,
}

/// Coarse entity classification, free of wire types, carried on [`super::SessionEvent::ObjectCreate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Player,
    Unit,
    GameObject,
    /// `TYPEID_DYNAMICOBJECT` (6): an area effect's anchor, drawn via `DYNAMICOBJECT_SPELLID`.
    DynamicObject,
    /// `TYPEID_CORPSE` (7): a dead player's body or bone pile, drawn as the player from its
    /// `CORPSE_FIELD_BYTES_*` and `CORPSE_FIELD_ITEM`.
    Corpse,
    Other,
}

/// Which character verb a [`super::SessionEvent::WrathCharActionResult`] answers, and so which code family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharAction {
    Create,
    Delete,
}

/// A unit's six movement speeds from its `LIVING` block: yd/s, except `turn_rate` in rad/s.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveSpeeds {
    pub walk: f32,
    pub run: f32,
    pub run_back: f32,
    pub swim: f32,
    pub swim_back: f32,
    pub turn_rate: f32,
    /// Build 12340's flight speeds and pitch rate (the create block's sixth, seventh and ninth
    /// speeds); 1.12 has none and takes the stock 7.0, 4.5 and pi.
    pub flight: f32,
    pub flight_back: f32,
    pub pitch_rate: f32,
}

/// How the server evaluates points between a movement spline's nodes. This is independent of
/// `flying`: Wrath permits a smooth ground path and a linear path that retains wire Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplineInterpolation {
    Linear,
    CatmullRom,
}

/// Optional vertical/time effect carried by a Wrath movement spline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplineEffect {
    Animation {
        animation_id: u8,
        effect_start_time_ms: u32,
    },
    Parabolic {
        vertical_acceleration: f32,
        effect_start_time_ms: u32,
    },
}

/// A login attempt's pre-roster stage; the connecting dialog shows its `LOGIN_STATE_*` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStage {
    /// Dialing the auth server (`LOGIN_STATE_CONNECTING`).
    Connecting,
    /// SRP6 challenge/proof in flight (`LOGIN_STATE_AUTHENTICATING`).
    Authenticating,
    /// Realm picked; the world-socket handshake and char enum (`LOGIN_STATE_HANDSHAKING`).
    Handshaking,
}

/// How a world session ended. On a loss the reference fires `DISCONNECTED_FROM_SERVER` and
/// `GlueParent.lua` returns to the login screen; a logout returns to character select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// A logout the server confirmed (`SMSG_LOGOUT_COMPLETE`); a fresh roster follows.
    LoggedOut,
    /// The stream died: a socket error, an EOF or a post-roster handshake failure. A displacement
    /// kick is this too: vmangos `WorldSession::KickPlayer` only closes the socket, no packet.
    Lost,
}

/// Which server refused the login, and its result byte. The two enums overlap: 0x0C is
/// `AUTH_LOGON_FAILED_SUSPENDED` to realmd but `AUTH_OK` to the world server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginRefusal {
    /// realmd's logon-proof `AuthLogonResult` (`crate::AuthReject`).
    Logon(u8),
    /// The world server's `SMSG_AUTH_RESPONSE` code (`crate::WorldAuthReject`).
    World(u8),
}

impl LoginRefusal {
    /// The raw byte, for logging only: without its server it is ambiguous.
    pub fn byte(self) -> u8 {
        match self {
            LoginRefusal::Logon(b) | LoginRefusal::World(b) => b,
        }
    }
}
