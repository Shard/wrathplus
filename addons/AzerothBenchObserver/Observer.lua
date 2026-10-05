-- Authored benchmark addon: ordinary Wrath APIs only. This is an observation modality,
-- not baseline visual-only information, a policy, privileged scorer or game controller.
local addonName, private = ...
assert(addonName == "AzerothBenchObserver" and type(private) == "table",
    "Wrath addon chunk arguments unavailable")
private.events = {}
local samples = {}
local copySample
private.sequence = 0
private.updates = 0
private.elapsed = 0
private.ready = false

-- File-scope reads pin seed-before-addon ordering. Strings are host registry
-- values, not effective consumer measurements or a stock Wrath default census.
private.presentationAtLoad = {
    uiScale = GetCVar("uiScale"),
    gamma = GetCVar("gamma"),
    mouseInvertPitch = GetCVar("mouseInvertPitch"),
    mousespeed = GetCVar("mousespeed"),
}
for _, name in ipairs({ "uiScale", "gamma", "mouseInvertPitch", "mousespeed" }) do
    assert(type(private.presentationAtLoad[name]) == "string",
        "unseeded host presentation CVar: " .. name)
end
assert(GetCVar("realmList") == nil and GetCVar("lastCharacterIndex") == nil,
    "internal host CVar exposed")
assert(SetCVar == nil and RegisterCVar == nil and GetCVarDefault == nil,
    "unsupported CVar API exposed")

local function record(event)
    private.events[#private.events + 1] = { event = event, time = GetTime() }
    if #private.events > 64 then table.remove(private.events, 1) end
end

local function observation(source)
    if not private.ready then return end
    if not UnitExists("player") then samples = {}; return end
    private.sequence = private.sequence + 1
    local version, build = GetBuildInfo()
    local sample = {
        schema = "azerothbench.player-visible.v1",
        sequence = private.sequence,
        timestamp = GetTime(),
        source = source,
        modality = "declared-addon-player-state",
        name = UnitName("player"),
        guid = UnitGUID("player"),
        -- Predicate normalization is addon-owned; identity comes from the host's
        -- existing unit resolver, never a target-setting or hidden-unit lookup.
        playerIsSelf = not not UnitIsUnit("player", "player"),
        health = UnitHealth("player"),
        healthMax = UnitHealthMax("player"),
        level = UnitLevel("player"),
        clientVersion = version,
        clientBuild = build,
        presentation = {
            uiScale = GetCVar("uiScale"),
            gamma = GetCVar("gamma"),
            mouseInvertPitch = GetCVar("mouseInvertPitch"),
            mousespeed = GetCVar("mousespeed"),
        },
    }
    samples[#samples + 1] = sample
    -- Bounded local history, never a server/evaluator write.
    if #samples > 32 then table.remove(samples, 1) end
    if source == "click" then
        AzerothBenchObserverDB.clicks = (AzerothBenchObserverDB.clicks or 0) + 1
    end
    AzerothBenchObserverDB.last = copySample(sample)
    AzerothBenchObserverDB.sequence = private.sequence
    if AzerothBenchObserverStatus then
        AzerothBenchObserverStatus:SetText(string.format("%s  HP %d/%d  #%d (%s)",
            sample.name or "?", sample.health, sample.healthMax, sample.sequence, source))
    end
end

copySample = function(sample)
    if not sample then return nil end
    return {
        schema = sample.schema,
        sequence = sample.sequence,
        timestamp = sample.timestamp,
        source = sample.source,
        modality = sample.modality,
        name = sample.name,
        guid = sample.guid,
        playerIsSelf = sample.playerIsSelf,
        health = sample.health,
        healthMax = sample.healthMax,
        level = sample.level,
        clientVersion = sample.clientVersion,
        clientBuild = sample.clientBuild,
        presentation = {
            uiScale = sample.presentation.uiScale,
            gamma = sample.presentation.gamma,
            mouseInvertPitch = sample.presentation.mouseInvertPitch,
            mousespeed = sample.presentation.mousespeed,
        },
    }
end

-- Returned samples are bounded addon-owned copies. Stale observations are not exposed while
-- the player is outside the admitted world lifecycle.
function private.Snapshot()
    if not private.ready then return nil, "NOT_READY" end
    if not UnitExists("player") then samples = {}; return nil, "NO_PLAYER" end
    local sample = copySample(samples[#samples])
    if not sample then return nil, "NO_SAMPLE" end
    return sample
end

function private.History()
    if not private.ready then return nil, "NOT_READY" end
    if not UnitExists("player") then samples = {}; return nil, "NO_PLAYER" end
    local history = {}
    for i, sample in ipairs(samples) do
        history[i] = copySample(sample)
    end
    return history
end

function AzerothBenchObserver_OnEvent(self, event, ...)
    record(event)
    if event == "ADDON_LOADED" then
        local name = ...
        if name ~= addonName then return end
        AzerothBenchObserverDB = AzerothBenchObserverDB or {}
        AzerothBenchObserverDB.loads = (AzerothBenchObserverDB.loads or 0) + 1
    elseif event == "PLAYER_LOGIN" then
        private.login = true
    elseif event == "PLAYER_ENTERING_WORLD" then
        assert(private.login, "PLAYER_ENTERING_WORLD preceded PLAYER_LOGIN")
        samples = {}
        private.ready = true
        observation("enter-world")
    elseif event == "PLAYER_LEAVING_WORLD" then
        private.ready = false
        samples = {}
    elseif event == "UNIT_HEALTH" or event == "UNIT_MAXHEALTH" then
        local unit = ...
        if unit == "player" then observation(event) end
    elseif event == "PLAYER_LOGOUT" then
        private.ready = false
        samples = {}
        AzerothBenchObserverDB.events = private.events
    end
end

function AzerothBenchObserver_OnLoad(self)
    for _, event in ipairs({ "ADDON_LOADED", "PLAYER_LOGIN", "PLAYER_ENTERING_WORLD",
        "PLAYER_LEAVING_WORLD", "PLAYER_LOGOUT", "UNIT_HEALTH", "UNIT_MAXHEALTH" }) do
        self:RegisterEvent(event)
    end
end

function AzerothBenchObserver_OnUpdate(self, elapsed)
    private.updates = private.updates + 1
    private.elapsed = private.elapsed + elapsed
    if private.elapsed >= 5 then
        private.elapsed = 0
        observation("periodic")
    end
end

function AzerothBenchObserver_OnClick(self, button)
    assert(button == "LeftButton", "unexpected click argument")
    observation("click")
end

function AzerothBenchObserver_LoadReport(self, button)
    assert(button == "LeftButton", "unexpected click argument")
    local loaded, reason = LoadAddOn("AzerothBenchObserverReport")
    if loaded and AzerothBenchObserverReport and AzerothBenchObserverReport.RecordLoadResult then
        AzerothBenchObserverReport.RecordLoadResult(loaded, reason)
        AzerothBenchObserverReport.Refresh("load-button")
    elseif AzerothBenchObserverStatus then
        AzerothBenchObserverStatus:SetText("Report unavailable: " .. tostring(reason or "UNSUPPORTED"))
    end
end

-- An ordinary addon-owned table for external, explicitly declared observation adapters.
-- No client memory/realm/score/reset access and no synthetic authoritative fields.
AzerothBenchObserver = private
