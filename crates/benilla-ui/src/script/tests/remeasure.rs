//! A geometry read re-measures a FontString dirtied by `SetText` or `SetWidth`, as the reference's
//! region getters resolve a dirty rect inline (`GetLeft 0x49d0b0`): stock
//! `QuestLogTitleButton_Resize` sets the title's width to 0, sets its text and reads its right edge
//! in one call.

use super::common::script;
use crate::script::{MeasureRequest, TextMeasure};

/// Each character 7 wide, one 14-tall line.
struct SevenWide;

impl TextMeasure for SevenWide {
    fn measure(&mut self, req: &MeasureRequest) -> (f32, f32, f32) {
        let natural = req.text.chars().count() as f32 * 7.0;
        (natural, 14.0, natural)
    }
}

#[test]
fn an_edge_read_after_set_text_sees_the_new_box_and_its_dependents() {
    let mut s = script();
    s.set_screen_size(800.0, 600.0);
    s.set_text_measurer(Box::new(SevenWide));
    let (right, check_left, width) = s
        .eval::<(f32, f32, f32)>(
            r#"
            local f = CreateFrame("Frame", "Host")
            f:SetPoint("TOPLEFT", 0, 0); f:SetWidth(400); f:SetHeight(300)
            local title = f:CreateFontString("Title", "ARTWORK")
            title:SetPoint("LEFT", f, "LEFT", 10, 0)
            title:SetText("Short")
            assert(title:GetRight() == 10 + 5 * 7)
            local check = f:CreateTexture(nil, "ARTWORK")
            check:SetWidth(8); check:SetHeight(8)
            check:SetPoint("LEFT", title, "RIGHT", 2, 0)
            -- QuestLogTitleButton_Resize: width 0, the text, then the edges, in one call.
            title:SetWidth(0)
            title:SetText("A much longer title")
            return title:GetRight(), check:GetLeft(), title:GetWidth()
        "#,
        )
        .unwrap();
    let long = "A much longer title".len() as f32 * 7.0;
    assert_eq!(right, 10.0 + long);
    assert_eq!(check_left, 10.0 + long + 2.0);
    assert_eq!(width, long);
}
