//! A FontString inheriting a virtual FontString template that names no font object.

use crate::framexml;
use crate::loader::*;
use crate::script::{QuadContent, UiScript};

fn no_files(_: &str) -> Option<Vec<u8>> {
    None
}

/// `Blizzard_DebugTools.xml:3-12`: `EventTraceTimeFont` is a virtual `<FontString>` with a font
/// file, and the event buttons' FontString inherits it. That links no font object, and warns of
/// none; the template's file is the region's font.
#[test]
fn a_virtual_fontstring_template_lends_its_file_and_links_no_font_object() {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(800.0, 600.0);
    let doc = framexml::parse(
        r#"<Ui>
            <FontString name="ProbeTimeFont" font="fonts\arialn.ttf" justifyH="RIGHT" virtual="true">
                <FontHeight><AbsValue val="10"/></FontHeight>
            </FontString>
            <Frame name="FT">
                <Size><AbsDimension x="100" y="30"/></Size>
                <Anchors><Anchor point="CENTER"/></Anchors>
                <Layers><Layer level="ARTWORK">
                    <FontString name="$parentTime" inherits="ProbeTimeFont" text="12:00"/>
                </Layer></Layers>
            </Frame>
        </Ui>"#,
    )
    .expect("valid FrameXML");
    let report = load(&s, &doc, &no_files);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    assert!(
        report.warnings.is_empty(),
        "warnings: {:?}",
        report.warnings
    );
    assert!(s
        .eval::<bool>("return FTTime:GetFontObject() == nil")
        .unwrap());
    assert_eq!(
        s.eval::<String>("return FTTime:GetJustifyH()").unwrap(),
        "RIGHT"
    );
    s.resolve();
    assert!(s.extract().into_iter().any(|q| matches!(
        &q.content,
        QuadContent::Text { text: Some(t), font: Some(f), font_height: Some(h), .. }
            if t == "12:00" && f.eq_ignore_ascii_case(r"fonts\arialn.ttf") && *h == 10.0
    )));
}
