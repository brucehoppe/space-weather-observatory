//! Dumps the frozen demonstration dataset as the exact JSON the frontend
//! receives, for browser-preview inspection and for reproducibility evidence.
//!
//! Browser preview is a layout and interaction check only. It is not a test of
//! the desktop application and never of the Windows build.

use std::path::PathBuf;
use swo_app::{commands, demo, snapshot};

fn write(path: &PathBuf, value: &impl serde::Serialize) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    std::fs::write(path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write");
    println!("wrote {}", path.display());
}

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public/dev-fixtures");
    let payloads = demo::payloads();
    let now = demo::captured_at();

    let dashboard = snapshot::assemble(snapshot::Mode::Demo, &payloads, now, "demo".into());
    write(&out.join("dashboard.json"), &dashboard);

    let grid = snapshot::aurora_grid(&payloads).expect("aurora grid");
    write(&out.join("aurora.json"), &grid);

    write(&out.join("sources.json"), &commands::get_sources());
    write(&out.join("lessons.json"), &commands::get_lessons());
    write(&out.join("scenarios.json"), &demo::alert_scenarios());
    write(
        &out.join("settings.json"),
        &swo_app::settings::Settings::default(),
    );

    // Alert evaluation over the frozen (quiet) dataset, exactly as the app
    // would compute it.
    let speed = dashboard
        .series
        .get("noaa-swpc:rtsw_wind_1m:proton_speed")
        .expect("speed series");
    let settings = swo_core::alert::AlertSettings::default();
    let source = swo_core::alert::SourceIdentity {
        product: "rtsw_wind_1m".into(),
        spacecraft: dashboard.wind_spacecraft.clone(),
    };
    let memory = swo_core::alert::AlertMemory {
        settings_version: 1,
        ..Default::default()
    };
    let last = speed.samples.last().map(|o| o.time).unwrap_or(now);
    let evaluation = swo_core::alert::evaluate(&speed.samples, &settings, &memory, &source, last);
    write(&out.join("alert.json"), &evaluation);
}
