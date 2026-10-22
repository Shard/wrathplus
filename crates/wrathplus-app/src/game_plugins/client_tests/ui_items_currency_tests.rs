//! Whole-client tests of `crate::ui_items::currency`, from `ui_items/currency/tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::ObjectStore;
use crate::net::SelfPlayer;
use crate::ui_items::currency::tests::fields;
use crate::ui_items::currency::*;
use wrathplus_protocol::types::Field;
use wrathplus_ui::script::UiScript;
use bevy::prelude::*;

#[test]
fn real_feed_preserves_optional_native_balances_and_clears_logout() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    app.insert_non_send_resource(UiScript::new().unwrap());
    let mut schedule = Schedule::default();
    schedule.add_systems(feed);
    let entity = app
        .world_mut()
        .spawn((
            SelfPlayer,
            ObjectStore(fields(&[
                (Field::PlayerHonorCurrency, 0, 0),
                (Field::PlayerArenaCurrency, 0, u32::MAX),
            ])),
        ))
        .id();
    schedule.run(app.world_mut());
    let script = app.world().non_send_resource::<UiScript>();
    script.run("local h,hcap=GetHonorCurrency(); local a,acap=GetArenaCurrency(); assert(h==0 and hcap==75000 and a==4294967295 and acap==10000)").unwrap();
    // An identical source must seed a new VM, not be hidden by the previous VM's memo.
    app.insert_non_send_resource(UiScript::new().unwrap());
    schedule.run(app.world_mut());
    app.world()
        .non_send_resource::<UiScript>()
        .run("assert(GetArenaCurrency()==4294967295)")
        .unwrap();
    app.world_mut().despawn(entity);
    schedule.run(app.world_mut());
    app.world()
        .non_send_resource::<UiScript>()
        .run("assert(GetHonorCurrency()==0 and GetArenaCurrency()==0 and GetCurrencyListSize()==0)")
        .unwrap();
}
