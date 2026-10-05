-- Optional transitional unit presentation using admitted observations only. This is not a
-- replacement for the stock Wrath UI destination, and a local target is not server confirmation.
local addonName, private = ...
assert(addonName == "AzerothBenchUnits" and type(private) == "table",
    "Wrath addon chunk arguments unavailable")

local ready = false
local elapsed = 0

local function unitText(unit, label)
    if not UnitExists(unit) then
        return label .. ": none"
    end

    local name = UnitName(unit) or "?"
    local health = UnitHealth(unit)
    local healthMax = UnitHealthMax(unit)
    local level = UnitLevel(unit)
    return string.format("%s: %s  HP %d/%d  Level %d", label, name, health, healthMax, level)
end

local function refresh()
    if not AzerothBenchUnitsPlayer or not AzerothBenchUnitsTarget then return end
    if not ready then
        AzerothBenchUnitsPlayer:SetText("Player: unavailable")
        AzerothBenchUnitsTarget:SetText("Selected target: none")
        return
    end

    AzerothBenchUnitsPlayer:SetText(unitText("player", "Player"))
    AzerothBenchUnitsTarget:SetText(unitText("target", "Selected target"))
end

function AzerothBenchUnits_OnLoad(self)
    for _, event in ipairs({ "PLAYER_TARGET_CHANGED", "UNIT_HEALTH", "UNIT_MAXHEALTH",
        "PLAYER_ENTERING_WORLD", "PLAYER_LEAVING_WORLD", "PLAYER_LOGOUT" }) do
        self:RegisterEvent(event)
    end
end

function AzerothBenchUnits_OnEvent(self, event, ...)
    if event == "PLAYER_ENTERING_WORLD" then
        ready = true
        refresh()
    elseif event == "PLAYER_LEAVING_WORLD" or event == "PLAYER_LOGOUT" then
        ready = false
        refresh()
    elseif event == "PLAYER_TARGET_CHANGED" then
        refresh()
    elseif event == "UNIT_HEALTH" or event == "UNIT_MAXHEALTH" then
        local unit = ...
        if unit == "player" or unit == "target" then refresh() end
    end
end

function AzerothBenchUnits_OnUpdate(self, delta)
    elapsed = elapsed + delta
    if elapsed >= 0.25 then
        elapsed = 0
        refresh()
    end
end
