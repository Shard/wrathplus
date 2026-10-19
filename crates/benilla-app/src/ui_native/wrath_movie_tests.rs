use super::*;

fn app() -> (App, crossbeam_channel::Receiver<ClientCommand>) {
    let mut app = App::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx))
        .init_resource::<NativeWorldObjectsPackets>()
        .init_resource::<PendingMovie>()
        .insert_resource(Movies(
            std::collections::HashMap::from([(
                2,
                benilla_formats::Movie {
                    file: "Interface\\Cinematics\\WOW_Intro.avi".into(),
                    volume: 100,
                },
            )])
            .into(),
        ))
        .add_systems(Update, feed_movie);
    let script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    script
        .run(
            r#"
            played = {}
            local f = CreateFrame("Frame")
            f:RegisterEvent("PLAY_MOVIE")
            f:SetScript("OnEvent", function(_, _, name, volume)
                played[#played + 1] = name .. "@" .. volume
                -- The stock MovieFrame_PlayMovie: the movie cannot start, so it finishes.
                GameMovieFinished()
            end)
        "#,
        )
        .unwrap();
    app.world_mut().insert_non_send_resource(script);
    (app, rx)
}

fn trigger(app: &mut App, movie: u32) {
    app.world_mut()
        .resource_mut::<NativeWorldObjectsPackets>()
        .0
        .push_back(Packet::TriggerMovie { movie });
}

fn completes(rx: &crossbeam_channel::Receiver<ClientCommand>) -> usize {
    rx.try_iter()
        .filter(|c| {
            matches!(
                c,
                ClientCommand::NativeWorldObjects(Command::CompleteMovie)
            )
        })
        .count()
}

#[test]
fn a_known_movie_plays_and_its_finish_completes_it_once() {
    let (mut app, rx) = app();
    trigger(&mut app, 2);
    app.update();
    let played: Vec<String> = app
        .world()
        .non_send_resource::<UiScript>()
        .eval("return played")
        .unwrap();
    assert_eq!(played, ["Interface\\Cinematics\\WOW_Intro@100"]);
    assert_eq!(completes(&rx), 0, "the finish is read on the next feed");
    app.update();
    assert_eq!(completes(&rx), 1);
    assert!(app.world().resource::<PendingMovie>().id.is_none());
    app.update();
    assert_eq!(completes(&rx), 0, "nothing pending, nothing sent");
}

#[test]
fn an_unknown_movie_finishes_without_playing() {
    let (mut app, rx) = app();
    trigger(&mut app, 99);
    app.update();
    let played: Vec<String> = app
        .world()
        .non_send_resource::<UiScript>()
        .eval("return played")
        .unwrap();
    assert!(played.is_empty());
    assert_eq!(completes(&rx), 1);
}
