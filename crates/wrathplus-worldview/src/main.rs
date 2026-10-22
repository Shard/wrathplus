//! The `wrathplus-worldview` launcher: boots the `wrathplus-world` engine with no game attached, so an
//! engine system that needs a gameplay resource fails here, a coupling no compiler sees. Like
//! `benilla`, a shim that carries the build stamp, so a new commit recompiles only these lines.

use wrathplus_world::build_id::BuildId;

fn main() -> wrathplus_world::AppExit {
    wrathplus_world::worldview::run(BuildId {
        version: env!("CARGO_PKG_VERSION"),
        describe: env!("BENILLA_GIT_DESCRIBE"),
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
        project_dir: env!("BENILLA_PROJECT_DIR"),
        ..Default::default()
    })
}
