//! Lodestar — a free, native-Rust sideloader for iPhone, iPad and Apple TV.

mod app;
mod boot;
mod device;
mod engine;
mod ipa;
mod motion;
mod pipeline;
mod store;
mod theme;

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use eframe::egui;
use tracing_subscriber::{EnvFilter, fmt::MakeWriter, prelude::*};

use crate::engine::LogLine;

/// Captures formatted tracing output for the Activity page.
#[derive(Clone)]
struct UiLog(Arc<Mutex<Vec<LogLine>>>);

struct UiLogWriter {
    sink: Arc<Mutex<Vec<LogLine>>>,
    level: tracing::Level,
    buf: Vec<u8>,
}

impl Write for UiLogWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for UiLogWriter {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.buf).trim_end().to_string();
        if !text.is_empty() {
            self.sink.lock().unwrap().push(LogLine { level: self.level, text });
        }
    }
}

impl<'a> MakeWriter<'a> for UiLog {
    type Writer = UiLogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        UiLogWriter { sink: self.0.clone(), level: tracing::Level::INFO, buf: vec![] }
    }
    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        UiLogWriter { sink: self.0.clone(), level: *meta.level(), buf: vec![] }
    }
}

fn main() -> eframe::Result {
    // reqwest is built with `rustls-no-provider`; without a process-wide
    // provider the first HTTPS client (e.g. silent sign-in at launch) panics.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = isideload::init();

    let lines = Arc::new(Mutex::new(Vec::new()));
    let filter = || {
        EnvFilter::try_from_env("LODESTAR_LOG")
            .unwrap_or_else(|_| EnvFilter::new("info,idevice=warn,jktcp=warn,mdns_sd=error,reqwest=warn,hyper=warn"))
    };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .without_time()
                .with_level(false)
                .with_writer(UiLog(lines.clone()))
                .with_filter(filter()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_filter(filter()))
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lodestar")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([900.0, 620.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };

    eframe::run_native(
        "Lodestar",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let engine = engine::Engine::start(move || ctx.request_repaint(), lines);
            Ok(Box::new(app::LodestarApp::new(cc, engine)))
        }),
    )
}
