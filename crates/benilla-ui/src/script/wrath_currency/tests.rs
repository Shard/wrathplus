use super::*;
use crate::script::ScriptDialect;

fn script() -> UiScript {
    UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap()
}

#[test]
fn empty_defaults_and_vanilla_surface() {
    script()
        .run(
            r#"
        assert(GetCurrencyListSize() == 0)
        for _, i in ipairs({-1, 0, 1, 4}) do
            assert(GetCurrencyListInfo(i) == nil)
            assert(GetBackpackCurrencyInfo(i) == nil)
            ExpandCurrencyList(i, 1); SetCurrencyUnused(i, 1); SetCurrencyBackpack(i, 1)
        end
        local a,b = GetHonorCurrency(); assert(a == 0 and b == 0)
        a,b = GetArenaCurrency(); assert(a == 0 and b == 0)
        assert(select('#', GetHonorCurrency()) == 2)
        assert(select('#', GetArenaCurrency()) == 2)
    "#,
        )
        .unwrap();
    UiScript::new()
        .unwrap()
        .run(
            r#"
        assert(GetCurrencyListSize == nil and GetCurrencyListInfo == nil)
        assert(GetBackpackCurrencyInfo == nil and ExpandCurrencyList == nil)
        assert(SetCurrencyUnused == nil and SetCurrencyBackpack == nil)
        assert(GetHonorCurrency == nil and GetArenaCurrency == nil)
    "#,
        )
        .unwrap();
}

fn snapshot() -> WrathCurrencySnapshot {
    let mut entries = vec![WrathCurrencyEntry {
        name: "Test category".into(),
        is_header: true,
        is_expanded: true,
        ..Default::default()
    }];
    for i in 1..=4 {
        entries.push(WrathCurrencyEntry {
            name: format!("Test token {i}"),
            count: i * 10,
            item_id: 100 + i,
            icon: format!("test-icon-{i}"),
            extra_currency_type: 0,
            ..Default::default()
        });
    }
    WrathCurrencySnapshot {
        entries,
        honor: (42, 100),
        arena: (12, 50),
    }
}

#[test]
fn supplied_snapshot_shapes_and_clear() {
    let mut s = script();
    s.set_wrath_currency_snapshot(snapshot());
    s.run(
        r#"
        assert(GetCurrencyListSize() == 5)
        assert(select('#', GetCurrencyListInfo(2)) == 9)
        local name,h,e,u,w,count,kind,icon,id = GetCurrencyListInfo(2)
        assert(name == 'Test token 1' and h == false and e == false)
        assert(u == false and w == false and count == 10 and kind == 0)
        assert(icon == 'test-icon-1' and id == 101)
        SetCurrencyBackpack(2, 1)
        assert(select('#', GetBackpackCurrencyInfo(1)) == 5)
        name,count,kind,icon,id = GetBackpackCurrencyInfo(1)
        assert(name == 'Test token 1' and count == 10 and kind == 0)
        assert(icon == 'test-icon-1' and id == 101)
        local a,b = GetHonorCurrency(); assert(a == 42 and b == 100)
        a,b = GetArenaCurrency(); assert(a == 12 and b == 50)
    "#,
    )
    .unwrap();
    s.set_wrath_currency_snapshot(Default::default());
    s.run("assert(GetCurrencyListSize()==0 and GetBackpackCurrencyInfo(1)==nil) local a,b=GetHonorCurrency() assert(a==0 and b==0)").unwrap();
}

#[test]
fn numeric_flags_visible_indexes_and_three_watch_slots() {
    let mut s = script();
    s.set_wrath_currency_snapshot(snapshot());
    s.run(
        r#"
        SetCurrencyUnused(2, 1); assert(select(4,GetCurrencyListInfo(2)))
        SetCurrencyUnused(2, 0); assert(not select(4,GetCurrencyListInfo(2)))
        for i=2,5 do SetCurrencyBackpack(i, 1) end
        assert(GetBackpackCurrencyInfo(3) == 'Test token 3')
        assert(GetBackpackCurrencyInfo(4) == nil)
        assert(not select(5,GetCurrencyListInfo(5)))
        SetCurrencyBackpack(2, 0); SetCurrencyBackpack(5, true)
        assert(GetBackpackCurrencyInfo(3) == 'Test token 4')
        ExpandCurrencyList(1, 0); assert(GetCurrencyListSize() == 1)
        assert(GetCurrencyListInfo(2) == nil)
        assert(GetBackpackCurrencyInfo(1) == 'Test token 2')
        SetCurrencyBackpack(1, 1); SetCurrencyUnused(1, 1)
        assert(not select(4,GetCurrencyListInfo(1)))
        ExpandCurrencyList(1, 1); assert(GetCurrencyListSize() == 5)
        ExpandCurrencyList(2, 0); assert(GetCurrencyListSize() == 5)
        assert(GetCurrencyListInfo(-9223372036854775808) == nil)
        assert(GetBackpackCurrencyInfo(-9223372036854775808) == nil)
    "#,
    )
    .unwrap();
}

#[test]
fn supplied_watch_flags_are_bounded_and_headers_cannot_be_watched() {
    let mut s = script();
    let mut data = snapshot();
    for row in &mut data.entries {
        row.is_watched = true;
    }
    s.set_wrath_currency_snapshot(data);
    s.run("assert(not select(5,GetCurrencyListInfo(1))) assert(not select(5,GetCurrencyListInfo(5))) assert(GetBackpackCurrencyInfo(3)=='Test token 3') assert(GetBackpackCurrencyInfo(4)==nil)").unwrap();
}
