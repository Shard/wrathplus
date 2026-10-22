//! The `benilla` launcher: a shim that carries the build stamp, so a new commit recompiles these
//! few lines and relinks instead of rebuilding `wrathplus-app`.

use wrathplus_app::BuildId;

fn main() -> wrathplus_app::AppExit {
    wrathplus_app::run(BuildId {
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
