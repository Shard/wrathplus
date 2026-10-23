//! Skill-line messages: the skills pane's `CMSG_UNLEARN_SKILL` (0x202). `HandleUnlearnSkillOpcode`
//! drops and anticheat-flags a request for a line without `SKILL_FLAG_UNLEARNABLE` (0x20) in its
//! `SkillRaceClassInfo.flags`; a removal comes back as a `PLAYER_SKILL_INFO` update, not an ack.
