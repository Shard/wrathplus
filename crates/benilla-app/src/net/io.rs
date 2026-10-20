//! The background networking threads, the socket half of the net bridge: [`spawn_net`] starts the
//! build-12340 backend ([`wrath`]), which refuses a login for any other dialect. The 1.12 session
//! is gone (Vanilla removal step 3).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use benilla_protocol::SessionEvent;
use crossbeam_channel::{Receiver, Sender};

use super::{CharRequest, RealmRequest, SessionCommand, SessionCommandSender};

pub(super) mod wrath;

/// The inbound census: every packet off the world socket, and the unix ms of the latest, which
/// tell a silent server from a dead socket for the runaway watch ([`crate::motion`]).
pub(crate) static INBOUND_PACKETS: AtomicU64 = AtomicU64::new(0);
static LAST_INBOUND_UNIX_MS: AtomicU64 = AtomicU64::new(0);

/// Counts one packet, parsed or skipped: either proves the socket is alive.
fn note_inbound() {
    INBOUND_PACKETS.fetch_add(1, Ordering::Relaxed);
    LAST_INBOUND_UNIX_MS.store(unix_ms(), Ordering::Relaxed);
}

/// Wall-clock unix ms, the clock two processes' traces share (the trace header's `t0`).
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `(packets seen, ms since the last one)`.
pub(crate) fn inbound_census() -> (u64, Option<u64>) {
    let last = LAST_INBOUND_UNIX_MS.load(Ordering::Relaxed);
    (
        INBOUND_PACKETS.load(Ordering::Relaxed),
        (last != 0).then(|| unix_ms().saturating_sub(last)),
    )
}

/// One login attempt and the abandon generation at submit time; the thread discards the attempt
/// at its next stage boundary once a Cancel has moved the counter. A counter, since a flag cleared
/// by the next submit would un-cancel the attempt in flight.
#[derive(Clone)]
pub(crate) struct LoginRequest {
    pub(crate) user: String,
    pub(crate) pass: String,
    /// The realmlist to dial, `host[:port]`, per attempt so an edit mid-dial cannot retarget the
    /// connection in flight.
    pub(crate) host: String,
    pub(crate) generation: u64,
    /// The app's startup snapshot dialect. The backend rejects a mismatch rather than falling back.
    pub(crate) dialect: super::SessionDialect,
}

/// The park-answer receivers, in the order a cycle blocks on them: credentials, realm, character.
pub(super) struct Parks {
    pub(super) login_rx: Receiver<LoginRequest>,
    pub(super) realm_rx: Receiver<RealmRequest>,
    pub(super) pick_rx: Receiver<CharRequest>,
}

/// The reference's 30 s ping timer (`0x537ff0`). vmangos kicks a socket whose pings come faster
/// than 27 s apart more than twice (`WorldSocket::_HandlePing`), so this must not shrink.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Round trips [`PingClock`] keeps: 15. The reference's ring (`conn+0x1a6c`) has 16 slots, but
/// `read == write` means empty (`0x537fa8`) and `HandlePong` advances the read index (`0x537de8`),
/// so at most 15 are ever averaged.
const RTT_RING: usize = 15;

/// The connection's ping stats, the reference's per-connection block (`conn+0x1a6c` ring,
/// `+0x1aac/+0x1ab0` head and tail, `+0x1a64` send stamp, `+0x1a68` expected sequence) under one
/// lock, as `conn+0x1ac0` guards it for `HandlePong` (`0x537d60`) and `GetNetStats` (`0x537f20`).
///
/// The write thread stamps each `CMSG_PING`; the read thread times the `SMSG_PONG` the instant it
/// lands, as the reference's `OnData` (`0x537b10`) hands it to `HandlePong` inline, since timing
/// it in the frame drain would add a client frame to every reading; the app reads
/// [`Self::avg_latency_ms`].
#[derive(Default)]
pub(crate) struct PingClock {
    /// Sequence of the latest ping sent, counting from 1 per connection.
    pub(crate) sequence: u32,
    /// When that ping went out.
    pub(crate) sent_at: Option<Instant>,
    /// The last round trip (ms), sent as the next ping's `lastRtt` as the reference does.
    pub(crate) last_rtt_ms: Option<u32>,
    /// The latest [`RTT_RING`] round trips (ms), oldest first.
    rtt_ring: std::collections::VecDeque<u32>,
}

impl PingClock {
    /// `HandlePong`: times and files one `SMSG_PONG`; a mismatched sequence is dropped, as in
    /// the reference.
    pub(crate) fn record_pong(&mut self, sequence: u32) -> Option<u32> {
        let sent = self.sent_at.filter(|_| self.sequence == sequence)?;
        let rtt = sent.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        self.file_rtt(rtt);
        Some(rtt)
    }

    /// The filing half of [`Self::record_pong`], apart so a test can give exact milliseconds.
    fn file_rtt(&mut self, rtt: u32) {
        self.last_rtt_ms = Some(rtt);
        if self.rtt_ring.len() == RTT_RING {
            self.rtt_ring.pop_front();
        }
        self.rtt_ring.push_back(rtt);
    }

    /// `GetNetStats`' latency: the unsigned truncating mean of the samples present (`0x537f20`,
    /// `div` at `0x537fce`); `None` when empty, which the UI shows as the reference's 0
    /// (`0x537fd2`).
    pub(crate) fn avg_latency_ms(&self) -> Option<u32> {
        if self.rtt_ring.is_empty() {
            return None;
        }
        let sum: u64 = self.rtt_ring.iter().map(|&ms| u64::from(ms)).sum();
        Some((sum / self.rtt_ring.len() as u64) as u32)
    }

    /// Forgets this connection's measurements.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Cap on send warnings per connection, so a movement stream during an outage cannot flood the log.
const SEND_WARN_CAP: u32 = 8;

/// Everything [`spawn_net`] hands the app.
pub(super) struct NetHandles {
    pub(super) events: Receiver<SessionEvent>,
    pub(super) commands: super::NetCommands,
    pub(super) command_endpoints: Receiver<SessionCommandSender>,
    pub(super) pick: Sender<CharRequest>,
    pub(super) realm: Sender<RealmRequest>,
    pub(super) login: Sender<LoginRequest>,
    pub(super) login_abandon: Arc<AtomicU64>,
    pub(super) ping: Arc<Mutex<PingClock>>,
}

/// Spawns the build-12340 backend's threads; offline (capture mode), the channel ends drop here,
/// sends become ignored `Err`s and the event stream stays empty.
pub(super) fn spawn_net(connect: bool) -> NetHandles {
    let (events_tx, events_rx) = crossbeam_channel::unbounded();
    let (session_cmd_tx, session_cmd_rx) = crossbeam_channel::unbounded::<SessionCommand>();
    let (command_endpoint_tx, command_endpoint_rx) = crossbeam_channel::unbounded();
    let (pick_tx, pick_rx) = crossbeam_channel::unbounded::<CharRequest>();
    let (realm_tx, realm_rx) = crossbeam_channel::unbounded::<RealmRequest>();
    let (login_tx, login_rx) = crossbeam_channel::unbounded::<LoginRequest>();
    let login_abandon = Arc::new(AtomicU64::new(0));
    let ping_clock = Arc::new(Mutex::new(PingClock::default()));
    let commands = if connect {
        wrath::spawn_threads(
            events_tx,
            session_cmd_rx,
            Parks {
                login_rx,
                realm_rx,
                pick_rx,
            },
            Arc::clone(&login_abandon),
            Arc::clone(&ping_clock),
            session_cmd_tx.clone(),
            command_endpoint_tx,
        );
        super::NetCommands::session_stamped(SessionCommandSender::new(session_cmd_tx, 0))
    } else {
        // Offline, a raw channel whose receiver is gone: no session endpoint is ever handed over.
        let (raw_tx, _) = crossbeam_channel::unbounded();
        super::NetCommandsRaw(super::CommandSender::Raw(raw_tx))
    };
    NetHandles {
        events: events_rx,
        commands,
        command_endpoints: command_endpoint_rx,
        pick: pick_tx,
        realm: realm_tx,
        login: login_tx,
        login_abandon,
        ping: ping_clock,
    }
}

#[cfg(test)]
mod rtt_tests {
    use super::{PingClock, RTT_RING};
    use std::time::Instant;

    /// Arms the clock as the write thread does.
    fn sent(clock: &mut PingClock, sequence: u32) {
        clock.sequence = sequence;
        clock.sent_at = Some(Instant::now());
    }

    /// At a 30 s cadence one spike moves the meter by a fifteenth, not the whole way.
    #[test]
    fn the_reported_latency_is_the_mean_of_a_bounded_ring() {
        let mut clock = PingClock::default();
        assert_eq!(clock.avg_latency_ms(), None, "no pong yet");

        clock.file_rtt(40);
        assert_eq!(clock.avg_latency_ms(), Some(40));
        clock.file_rtt(60);
        assert_eq!(clock.avg_latency_ms(), Some(50), "the mean, not the last");
        assert_eq!(
            clock.last_rtt_ms,
            Some(60),
            "the last sample stays separate"
        );

        // Fill past the ring: only the newest RTT_RING samples count, so the two above age out.
        for _ in 0..RTT_RING {
            clock.file_rtt(100);
        }
        assert_eq!(clock.rtt_ring.len(), RTT_RING);
        assert_eq!(clock.avg_latency_ms(), Some(100));

        clock.clear();
        assert_eq!(clock.avg_latency_ms(), None, "a disconnect forgets it all");
        assert_eq!(clock.last_rtt_ms, None);
    }

    /// A pong from a dead socket (straddling a reconnect, after `clear`) must not enter the ring.
    #[test]
    fn only_the_pong_we_are_waiting_for_is_recorded() {
        let mut clock = PingClock::default();
        assert_eq!(clock.record_pong(1), None, "no ping is in flight");

        sent(&mut clock, 7);
        assert_eq!(
            clock.record_pong(6),
            None,
            "a stale sequence is not our ping"
        );
        assert_eq!(clock.record_pong(8), None, "nor is one we never sent");
        assert!(clock.rtt_ring.is_empty(), "neither entered the ring");

        assert!(clock.record_pong(7).is_some(), "the one we are timing");
        assert_eq!(clock.rtt_ring.len(), 1);
        assert!(clock.avg_latency_ms().is_some());

        // The reconnect edge: cleared, so the old socket's echo has nothing to match.
        clock.clear();
        assert_eq!(
            clock.record_pong(7),
            None,
            "the dead socket's echo is dropped"
        );
        assert_eq!(clock.avg_latency_ms(), None);
    }
}
