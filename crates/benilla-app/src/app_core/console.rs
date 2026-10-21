//! The console command registry's table and its registration trait, which any plugin uses
//! (`video`, `experience`, the UI); the built-ins and [`crate::console::execute`] are the console's.

use std::collections::BTreeMap;

use bevy::prelude::*;

/// A command's body: the world and the trimmed argument tail in, the lines to print out.
pub(crate) type ConsoleHandler = fn(&mut World, &str) -> Vec<String>;

#[derive(Clone, Copy)]
pub(crate) struct ConsoleCommand {
    /// The registered spelling, as `help` prints it.
    pub(crate) name: &'static str,
    pub(crate) help: &'static str,
    pub(crate) run: ConsoleHandler,
}

/// The registry: lowercased name to command, in name order.
#[derive(Resource, Default)]
pub(crate) struct ConsoleCommands {
    pub(crate) by_key: BTreeMap<String, ConsoleCommand>,
}

/// Registers a command from a plugin, the reference's `ConsoleCommandRegister`.
pub(crate) trait ConsoleCommandApp {
    fn console_command(
        &mut self,
        name: &'static str,
        help: &'static str,
        run: ConsoleHandler,
    ) -> &mut Self;
}

impl ConsoleCommandApp for App {
    fn console_command(
        &mut self,
        name: &'static str,
        help: &'static str,
        run: ConsoleHandler,
    ) -> &mut Self {
        let mut table = self.world_mut().get_resource_or_init::<ConsoleCommands>();
        let key = name.to_ascii_lowercase();
        // Two registrations of one name is a wiring bug.
        assert!(
            !table.by_key.contains_key(&key),
            "console command {name:?} registered twice"
        );
        table.by_key.insert(key, ConsoleCommand { name, help, run });
        self
    }
}
