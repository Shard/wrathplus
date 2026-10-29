//! Traverse crossed animation boundaries, including ticks spanning multiple loop legs.
use super::fire;
use mlua::Table;
use std::collections::BTreeMap;

pub(super) fn advance(
    g: &Table,
    animations: &[Table],
    slots: &BTreeMap<u32, f64>,
    duration: f64,
    previous: f64,
    time: f64,
    looping: &str,
) -> mlua::Result<()> {
    let fresh = g.raw_get::<Option<bool>>("_fresh")?.unwrap_or(false);
    g.raw_set("_fresh", false)?;
    let first = if duration > 0.0 {
        (previous / duration).floor() as u64
    } else {
        0
    };
    let last = if duration > 0.0 && looping != "NONE" {
        (time / duration).floor() as u64
    } else {
        0
    };
    for leg in first..=last {
        let origin = leg as f64 * duration;
        let reverse = looping == "BOUNCE" && leg % 2 == 1;
        let mut events = Vec::new();
        for (i, a) in animations.iter().enumerate() {
            let order = a.raw_get::<f64>("_order")? as u32;
            let start = slots.range(..order).map(|(_, v)| v).sum::<f64>()
                + a.raw_get::<f64>("_start_delay")?;
            let end = start + a.raw_get::<f64>("_duration")?.max(0.0);
            let (start, end) = if reverse {
                (duration - end, duration - start)
            } else {
                (start, end)
            };
            events.push((origin + start, i, "OnPlay"));
            events.push((origin + end, i, "OnFinished"));
        }
        events.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.cmp(&b.1))
                .then_with(|| (a.2 == "OnFinished").cmp(&(b.2 == "OnFinished")))
        });
        for (at, i, event) in events {
            if at > time
                || !(at > previous || (fresh && at == previous) || (leg > first && at == origin))
            {
                continue;
            }
            fire(&animations[i], event)?;
            if !g.raw_get::<bool>("_playing")? {
                return Ok(());
            }
        }
        let boundary = origin + duration;
        if looping != "NONE" && duration > 0.0 && boundary > previous && boundary <= time {
            fire(g, "OnLoop")?;
            if !g.raw_get::<bool>("_playing")? {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;
    #[test]
    fn individual_orders_delays_and_repeat_crossings_fire_once() {
        let mut s = UiScript::new().unwrap();
        s.run("f=CreateFrame('Frame');g=f:CreateAnimationGroup();a=g:CreateAnimation();a:SetDuration(1);b=g:CreateAnimation();b:SetOrder(2);b:SetStartDelay(.5);b:SetDuration(1);events='';for i,v in ipairs({a,b}) do v:SetScript('OnPlay',function() events=events..i..'p' end);v:SetScript('OnFinished',function() events=events..i..'f' end) end;g:Play()").unwrap();
        s.tick(1.25);
        assert_eq!(s.eval::<String>("return events").unwrap(), "1p1f");
        s.tick(0.5);
        assert_eq!(s.eval::<String>("return events").unwrap(), "1p1f2p");
        s.tick(0.75);
        assert_eq!(s.eval::<String>("return events").unwrap(), "1p1f2p2f");
        s.run("events='';g:SetLooping('REPEAT');g:Play()").unwrap();
        s.tick(5.0);
        assert_eq!(
            s.eval::<String>("return events").unwrap(),
            "1p1f2p2f1p1f2p2f1p"
        );
    }
    #[test]
    fn bounce_exact_boundaries_pause_and_child_play_do_not_duplicate() {
        let mut s = UiScript::new().unwrap();
        s.run("f=CreateFrame('Frame');g=f:CreateAnimationGroup();a=g:CreateAnimation();a:SetDuration(1);events='';a:SetScript('OnPlay',function() events=events..'p' end);a:SetScript('OnFinished',function() events=events..'f' end);g:SetLooping('BOUNCE');g:SetScript('OnLoop',function() events=events..'l' end);a:Play()").unwrap();
        s.tick(1.0);
        assert_eq!(s.eval::<String>("return events").unwrap(), "pflp");
        s.tick(0.25);
        assert_eq!(s.eval::<String>("return events").unwrap(), "pflp");
        s.run("g:Pause()").unwrap();
        s.tick(3.0);
        s.run("g:Play()").unwrap();
        s.tick(0.75);
        assert_eq!(s.eval::<String>("return events").unwrap(), "pflpflp");
        s.tick(0.25);
        assert_eq!(s.eval::<String>("return events").unwrap(), "pflpflp");
    }

    #[test]
    fn smooth_progress_and_stop_animating() {
        let mut s = UiScript::new().unwrap();
        s.run("f=CreateFrame('Frame');t=f:CreateTexture();g=t:CreateAnimationGroup();a=g:CreateAnimation('Alpha');a:SetDuration(1);a:SetSmoothing('IN');stops=0;g:SetScript('OnStop',function() stops=stops+1 end);g:Play()").unwrap();
        s.tick(0.5);
        assert!(s.eval::<bool>("return math.abs(a:GetProgress()-0.5)<1e-6 and math.abs(a:GetSmoothProgress()-0.25)<1e-6").unwrap());
        s.run("t:StopAnimating(); t:StopAnimating(); f:StopAnimating()")
            .unwrap();
        assert!(s
            .eval::<bool>("return stops==1 and not g:IsPlaying()")
            .unwrap());
    }

    #[test]
    fn child_callback_can_stop_without_later_callbacks_or_group_finish() {
        let mut s = UiScript::new().unwrap();
        s.run("f=CreateFrame('Frame');g=f:CreateAnimationGroup();a=g:CreateAnimation();a:SetDuration(1);b=g:CreateAnimation();b:SetOrder(2);b:SetDuration(1);events='';a:SetScript('OnFinished',function() events=events..'a';g:Stop() end);b:SetScript('OnPlay',function() events=events..'b' end);g:SetScript('OnFinished',function() events=events..'g' end);g:Play()").unwrap();
        s.tick(3.0);
        assert_eq!(s.eval::<String>("return events").unwrap(), "a");
    }
}
