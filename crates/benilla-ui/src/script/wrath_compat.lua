-- The build-12340 engine API that the mature 1.12 surface lacks, over the state that surface
-- keeps. Systems this client does not have (Battle.net, voice chat, the dungeon finder, arena
-- teams, achievements, the calendar, vehicles, companions) answer as a client with nothing in
-- them would: no entries, not available. `__azb_fire_event` is the host's event dispatch.

local fire = __azb_fire_event
__azb_fire_event = nil

local function none() end
local function zero() return 0 end

-- ── CVars ──────────────────────────────────────────────────────────────────────────────────────
function GetCVarBool(name)
    local value = GetCVar(name)
    if value and value ~= "0" and value ~= "" then return 1 end
    return nil
end
GetCVarMin, GetCVarMax = none, none
GetCVarAbsoluteMin, GetCVarAbsoluteMax = none, none

-- ── action bars ────────────────────────────────────────────────────────────────────────────────
local actionBarPage = 1
function GetActionBarPage() return actionBarPage end
function ChangeActionBarPage(page)
    page = tonumber(page)
    if page and page ~= actionBarPage then
        actionBarPage = page
        fire("ACTIONBAR_PAGE_CHANGED")
    end
end
GetBonusBarOffset = GetBonusBarOffset or zero
GetActionBarToggles = GetActionBarToggles or none

-- ── modified clicks ────────────────────────────────────────────────────────────────────────────
local modifiedClicks = {
    AUTOLOOTTOGGLE = "SHIFT", CHATLINK = "SHIFT", COMPAREITEMS = "SHIFT", DRESSUP = "CTRL",
    FOCUSCAST = "NONE", OPENALLBAGS = "SHIFT", PICKUPACTION = "SHIFT", QUESTWATCHTOGGLE = "SHIFT",
    SELFCAST = "ALT", SHOWITEMFLYOUT = "ALT", SOCKETITEM = "SHIFT", SPLITSTACK = "SHIFT",
    STICKYCAMERA = "CTRL", TOKENWATCHTOGGLE = "SHIFT",
}
local function modifierHeld(modifier)
    local shift, ctrl, alt = IsShiftKeyDown(), IsControlKeyDown(), IsAltKeyDown()
    if modifier == "NONE" then return false end
    if modifier == "SHIFT" then return shift and not ctrl and not alt end
    if modifier == "CTRL" then return ctrl and not shift and not alt end
    if modifier == "ALT" then return alt and not shift and not ctrl end
    local want = {}
    for part in modifier:gmatch("[^%-]+") do want[part] = true end
    return (not not want.SHIFT) == (not not shift) and (not not want.CTRL) == (not not ctrl)
        and (not not want.ALT) == (not not alt)
end
function IsModifiedClick(action)
    if action then
        local modifier = modifiedClicks[action]
        return modifier and modifierHeld(modifier) and 1 or nil
    end
    return (IsShiftKeyDown() or IsControlKeyDown() or IsAltKeyDown()) and 1 or nil
end
function GetModifiedClick(action) return modifiedClicks[action] end
function SetModifiedClick(action, modifier) modifiedClicks[action] = modifier end
function GetModifiedClickAction(index)
    local i = 0
    for action in pairs(modifiedClicks) do
        i = i + 1
        if i == index then return action end
    end
end
function GetNumModifiedClickActions()
    local n = 0
    for _ in pairs(modifiedClicks) do n = n + 1 end
    return n
end
SaveBindings = SaveBindings or none

-- ── macro conditionals ([mod:shift,target=focus] a; b) ─────────────────────────────────────────
local function conditionHolds(condition, target)
    local name, args = condition:match("^%s*(%a+)%s*:?%s*(.-)%s*$")
    if not name then return true end
    name = name:lower()
    local negate = name:sub(1, 2) == "no"
    if negate then name = name:sub(3) end
    local result
    if name == "mod" or name == "modifier" then
        if args == "" then
            result = IsShiftKeyDown() or IsControlKeyDown() or IsAltKeyDown()
        else
            result = false
            for mod in args:gmatch("[^/]+") do
                mod = mod:upper()
                if (mod == "SHIFT" and IsShiftKeyDown()) or (mod == "CTRL" and IsControlKeyDown())
                    or (mod == "ALT" and IsAltKeyDown()) then
                    result = true
                end
            end
        end
    elseif name == "combat" then
        result = UnitAffectingCombat("player")
    elseif name == "exists" then
        result = UnitExists(target)
    elseif name == "dead" then
        result = UnitIsDead(target) or UnitIsGhost(target)
    elseif name == "harm" then
        result = UnitCanAttack("player", target)
    elseif name == "help" then
        result = UnitIsFriend("player", target)
    elseif name == "button" or name == "btn" then
        result = args == "" or args:find("1") ~= nil
    else
        -- Unknown conditions (stance, group, vehicle, ...) do not exclude the clause.
        result = true
    end
    if negate then return not result end
    return result and true or false
end

-- Clauses are tried in order; a clause's bracket groups are alternatives, and every condition
-- in a group must hold. Returns the first matching clause's value and its target unit.
function SecureCmdOptionParse(options)
    for clause in (options .. ";"):gmatch("([^;]*);") do
        local groups, value = {}, clause
        while true do
            local group, after = value:match("^%s*%[(.-)%](.*)$")
            if not group then break end
            groups[#groups + 1], value = group, after
        end
        value = value:match("^%s*(.-)%s*$")
        if #groups == 0 then return value, nil end
        for _, group in ipairs(groups) do
            local target, holds = nil, true
            for condition in (group .. ","):gmatch("([^,]*),") do
                local unit = condition:match("^%s*target%s*=%s*(.-)%s*$")
                    or condition:match("^%s*@%s*(.-)%s*$")
                if unit then
                    target = unit
                elseif condition:match("%S") and not conditionHolds(condition, target or "target") then
                    holds = false
                end
            end
            if holds then return value, target end
        end
    end
end

-- ── casting, mirror timers, runes, totems ──────────────────────────────────────────────────────
UnitCastingInfo = UnitCastingInfo or none
UnitChannelInfo = UnitChannelInfo or none
function GetMirrorTimerInfo() return "UNKNOWN", 0, 0, 0, 0, "" end
GetMirrorTimerProgress = GetMirrorTimerProgress or zero
local runeTypes = { 1, 1, 2, 2, 3, 3 }
function GetRuneType(rune) return runeTypes[rune] end
function GetRuneCooldown() return 0, 0, true end
function GetRuneCount() return 1 end
function GetTotemInfo() return false, "", 0, 0, "" end
GetTotemTimeLeft = zero

-- ── languages ──────────────────────────────────────────────────────────────────────────────────
function GetNumLanguages()
    local n = 0
    while GetLanguageByIndex(n + 1) do n = n + 1 end
    return n
end

-- ── talents, titles, flags ─────────────────────────────────────────────────────────────────────
GetActiveTalentGroup = GetActiveTalentGroup or function() return 1 end
GetNumTalentGroups = GetNumTalentGroups or function() return 1 end
GetCurrentTitle = GetCurrentTitle or function() return -1 end
GetNumTitles = GetNumTitles or zero
GetTitleName = GetTitleName or none
IsTitleKnown = IsTitleKnown or none
UnitIsAFK = UnitIsAFK or none
UnitIsDND = UnitIsDND or none
UnitIsSameServer = UnitIsSameServer or function() return 1 end
UnitIsRaidOfficer = UnitIsRaidOfficer or none
UnitGroupRolesAssigned = UnitGroupRolesAssigned or function() return false, false, false end
UnitThreatSituation = UnitThreatSituation or none
UnitDetailedThreatSituation = UnitDetailedThreatSituation or none
IsXPUserDisabled = IsXPUserDisabled or none
GetAccountExpansionLevel = GetAccountExpansionLevel or function() return 2 end
GetExpansionLevel = GetExpansionLevel or function() return 2 end
GetExistingLocales = GetExistingLocales or function() return "enUS" end
IsStereoVideoAvailable = IsStereoVideoAvailable or none
MovieRecording_IsSupported = MovieRecording_IsSupported or none
IsReferAFriendLinked = IsReferAFriendLinked or none
CanSummonFriend = CanSummonFriend or none
GetOptOutOfLoot = GetOptOutOfLoot or none

-- ── maps and instances ─────────────────────────────────────────────────────────────────────────
InitWorldMapPing = InitWorldMapPing or none
GetCurrentMapDungeonLevel = GetCurrentMapDungeonLevel or zero
GetNumDungeonMapLevels = GetNumDungeonMapLevels or zero
SetDungeonMapLevel = SetDungeonMapLevel or none
function GetInstanceInfo()
    local name = GetRealZoneText and GetRealZoneText() or ""
    local inInstance, kind = false, "none"
    if IsInInstance then inInstance, kind = IsInInstance() end
    return name, kind or "none", 1, "", 0, 0, false
end

-- ── PvP, arena, battlefields ───────────────────────────────────────────────────────────────────
UnitInBattleground = UnitInBattleground or none
GetWorldPVPQueueStatus = GetWorldPVPQueueStatus or none
GetNumWorldPVPAreas = GetNumWorldPVPAreas or zero
GetNumBattlegroundTypes = GetNumBattlegroundTypes or zero
GetBattlegroundInfo = GetBattlegroundInfo or none
IsActiveBattlefieldArena = IsActiveBattlefieldArena or none
GetCurrentArenaSeason = GetCurrentArenaSeason or zero
GetArenaTeam = GetArenaTeam or none
IsArenaTeamCaptain = IsArenaTeamCaptain or none
PlayerIsPVPInactive = PlayerIsPVPInactive or none

-- ── Battle.net, voice chat ─────────────────────────────────────────────────────────────────────
BNFeaturesEnabled, BNFeaturesEnabledAndConnected, BNConnected = none, none, none
BNGetNumFriends = function() return 0, 0 end
BNGetNumFriendInvites, BNGetNumBlocked, BNGetNumBlockedToons = zero, zero, zero
BNGetMaxPlayersInConversation = zero
BNGetNumConversationMembers = zero
BNGetFriendInfo, BNGetFriendInfoByID, BNGetToonInfo, BNGetFriendInviteInfo = none, none, none, none
BNGetConversationInfo, BNGetCustomMessageTable, BNGetInfo = none, none, none
IsVoiceChatEnabled, IsVoiceChatAllowed, IsVoiceChatAllowedByServer = none, none, none
VoiceIsDisabledByClient = function() return 1 end
GetNumVoiceSessions = zero
GetVoiceCurrentSessionID, GetVoiceSessionInfo, GetVoiceStatus = none, none, none
GetSelectedDisplayChannel = none
Sound_ChatSystem_GetNumInputDrivers, Sound_ChatSystem_GetNumOutputDrivers = zero, zero
Sound_ChatSystem_GetInputDriverNameByIndex = none
Sound_ChatSystem_GetOutputDriverNameByIndex = none
Sound_GameSystem_GetNumOutputDrivers = Sound_GameSystem_GetNumOutputDrivers or zero
Sound_GameSystem_GetOutputDriverNameByIndex = none

-- ── dungeon finder, achievements, calendar, companions, vehicles ───────────────────────────────
GetLFGProposal, GetLFGMode, GetLFGRoleUpdate, GetLFGDungeonInfo = none, none, none, none
GetLFGQueuedList, GetLFGInfoServer, GetLFGDungeonRewards = none, none, zero
GetLFGRoles = function() return false, false, false, false end
HasLFGRestrictions, IsPartyLFG, IsLFGDungeonJoinable = none, none, none
GetNumRandomDungeons, GetLFDChoiceOrder = zero, none
GetTrackedAchievements, GetAchievementInfo = none, none
GetNumTrackedAchievements, GetTotalAchievementPoints = zero, zero
CanShowAchievementUI = none
CalendarGetNumPendingInvites = zero
function CalendarGetDate()
    local now = date("*t")
    return now.wday, now.month, now.day, now.year
end
GetNumCompanions, GetCompanionInfo = zero, none
UnitHasVehicleUI, UnitInVehicle, UnitUsingVehicle, CanExitVehicle = none, none, none, none
UnitVehicleSkin, UnitVehicleSeatInfo = none, none
UnitVehicleSeatCount = zero
IsVehicleAimAngleAdjustable, IsVehicleAimPowerAdjustable = none, none

-- ── misc build-12340 queries ───────────────────────────────────────────────────────────────────
IsThreatWarningEnabled = IsThreatWarningEnabled or none
GetSummonFriendCooldown = GetSummonFriendCooldown or function() return 0, 0 end
IsListedInLFR, GetLFGDeserterExpiration, GetLFGRandomCooldownExpiration = none, none, none
GetDungeonDifficulty = GetDungeonDifficulty or function() return 1 end
GetRaidDifficulty = GetRaidDifficulty or function() return 1 end
UnitIsPossessed, IsPossessBarVisible = none, none
GetHonorCurrency = GetHonorCurrency or zero
GetArenaCurrency = GetArenaCurrency or zero
GetActiveVoiceChannel, GetNumVoiceSessionMembersBySessionID = none, zero
CanHearthAndResurrectFromArea = none
DungeonUsesTerrainMap = none

-- ── the stock UI's CVars, at build 12340's defaults, for any the host has not registered ────────
do
    local stock = {
    addFriendInfoShown = "0",
    advancedWorldMap = "0",
    alwaysCompareItems = "0",
    alwaysShowActionBars = "0",
    assistAttack = "0",
    autoClearAFK = "1",
    autoDismountFlying = "0",
    autointeract = "0",
    autojoinBGVoice = "0",
    autojoinPartyVoice = "0",
    autoLootDefault = "0",
    autoQuestProgress = "1",
    autoQuestWatch = "1",
    autoRangedCombat = "1",
    autoSelfCast = "1",
    BaseMip = "0",
    blockTrades = "0",
    buffDurations = "1",
    cameraBobbing = "0",
    cameraDistanceMaxFactor = "1",
    cameraPitchMoveSpeed = "90",
    cameraPitchSmoothSpeed = "45",
    cameraPivot = "0",
    cameraSmoothStyle = "4",
    cameraSmoothTrackingStyle = "4",
    cameraTerrainTilt = "0",
    cameraWaterCollision = "0",
    cameraYawMoveSpeed = "180",
    cameraYawSmoothSpeed = "180",
    ChatAmbienceVolume = "0.3",
    chatBubbles = "1",
    chatBubblesParty = "1",
    chatMouseScroll = "0",
    ChatMusicVolume = "0.3",
    ChatSoundVolume = "0.4",
    chatStyle = "im",
    colorblindMode = "0",
    CombatDamage = "1",
    CombatHealing = "1",
    CombatLogPeriodicSpells = "1",
    combatTextFloatMode = "1",
    componentTextureLevel = "0",
    consolidateBuffs = "1",
    conversationMode = "inline",
    deselectOnClick = "0",
    desktopGamma = "1",
    displayFreeBagSlots = "1",
    displayWorldPVPObjectives = "1",
    enableCombatText = "1",
    EnableMicrophone = "0",
    enablePVPNotifyAFK = "0",
    EnableVoiceChat = "0",
    enableWoWMouse = "0",
    environmentDetail = "1",
    equipmentManager = "0",
    extShadowQuality = "0",
    farclip = "1000",
    fctAuras = "0",
    fctCombatState = "0",
    fctComboPoints = "0",
    fctDamageReduction = "0",
    fctDodgeParryMiss = "0",
    fctEnergyGains = "0",
    fctFriendlyHealers = "0",
    fctHonorGains = "0",
    fctLowManaHealth = "0",
    fctPeriodicEnergyGains = "0",
    fctReactives = "0",
    fctRepChanges = "0",
    fctSpellMechanics = "0",
    fctSpellMechanicsOther = "0",
    ffxDeath = "0",
    ffxGlow = "0",
    fullSizeFocusFrame = "0",
    groundEffectDensity = "16",
    groundEffectDist = "70",
    guildMemberNotify = "1",
    guildRecruitmentChannel = "1",
    gxCursor = "0",
    gxFixLag = "0",
    gxMaximize = "0",
    gxRefresh = "0",
    gxStereoConvergence = "0",
    gxStereoEnabled = "0",
    gxStereoSeparation = "0",
    gxTripleBuffer = "0",
    gxVSync = "0",
    gxWindow = "1",
    hidePartyInRaid = "0",
    InboundChatVolume = "1",
    iTunesRemoteFeedback = "0",
    iTunesTrackDisplay = "0",
    lastTalkedToGM = "",
    lfgDebug = "0",
    locale = "enUS",
    lockActionBars = "1",
    lootUnderMouse = "0",
    mapQuestDifficulty = "0",
    miniWorldMap = "0",
    mouseInvertPitch = "0",
    mouseSpeed = "1",
    MovieRecordingAutoCompress = "0",
    MovieRecordingCompression = "0",
    MovieRecordingCursor = "0",
    MovieRecordingFramerate = "29.97",
    MovieRecordingGUI = "0",
    MovieRecordingIcon = "0",
    MovieRecordingQuality = "2",
    MovieRecordingRecover = "0",
    MovieRecordingSound = "0",
    MovieRecordingWidth = "1280",
    movieSubtitle = "0",
    nameplateAllowOverlap = "1",
    nameplateShowEnemies = "0",
    nameplateShowEnemyGuardians = "0",
    nameplateShowEnemyPets = "0",
    nameplateShowEnemyTotems = "0",
    nameplateShowFriendlyGuardians = "0",
    nameplateShowFriendlyPets = "0",
    nameplateShowFriendlyTotems = "0",
    nameplateShowFriends = "0",
    OutboundChatVolume = "1",
    particleDensity = "1",
    partyBackgroundOpacity = "0.5",
    partyStatusText = "0",
    pendingInviteInfoShown = "0",
    PetMeleeDamage = "1",
    PetSpellDamage = "1",
    petStatusText = "0",
    playerStatLeftDropdown = "PLAYERSTAT_BASE_STATS",
    playerStatRightDropdown = "PLAYERSTAT_MELEE_COMBAT",
    playerStatusText = "0",
    predictedHealth = "0",
    predictedPower = "0",
    previewTalents = "0",
    profanityFilter = "1",
    projectedTextures = "0",
    PushToTalkSound = "0",
    questFadingDisable = "0",
    questPOI = "1",
    removeChatDelay = "0",
    rotateMinimap = "0",
    screenEdgeFlash = "1",
    scriptErrors = "0",
    secureAbilityToggle = "1",
    ShowAllSpellRanks = "1",
    showArenaEnemyCastbar = "1",
    showArenaEnemyFrames = "1",
    showArenaEnemyPets = "1",
    showBattlefieldMinimap = "1",
    showCastableBuffs = "0",
    showCastableDebuffs = "0",
    ShowClassColorInNameplate = "0",
    showClock = "1",
    showDispelDebuffs = "1",
    showGameTips = "1",
    showItemLevel = "0",
    showKeyring = "0",
    showLootSpam = "1",
    showNewbieTips = "1",
    showPartyBackground = "0",
    showPartyPets = "1",
    showRaidRange = "0",
    showTargetCastbar = "1",
    showTargetOfTarget = "0",
    showTimestamps = "none",
    showToastBroadcast = "1",
    showToastConversation = "1",
    showToastFriendRequest = "1",
    showToastOffline = "1",
    showToastOnline = "1",
    showToastWindow = "1",
    showTokenFrame = "0",
    showTokenFrameHonor = "0",
    showTutorials = "1",
    showVKeyCastbar = "1",
    Sound_AmbienceVolume = "0.6",
    Sound_EnableAllSound = "1",
    Sound_EnableAmbience = "1",
    Sound_EnableDSPEffects = "0",
    Sound_EnableEmoteSounds = "0",
    Sound_EnableErrorSpeech = "0",
    Sound_EnableHardware = "0",
    Sound_EnableMusic = "1",
    Sound_EnablePetSounds = "0",
    Sound_EnableReverb = "0",
    Sound_EnableSFX = "1",
    Sound_EnableSoftwareHRTF = "0",
    Sound_EnableSoundWhenGameIsInBG = "0",
    Sound_MasterVolume = "1",
    Sound_MusicVolume = "0.4",
    Sound_NumChannels = "32",
    Sound_OutputDriverIndex = "0",
    Sound_OutputQuality = "1",
    Sound_SFXVolume = "1",
    Sound_VoiceChatInputDriverIndex = "0",
    Sound_VoiceChatOutputDriverIndex = "0",
    Sound_ZoneMusicNoDelay = "0",
    spamFilter = "1",
    specular = "0",
    statusTextPercentage = "0",
    stopAutoAttackOnTargetChange = "0",
    targetOfTargetMode = "5",
    targetStatusText = "0",
    textureFilteringMode = "0",
    threatPlaySounds = "1",
    threatShowNumeric = "0",
    threatWarning = "3",
    timeMgrAlarmEnabled = "0",
    timeMgrUseLocalTime = "0",
    timeMgrUseMilitaryTime = "0",
    toastDuration = "0",
    trackerFilter = "0",
    trackerSorting = "proximity",
    UberTooltips = "1",
    uiscale = "1",
    UnitNameEnemyGuardianName = "0",
    UnitNameEnemyPetName = "0",
    UnitNameEnemyPlayerName = "1",
    UnitNameEnemyTotemName = "0",
    UnitNameFriendlyGuardianName = "0",
    UnitNameFriendlyPetName = "0",
    UnitNameFriendlyPlayerName = "1",
    UnitNameFriendlyTotemName = "0",
    UnitNameNonCombatCreatureName = "0",
    UnitNameNPC = "0",
    UnitNameOwn = "0",
    UnitNamePlayerGuild = "1",
    UnitNamePlayerPVPTitle = "1",
    useEnglishAudio = "0",
    useUIScale = "0",
    VoiceActivationSensitivity = "0.4",
    VoiceChatMode = "0",
    VoiceChatSelfMute = "0",
    watchFrameWidth = "0",
    weatherDensity = "2",
    wholeChatWindowClickable = "1",
    windowResizeLock = "0",
    worldMapOpacity = "0",
    xpBarText = "0",
    }
    for name, default in pairs(stock) do
        RegisterCVar(name, default)
    end
end

-- ── class names, action and pet-bar predicates, group queries, chat window geometry ───────────
do
    local male = {
        WARRIOR = "Warrior", PALADIN = "Paladin", HUNTER = "Hunter", ROGUE = "Rogue",
        PRIEST = "Priest", DEATHKNIGHT = "Death Knight", SHAMAN = "Shaman", MAGE = "Mage",
        WARLOCK = "Warlock", DRUID = "Druid",
    }
    function FillLocalizedClassList(t, female)
        for token, name in pairs(male) do t[token] = name end
        return t
    end
end
IsStackableAction = IsStackableAction or IsConsumableAction or none
IsPetAttackAction = IsPetAttackAction or none
GetPossessInfo = GetPossessInfo or none
GetReadyCheckStatus = GetReadyCheckStatus or none
GetNumDisplayChannels = GetNumDisplayChannels or zero
GetPreviousArenaSeason = GetPreviousArenaSeason or zero
GetAvailableRoles = GetAvailableRoles or function() return true, true, true end
do
    local dimensions, positions = {}, {}
    function GetChatWindowSavedDimensions(id)
        local d = dimensions[id]
        if d then return d[1], d[2] end
    end
    function SetChatWindowSavedDimensions(id, width, height) dimensions[id] = { width, height } end
    function GetChatWindowSavedPosition(id)
        local p = positions[id]
        if p then return p[1], p[2], p[3] end
    end
    function SetChatWindowSavedPosition(id, point, x, y) positions[id] = { point, x, y } end
end

-- ── the next batch: static constants, multi-cast bar, bags, world map, achievements ────────────
RegisterStaticConstants = RegisterStaticConstants or none
GetMultiCastBarOffset = GetMultiCastBarOffset or zero
HasCompletedAnyAchievement = HasCompletedAnyAchievement or none
GetChannelDisplayInfo = GetChannelDisplayInfo or none
IsZoomOutAvailable = IsZoomOutAvailable or none
IsPlayerResolutionAvailable = IsPlayerResolutionAvailable or function() return 1 end
CanPartyLFGBackfill = CanPartyLFGBackfill or none
function GetContainerNumFreeSlots(bag)
    local free = 0
    for slot = 1, GetContainerNumSlots(bag) or 0 do
        if not GetContainerItemInfo(bag, slot) then free = free + 1 end
    end
    return free, 0
end

-- ── vehicles, tutorials, guild log, friends ────────────────────────────────────────────────────
VehicleExit, VehicleAimUpStart, VehicleAimUpStop = none, none, none
VehicleAimDownStart, VehicleAimDownStop = none, none
VehicleAimIncrement, VehicleAimDecrement, VehicleAimRequestAngle = none, none, none
VehicleNextSeat, VehiclePrevSeat, CanSwitchVehicleSeats = none, none, none
TriggerTutorial = TriggerTutorial or none
HasDebugZoneMap = HasDebugZoneMap or none
QueryGuildEventLog = QueryGuildEventLog or none
RefreshLFGList = RefreshLFGList or none
do
    -- Wrath's GetNumFriends answers the online count too.
    local numFriends = GetNumFriends
    function GetNumFriends()
        local total, online = numFriends() or 0, 0
        for i = 1, total do
            local _, _, _, _, connected = GetFriendInfo(i)
            if connected then online = online + 1 end
        end
        return total, online
    end
end
UnitIsTalking = UnitIsTalking or none
GetNumMapDebugObjects = GetNumMapDebugObjects or zero

-- ── the combat log: an empty log, and the engine's unit-flag constants ─────────────────────────
CombatLogResetFilter, CombatLogAddFilter, CombatLogClearEntries = none, none, none
CombatLogSetCurrentEntry, CombatLogAdvanceEntry, CombatLogSetRetentionTime = none, none, none
CombatLogGetNumEntries, CombatLogGetCurrentEntry = zero, none
CombatLogGetRetentionTime = function() return 300 end
CombatLog_Object_IsA = function(flags, mask)
    return bit.band(flags or 0, mask or 0) ~= 0 and 1 or nil
end
GetCurrentCombatTextEventInfo = GetCurrentCombatTextEventInfo or none
COMBATLOG_OBJECT_AFFILIATION_MINE, COMBATLOG_OBJECT_AFFILIATION_PARTY = 0x1, 0x2
COMBATLOG_OBJECT_AFFILIATION_RAID, COMBATLOG_OBJECT_AFFILIATION_OUTSIDER = 0x4, 0x8
COMBATLOG_OBJECT_AFFILIATION_MASK = 0xF
COMBATLOG_OBJECT_REACTION_FRIENDLY, COMBATLOG_OBJECT_REACTION_NEUTRAL = 0x10, 0x20
COMBATLOG_OBJECT_REACTION_HOSTILE, COMBATLOG_OBJECT_REACTION_MASK = 0x40, 0xF0
COMBATLOG_OBJECT_CONTROL_PLAYER, COMBATLOG_OBJECT_CONTROL_NPC = 0x100, 0x200
COMBATLOG_OBJECT_CONTROL_MASK = 0x300
COMBATLOG_OBJECT_TYPE_PLAYER, COMBATLOG_OBJECT_TYPE_NPC = 0x400, 0x800
COMBATLOG_OBJECT_TYPE_PET, COMBATLOG_OBJECT_TYPE_GUARDIAN = 0x1000, 0x2000
COMBATLOG_OBJECT_TYPE_OBJECT, COMBATLOG_OBJECT_TYPE_MASK = 0x4000, 0xFC00
COMBATLOG_OBJECT_TARGET, COMBATLOG_OBJECT_FOCUS = 0x10000, 0x20000
COMBATLOG_OBJECT_MAINTANK, COMBATLOG_OBJECT_MAINASSIST = 0x40000, 0x80000
for i = 1, 8 do _G["COMBATLOG_OBJECT_RAIDTARGET" .. i] = 2 ^ (19 + i) end
COMBATLOG_OBJECT_RAIDTARGET_MASK = 0x0FF00000
COMBATLOG_OBJECT_NONE, COMBATLOG_OBJECT_SPECIAL_MASK = 0x80000000, 0xFFFF0000
IsPVPTimerRunning = IsPVPTimerRunning or none
debuginfo = debuginfo or none
CombatTextSetActiveUnit = CombatTextSetActiveUnit or none

-- ── unit selection colour (the target frame's name background) ─────────────────────────────────
do
    local colors = {
        [1] = { 1, 0, 0 }, [2] = { 1, 0, 0 }, [3] = { 1, 0.5, 0 }, [4] = { 1, 1, 0 },
        [5] = { 0, 1, 0 }, [6] = { 0, 1, 0 }, [7] = { 0, 1, 0 }, [8] = { 0, 1, 0 },
    }
    function UnitSelectionColor(unit)
        if UnitIsPlayer(unit) and UnitIsFriend("player", unit) then return 0, 0, 1, 1 end
        local c = colors[UnitReaction(unit, "player") or 4] or colors[4]
        return c[1], c[2], c[3], 1
    end
end
UnitIsTappedByAllThreatList = UnitIsTappedByAllThreatList or none

-- ── the secure templates' remaining helpers ────────────────────────────────────────────────────
SpellCanTargetItem, SpellTargetItem = none, none
FocusUnit, ClearFocus = FocusUnit or none, ClearFocus or none
IsEquippableItem, IsEquippedItem = IsEquippableItem or none, IsEquippedItem or none
EquipItemByName, PickupItem = EquipItemByName or none, PickupItem or none
PickupCompanion, PickupEquipmentSet, SetMultiCastSpell = none, none, none
ClearOverrideBindings, SetOverrideBinding, SetOverrideBindingClick = none, none, none
SetOverrideBindingItem, SetOverrideBindingMacro, SetOverrideBindingSpell = none, none, none
GetCursorInfo = GetCursorInfo or none

-- ── gossip and the quest panels ────────────────────────────────────────────────────────────────
ForceGossip = ForceGossip or function() return nil end
QuestFlagsPVP = QuestFlagsPVP or function() return nil end
GetNumQuestItemDrops = GetNumQuestItemDrops or function() return 0 end
GetQuestLogItemDrop = GetQuestLogItemDrop or none
GetContainerItemQuestInfo = GetContainerItemQuestInfo or function() return nil, nil, nil end
GetDailyQuestsCompleted = GetDailyQuestsCompleted or function() return 0 end
GetMaxDailyQuests = GetMaxDailyQuests or function() return 25 end
GetQuestResetTime = GetQuestResetTime or function() return 0 end
GetQuestLink = GetQuestLink or none
GetNumQuestLogRewardFactions = GetNumQuestLogRewardFactions or function() return 0 end
GetQuestLogRewardFactionInfo = GetQuestLogRewardFactionInfo or none
ProcessQuestLogRewardFactions = ProcessQuestLogRewardFactions or none
GetQuestLogSpecialItemInfo = GetQuestLogSpecialItemInfo or none
GetQuestLogSpecialItemCooldown = GetQuestLogSpecialItemCooldown or none
IsQuestLogSpecialItemInRange = IsQuestLogSpecialItemInRange or none
UseQuestLogSpecialItem = UseQuestLogSpecialItem or none
GetQuestLogSpellLink, GetQuestSpellLink = GetQuestLogSpellLink or none, GetQuestSpellLink or none
GetQuestSortIndex = GetQuestSortIndex or function() return 0 end
GetQuestWorldMapAreaID = GetQuestWorldMapAreaID or function() return 0 end
SortQuestWatches = SortQuestWatches or none
QuestMapUpdateAllQuests = QuestMapUpdateAllQuests or zero
QuestPOIUpdateIcons = QuestPOIUpdateIcons or none
QuestPOIGetIconInfo = QuestPOIGetIconInfo or none
QuestPOIGetQuestIDByVisibleIndex = QuestPOIGetQuestIDByVisibleIndex or none
GetQuestPOILeaderBoard = GetQuestPOILeaderBoard or none

-- ── 3.x movement binding names ────────────────────────────────────────────────────────────────
-- `Bindings.xml` calls the flying-era names; on the ground they are 1.12's jump and sit.
JumpOrAscendStart = JumpOrAscendStart or Jump
AscendStop = AscendStop or none
SitStandOrDescendStart = SitStandOrDescendStart or SitOrStand
DescendStop = DescendStop or none
