//! A skinnable corpse's tooltip line (12340 `0x620fc3`): the trade's `UNIT_SKINNABLE_*` text,
//! coloured by the trade's difficulty for the player (`0x61a1f0`), and with `colorblindMode` set,
//! prefixed by the difficulty's mark (`0xad2ac8`, formatted `"%s%s"`).

use mlua::Lua;

/// `0xad2d54`..`0xad2d64`: grey, green, yellow, orange, red, as `0xAARRGGBB`.
const COLORS: [u32; 5] = [
    0xff80_8080,
    0xff40_c040,
    0xffff_ff00,
    0xffff_8040,
    0xffff_2020,
];
/// The colour-blind marks, `""` for grey.
const MARKS: [&str; 5] = ["", "[+] ", "[++] ", "[+++] ", "[-] "];

/// The line's text and colour; `difficulty` above 4 reads red.
pub(super) fn line(lua: &Lua, key: &str, difficulty: u8) -> (String, [f32; 4]) {
    let d = usize::from(difficulty.min(4));
    let text = crate::strings::global(lua, key).unwrap_or_else(|| "Skinnable".into());
    let colorblind = {
        let model = lua.app_data_ref::<super::Model>().expect("model app_data");
        model
            .cvars
            .get("colorblindmode")
            .is_some_and(|row| row.value.parse::<f64>().unwrap_or(0.0) != 0.0)
    };
    let mark = if colorblind { MARKS[d] } else { "" };
    let c = COLORS[d];
    let channel = |shift: u32| ((c >> shift) & 0xff) as f32 / 255.0;
    (
        format!("{mark}{text}"),
        [channel(16), channel(8), channel(0), 1.0],
    )
}

#[cfg(test)]
mod tests {
    use super::super::UiScript;

    fn line(s: &UiScript, key: &str, difficulty: u8) -> (String, [f32; 4]) {
        super::line(s.lua(), key, difficulty)
    }

    #[test]
    fn the_difficulty_picks_the_colour_and_colorblind_mode_the_mark() {
        let s = UiScript::new().unwrap();
        s.run(r#"UNIT_SKINNABLE_ROCK = "Requires Mining""#).unwrap();
        assert_eq!(line(&s, "UNIT_SKINNABLE_ROCK", 1).0, "Requires Mining");
        assert_eq!(
            line(&s, "UNIT_SKINNABLE_ROCK", 1).1,
            [64.0 / 255.0, 192.0 / 255.0, 64.0 / 255.0, 1.0]
        );
        assert_eq!(
            line(&s, "UNIT_SKINNABLE_ROCK", 4).1,
            [1.0, 32.0 / 255.0, 32.0 / 255.0, 1.0]
        );
        assert_eq!(
            line(&s, "UNIT_SKINNABLE_BOLTS", 0).0,
            "Skinnable",
            "strings missing"
        );
        s.run(r#"SetCVar("colorblindMode", "1")"#).unwrap();
        assert_eq!(
            line(&s, "UNIT_SKINNABLE_ROCK", 3).0,
            "[+++] Requires Mining"
        );
        assert_eq!(line(&s, "UNIT_SKINNABLE_ROCK", 0).0, "Requires Mining");
    }
}
