-- Demand-loaded read-only companion. It presents copied observer data and records only its
-- own bounded loader/result counters and lifecycle; it has no observation or gameplay source.
local addonName, private = ...
assert(addonName == "AzerothBenchObserverReport" and type(private) == "table",
    "Wrath addon chunk arguments unavailable")

local HISTORY_LIMIT = 16
local function appendBounded(list, value)
    list[#list + 1] = value
    while #list > HISTORY_LIMIT do table.remove(list, 1) end
end

local function database()
    AzerothBenchObserverReportDB = AzerothBenchObserverReportDB or {}
    local db = AzerothBenchObserverReportDB
    db.loadResults = db.loadResults or {}
    db.reasons = db.reasons or {}
    db.events = db.events or {}
    db.refreshes = db.refreshes or 0
    db.available = db.available or 0
    db.unavailable = db.unavailable or 0
    db.lifecycle = db.lifecycle or {}
    -- SavedVariables can contain histories from an earlier, larger configuration.
    while #db.loadResults > HISTORY_LIMIT do table.remove(db.loadResults, 1) end
    while #db.lifecycle > HISTORY_LIMIT do table.remove(db.lifecycle, 1) end
    return db
end

local function setStatus(text)
    if AzerothBenchObserverReportStatus then
        AzerothBenchObserverReportStatus:SetText(text)
    end
end

function private.RecordLoadResult(loaded, reason)
    local db = database()
    local token = loaded and "SUCCESS" or tostring(reason or "UNSUPPORTED")
    db.reasons[token] = (db.reasons[token] or 0) + 1
    appendBounded(db.loadResults, { time = GetTime(), reason = token })
end

function private.Refresh(source)
    local db = database()
    db.refreshes = db.refreshes + 1
    local sample, reason = AzerothBenchObserver.Snapshot()
    if not sample then
        local token = tostring(reason or "UNAVAILABLE")
        db.unavailable = db.unavailable + 1
        db.reasons[token] = (db.reasons[token] or 0) + 1
        appendBounded(db.loadResults, { time = GetTime(), reason = token })
        setStatus("Observer report unavailable: " .. token)
        return nil, token
    end
    db.available = db.available + 1
    setStatus(string.format("%s  HP %d/%d  #%d (%s; %s)", sample.name or "?",
        sample.health, sample.healthMax, sample.sequence, sample.source, source or "refresh"))
    return true
end

function AzerothBenchObserverReport_OnLoad(self)
    self:RegisterEvent("ADDON_LOADED")
    self:RegisterEvent("PLAYER_LEAVING_WORLD")
    self:RegisterEvent("PLAYER_LOGOUT")
end

function AzerothBenchObserverReport_OnEvent(self, event, ...)
    if event == "ADDON_LOADED" and (...) ~= addonName then return end
    local db = database()
    appendBounded(db.lifecycle, { event = event, time = GetTime() })
    db.events[event] = (db.events[event] or 0) + 1
    if event == "ADDON_LOADED" then
        private.Refresh("addon-loaded")
    elseif event == "PLAYER_LEAVING_WORLD" or event == "PLAYER_LOGOUT" then
        -- Independent of handler dispatch order: the event itself invalidates the display.
        setStatus("Observer report unavailable: NOT_READY")
    end
end

function AzerothBenchObserverReport_OnClick(self, button)
    assert(button == "LeftButton", "unexpected click argument")
    private.Refresh("refresh-button")
end

AzerothBenchObserverReport = private
