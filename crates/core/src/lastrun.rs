//! Read the component log a game's last run left beside its exe and keep the
//! handful of lines that say whether the DLSS 5 pass ran, so the Game page
//! can show them. Pure text filtering; nothing is interpreted beyond "did an
//! error line appear".

use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::api::LastRun;

/// Logs in the order we prefer when several exist.
const LOGS: &[&str] = &["OptiScaler.log", "ReShade.log", "dlss5-feed.log"];
const MAX_LINES: usize = 14;
const MAX_LINE_CHARS: usize = 220;

/// Substrings that make a line worth showing.
const KEEP: &[&str] = &[
    "[E]",
    "ERROR",
    "DLSS-NR",
    "DlssNr_Dx12",
    "_CreateFeature result",
    "_EvaluateFeature result",
    "Feature evaluation failed",
    "Device removed",
    "DLSS5 Generic",
    "Registered add-on",
    "NGX hooks installed",
    "feature 18",
    "feature create intercepted",
    "runtime",
];
/// Noise that matches `KEEP` but says nothing on its own.
const DROP: &[&str] = &[
    "near-miss",
    "ExposureScan",
    "NgxDiagnostics",
    "vtable::Hook",
    "search path",
];

pub fn read(exe_dir: &Path) -> Option<LastRun> {
    let (path, modified) = LOGS
        .iter()
        .map(|n| exe_dir.join(n))
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, m))
        })
        .max_by_key(|(_, m)| *m)?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some(summarise(
        &path.display().to_string(),
        modified
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        &text,
    ))
}

pub fn summarise(log: &str, modified: u64, text: &str) -> LastRun {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| KEEP.iter().any(|k| l.contains(k)) && !DROP.iter().any(|d| l.contains(d)))
        .map(|l| {
            let l = l.trim();
            if l.chars().count() > MAX_LINE_CHARS {
                let cut: String = l.chars().take(MAX_LINE_CHARS - 1).collect();
                format!("{cut}…")
            } else {
                l.to_string()
            }
        })
        .collect();
    if lines.len() > MAX_LINES {
        lines.drain(..lines.len() - MAX_LINES);
    }
    let failed = lines
        .iter()
        .any(|l| l.contains("[E]") || l.contains("ERROR") || l.contains("failed"));
    LastRun {
        log: log.to_string(),
        modified,
        lines,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_decisive_lines_and_flags_errors() {
        let text = "\
[1] [I] OptiScaler v10 loaded
[2] [I] DlssNr::ExposureScan::NoteResource DLSS-NR scan near-miss #1
[3] [I] DLSSFeatureDx12::InitDLSS _CreateFeature result: NVSDK_NGX_Result_Success
[4] [I] DlssNr_Dx12::State::Run DLSS-NR guides: depth inverted
[5] [E] DLSSFeatureDx12::EvaluateInternal _EvaluateFeature result: BAD00002
";
        let r = summarise("x.log", 7, text);
        assert_eq!(r.lines.len(), 3);
        assert!(r.lines[0].contains("_CreateFeature"));
        assert!(r.failed);
        assert_eq!(r.modified, 7);

        let ok = summarise(
            "x.log",
            1,
            "[I] [DLSS 5 Neural Rendering] DLSS5 Generic: NGX hooks installed\n",
        );
        assert_eq!(ok.lines.len(), 1);
        assert!(!ok.failed);
    }

    #[test]
    fn reads_the_newest_log_in_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path()).is_none());
        std::fs::write(
            tmp.path().join("ReShade.log"),
            "[I] Registered add-on \"DLSS 5 Neural Rendering\"\n",
        )
        .unwrap();
        let r = read(tmp.path()).unwrap();
        assert!(r.log.ends_with("ReShade.log"));
        assert_eq!(r.lines.len(), 1);
    }
}
