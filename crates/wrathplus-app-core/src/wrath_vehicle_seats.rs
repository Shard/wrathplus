//! Build 12340's vehicle tables, `Vehicle.dbc` and `VehicleSeat.dbc`, as one shared resource for
//! the vehicle bindings. [`VehicleSeats`] is absent without Wrath data or when the load fails, as
//! [`crate::chr_classes::ChrClassTable`] is.

use bevy::prelude::*;

use wrathplus_assets::{AssetSet, LockRecover, WorldAssets};
use wrathplus_formats::vehicles::{VehicleCatalog, VehicleSeat};

/// `Vehicle.dbc` and `VehicleSeat.dbc`.
#[derive(Resource)]
pub struct VehicleSeats(pub VehicleCatalog);

impl VehicleSeats {
    /// The seat in `vehicle_id`'s slot `index` (0-7), when both rows resolve.
    pub fn seat_of(&self, vehicle_id: u32, index: usize) -> Option<&VehicleSeat> {
        self.0.seat_of(vehicle_id, index)
    }

    /// A `VehicleSeat.dbc` row by its id.
    pub fn seat(&self, seat_id: u32) -> Option<&VehicleSeat> {
        self.0.seat(seat_id)
    }
}

pub struct VehicleSeatsPlugin;

impl Plugin for VehicleSeatsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load.after(AssetSet::Open));
    }
}

fn load(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    if !matches!(
        chain.profile(),
        wrathplus_formats::DataProfile::Wrath { .. }
    ) {
        return;
    }
    match wrathplus_formats::vehicles::load_vehicle_catalog(&mut chain) {
        Ok(catalog) => commands.insert_resource(VehicleSeats(catalog)),
        Err(error) => warn!("wrath_vehicle_seats: vehicle DBCs failed to load: {error:#}"),
    }
}
