//! The "buy me a coffee" note: shown once, after the first app installs, and
//! reachable any time from the sidebar.
//!
//! Lodestar makes no network request for this. Choosing the link hands one fixed
//! URL to the browser; the site behind it lists the ways to chip in.

use eframe::egui;

use crate::engine::Job;

/// One stable address, so payment options can change without an app release.
/// No query string: the site learns nothing about who clicked or from where.
pub const URL: &str = "https://asroya.github.io/lodestar/#support";

/// Whether the one-time note should open now: it has never been shown, no
/// two-factor or PIN dialog is up, and the current job has just succeeded.
pub fn prompt_due(already_shown: bool, prompt_open: bool, job: Option<&Job>) -> bool {
    !already_shown && !prompt_open && job.is_some_and(|j| matches!(j.result, Some(Ok(_))))
}

/// Opens the support page in the default browser.
pub fn open(ctx: &egui::Context) {
    ctx.open_url(egui::OpenUrl::new_tab(URL));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Stage;

    fn job(result: Option<Result<String, String>>) -> Job {
        Job {
            id: 1,
            title: "Installing Stremio".into(),
            stage: Stage::Done,
            fraction: 1.0,
            detail: String::new(),
            result,
        }
    }

    #[test]
    fn the_note_waits_for_a_finished_successful_job() {
        let ok = job(Some(Ok("Stremio is installed on Living Room".into())));
        assert!(prompt_due(false, false, Some(&ok)));
        assert!(!prompt_due(false, false, None), "no job yet");
        assert!(!prompt_due(false, false, Some(&job(None))), "still running");
        assert!(!prompt_due(false, false, Some(&job(Some(Err("failed".into()))))), "failed install");
    }

    #[test]
    fn the_note_appears_only_once_and_never_over_another_dialog() {
        let ok = job(Some(Ok("done".into())));
        assert!(!prompt_due(true, false, Some(&ok)), "already shown");
        assert!(!prompt_due(false, true, Some(&ok)), "a code prompt is open");
    }

    #[test]
    fn opening_sends_exactly_the_support_url_and_nothing_else() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| open(ui.ctx()));
        // egui panics if a frame's texture uploads are dropped unhandled.
        out.textures_delta.clear();
        let urls: Vec<_> = out
            .platform_output
            .commands
            .iter()
            .filter_map(|c| match c {
                egui::OutputCommand::OpenUrl(u) => Some((u.url.clone(), u.new_tab)),
                _ => None,
            })
            .collect();
        assert_eq!(urls, vec![(URL.to_string(), true)]);
        assert!(URL.starts_with("https://") && !URL.contains('?'), "no tracking parameters");
    }
}
