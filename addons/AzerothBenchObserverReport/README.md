# AzerothBench Observer Report

An optional authored Wrath 3.3.5a (`Interface: 30300`) companion for `AzerothBenchObserver`. Install both sibling directories under the client's local-state AddOns root. Do not copy either addon into the read-only WoW install.

The TOC deliberately declares `Dependencies: AzerothBenchObserver`, `LoadOnDemand: 1`, and character-scoped `SavedVariablesPerCharacter: AzerothBenchObserverReportDB`. The ordinary `AzerothBenchObserverReportButton` calls `LoadAddOn("AzerothBenchObserverReport")`; direct callers may make the same ordinary API call. A missing, disabled, incompatible, or otherwise unsupported package remains unavailable with the loader's reason token rather than a fabricated report.

The companion consumes only `AzerothBenchObserver.Snapshot()`, which supplies a copied current sample. It renders its own `AzerothBenchObserverReportFrame`, `AzerothBenchObserverReportStatus`, and `AzerothBenchObserverReportRefresh` button. It persists only bounded 16-entry load-result/lifecycle histories and counters for refresh availability and reason tokens. It does not persist copied samples.

Because this addon is loaded late, its first lifecycle fact is its actual `ADDON_LOADED` event. It records only events it really receives after registration (`ADDON_LOADED`, `PLAYER_LEAVING_WORLD`, and `PLAYER_LOGOUT`); it never pretends that `PLAYER_LOGIN` or `PLAYER_ENTERING_WORLD` was replayed. It has no score, epoch, reset, action, movement, combat, realm, server, hidden-unit, or privileged host authority.

Supported acceptance is bounded to ordinary dependency/demand-load discovery, synchronous `LoadAddOn`, actual `ADDON_LOADED`, copied observer consumption, independent FrameXML rendering/click dispatch, and per-character SavedVariables when the host supports that declared loader contract. This fixture does not qualify stock FrameXML, secure templates, broad addon compatibility, persistence on hosts lacking `SavedVariablesPerCharacter`, or any standard API beyond the already documented Wrath allowlist.
