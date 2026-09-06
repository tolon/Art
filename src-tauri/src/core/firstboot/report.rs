//! Reading `S:FirstBoot.log` — what the Amiga said happened.
//!
//! One line per event, appended by the dispatcher and the step wrapper
//! (`scripts/`). The parser is lenient on purpose: a line it does not know is
//! **carried** in `unknown`, never dropped, because a card written by a newer
//! ART must still be readable by this one. Endings stay distinct (spec §4.3):
//! no text at all is "not booted", text without `done` is "unfinished", and
//! `done all` / `done partial` are what the directory said.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstBootReport {
    pub version: Option<u32>,
    pub system: Option<Detected>,
    pub steps: Vec<StepReport>,
    pub ending: Ending,
    pub fat_copy_failed: bool,
    pub reboot_requested_by: Option<String>,
    pub unknown: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub system: String,
    pub rpi: String,
    pub kick: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepReport {
    pub name: String,
    pub outcome: StepOutcome,
    pub details: Vec<String>,
}

/// `rename_all` on the enum governs variant *names* only — it does not cascade
/// into a struct variant's own fields (the `PackagePanel` trap). `reason` and
/// `rc` are already single words with nothing to rename, but a future
/// two-word field must carry its own `#[serde(rename = "...")]` explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StepOutcome {
    Ok,
    Skipped {
        #[serde(rename = "reason")]
        reason: String,
    },
    Refused {
        #[serde(rename = "rc")]
        rc: i64,
    },
    /// `started` with nothing after it: the machine stopped or hung mid-step.
    Unfinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ending {
    /// No text at all — the card has not run the block yet.
    NotBooted,
    /// Text, but no `done` line.
    Unfinished,
    DoneAll,
    DonePartial,
}

/// The Amiga writes Latin-1. Decoding as UTF-8 would turn `türkçe` into
/// replacement characters — ART-168's class, on the way back in.
pub fn parse_bytes(bytes: &[u8]) -> FirstBootReport {
    let text: String = bytes.iter().map(|&b| b as char).collect();
    parse(&text)
}

pub fn parse(text: &str) -> FirstBootReport {
    let mut report = FirstBootReport {
        version: None,
        system: None,
        steps: Vec::new(),
        ending: Ending::NotBooted,
        fat_copy_failed: false,
        reboot_requested_by: None,
        unknown: Vec::new(),
    };
    let mut saw_text = false;
    let mut done: Option<Ending> = None;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        saw_text = true;
        let words: Vec<&str> = line.splitn(4, ' ').collect();
        match words.as_slice() {
            ["art-firstboot", v] => report.version = v.parse().ok(),
            ["system", system, rpi, rest] => {
                let kick = rest.strip_prefix("kick ").unwrap_or(rest).to_string();
                report.system = Some(Detected {
                    system: system.to_string(),
                    rpi: rpi.to_string(),
                    kick,
                });
            }
            ["step", name, "started"] => {
                report.steps.push(StepReport {
                    name: name.to_string(),
                    outcome: StepOutcome::Unfinished,
                    details: Vec::new(),
                });
            }
            ["step", name, "ok"] => {
                if let Some(step) = step_named(&mut report, name) {
                    if step.outcome == StepOutcome::Unfinished {
                        step.outcome = StepOutcome::Ok;
                    }
                } else {
                    report.unknown.push(line.to_string());
                }
            }
            ["step", name, "skipped", reason] => {
                if let Some(step) = step_named(&mut report, name) {
                    step.outcome = StepOutcome::Skipped {
                        reason: reason.to_string(),
                    };
                } else {
                    report.unknown.push(line.to_string());
                }
            }
            ["step", name, "refused", rc] => {
                let rc = rc
                    .strip_prefix("rc=")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(-1);
                if let Some(step) = step_named(&mut report, name) {
                    step.outcome = StepOutcome::Refused { rc };
                } else {
                    report.unknown.push(line.to_string());
                }
            }
            ["step", name, "detail", detail] => {
                if let Some(step) = step_named(&mut report, name) {
                    step.details.push(detail.to_string());
                } else {
                    report.unknown.push(line.to_string());
                }
            }
            ["reboot", "requested", "by", name] => {
                report.reboot_requested_by = Some(name.to_string());
            }
            ["done", "all"] => done = Some(Ending::DoneAll),
            ["done", "partial"] => done = Some(Ending::DonePartial),
            ["copy-to-fat", "failed"] => report.fat_copy_failed = true,
            _ => report.unknown.push(line.to_string()),
        }
    }

    report.ending = match (saw_text, done) {
        (false, _) => Ending::NotBooted,
        (true, None) => Ending::Unfinished,
        (true, Some(ending)) => ending,
    };
    report
}

/// The most recent step of that name — a step retried on a later boot
/// appears twice, and the later lines belong to the later attempt.
fn step_named<'a>(report: &'a mut FirstBootReport, name: &str) -> Option<&'a mut StepReport> {
    report.steps.iter_mut().rev().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "art-firstboot 1\nsystem PiStorm RPi4 kick 3.2\nstep 10-hardware started\nstep 10-hardware detail sd0-mounted RPi4\nstep 10-hardware ok\nstep 20-aux started\nstep 20-aux skipped not-3.9\nstep 20-aux ok\nstep 50-pkg-boingbag-39-1 started\nstep 50-pkg-boingbag-39-1 refused rc=20\nstep 90-prefs started\nstep 90-prefs ok\nreboot requested by 90-prefs\ndone partial\n";

    #[test]
    fn a_full_report_keeps_every_ending_distinct() {
        let r = parse(FULL);
        assert_eq!(r.version, Some(1));
        let d = r.system.as_ref().unwrap();
        assert_eq!(
            (d.system.as_str(), d.rpi.as_str(), d.kick.as_str()),
            ("PiStorm", "RPi4", "3.2")
        );
        assert_eq!(r.steps.len(), 4);
        assert_eq!(r.steps[0].outcome, StepOutcome::Ok);
        assert_eq!(r.steps[0].details, vec!["sd0-mounted RPi4".to_string()]);
        assert_eq!(
            r.steps[1].outcome,
            StepOutcome::Skipped {
                reason: "not-3.9".into()
            }
        );
        assert_eq!(r.steps[2].outcome, StepOutcome::Refused { rc: 20 });
        assert_eq!(r.steps[3].outcome, StepOutcome::Ok);
        assert_eq!(r.reboot_requested_by.as_deref(), Some("90-prefs"));
        assert_eq!(r.ending, Ending::DonePartial);
        assert!(r.unknown.is_empty());
    }

    #[test]
    fn a_step_that_started_and_never_finished_is_unfinished_and_so_is_the_report() {
        let r = parse("art-firstboot 1\nsystem UAE none kick 3.2\nstep 10-hardware started\n");
        assert_eq!(r.steps[0].outcome, StepOutcome::Unfinished);
        assert_eq!(r.ending, Ending::Unfinished);
    }

    #[test]
    fn an_empty_file_is_not_booted_and_a_missing_done_line_is_unfinished() {
        assert_eq!(parse("").ending, Ending::NotBooted);
        assert_eq!(parse("art-firstboot 1\n").ending, Ending::Unfinished);
        assert_eq!(parse("art-firstboot 1\ndone all\n").ending, Ending::DoneAll);
    }

    #[test]
    fn a_skipped_line_wins_over_the_ok_that_follows_it() {
        let r = parse("art-firstboot 1\nstep 20-aux started\nstep 20-aux skipped uae\nstep 20-aux ok\ndone all\n");
        assert_eq!(
            r.steps[0].outcome,
            StepOutcome::Skipped {
                reason: "uae".into()
            }
        );
    }

    #[test]
    fn a_line_from_a_newer_art_is_carried_not_dropped() {
        let r = parse("art-firstboot 2\nfrobnicate 3\nstep 10-hardware started\nstep 10-hardware ok\ndone all\n");
        assert_eq!(r.version, Some(2));
        assert_eq!(r.unknown, vec!["frobnicate 3".to_string()]);
        assert_eq!(r.steps[0].outcome, StepOutcome::Ok);
    }

    #[test]
    fn the_fat_copy_failure_is_a_flag_not_a_step() {
        let r = parse("art-firstboot 1\ndone all\ncopy-to-fat failed\n");
        assert!(r.fat_copy_failed);
        assert!(r.steps.is_empty());
        assert_eq!(r.ending, Ending::DoneAll);
    }

    #[test]
    fn crlf_and_latin1_are_read_as_the_amiga_wrote_them() {
        let bytes = b"art-firstboot 1\r\nstep 50-pkg-t\xFCrk started\r\nstep 50-pkg-t\xFCrk ok\r\ndone all\r\n";
        let r = parse_bytes(bytes);
        assert_eq!(r.steps[0].name, "50-pkg-t\u{fc}rk");
        assert_eq!(r.ending, Ending::DoneAll);
    }

    #[test]
    fn a_refused_step_with_a_bad_rc_is_still_refused() {
        let r = parse("art-firstboot 1\nstep x started\nstep x refused rc=banana\ndone partial\n");
        assert_eq!(r.steps[0].outcome, StepOutcome::Refused { rc: -1 });
    }

    #[test]
    fn the_wire_shape_is_what_the_frontend_reads() {
        let r = parse(FULL);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["ending"], "done-partial");
        assert_eq!(json["steps"][1]["outcome"]["kind"], "skipped");
        assert_eq!(json["steps"][1]["outcome"]["reason"], "not-3.9");
        assert_eq!(json["steps"][2]["outcome"]["rc"], 20);
        assert_eq!(json["rebootRequestedBy"], "90-prefs");
        assert_eq!(json["fatCopyFailed"], false);
    }

    #[test]
    fn an_outcome_without_a_start_is_unknown() {
        let r = parse("art-firstboot 1\nstep x ok\ndone all\n");
        assert_eq!(r.unknown, vec!["step x ok".to_string()]);
    }
}
