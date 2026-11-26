use super::*;
use crate::recovery::tests::{Directory, document};

#[test]
fn idle_app_has_no_recurring_repaint_deadline() {
    let app = App::default();
    let ctx = egui::Context::default();
    assert_eq!(
        app.repaint_delay_for_pending_work(&ctx, std::time::Instant::now()),
        None
    );
}

#[test]
fn preference_save_deadline_only_wakes_when_due() {
    let mut app = App::default();
    let ctx = egui::Context::default();
    let now = std::time::Instant::now();
    app.preferences_changed = Some(now - std::time::Duration::from_millis(500));
    assert_eq!(
        app.repaint_delay_for_pending_work(&ctx, now),
        Some(std::time::Duration::from_millis(250))
    );
}

#[test]
fn preference_deadline_frame_saves_without_new_input() {
    let directory = Directory::new();
    let path = directory.0.join("preferences.json");
    let mut app = App::default();
    app.preferences_path = Some(path.clone());
    app.ui.preferences.external_paint_app = "idle-save-test".into();
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    let first = ctx.run(egui::RawInput::default(), |ctx| {
        eframe::App::update(&mut app, ctx, &mut frame)
    });
    assert!(
        first
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .unwrap()
            .repaint_delay
            <= PREFERENCE_SAVE_DELAY,
        "the changed preference must schedule a wakeup"
    );
    assert!(app.preferences_changed.is_some());
    assert!(!path.exists(), "preference should still be debouncing");

    // The scheduled deadline fires with no further input; run the actual app update.
    app.preferences_changed = Some(std::time::Instant::now() - PREFERENCE_SAVE_DELAY);
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        eframe::App::update(&mut app, ctx, &mut frame)
    });
    assert_eq!(
        preferences::load(&path).unwrap().external_paint_app,
        "idle-save-test"
    );
    assert!(app.preferences_changed.is_none());
}

#[test]
fn mesh_worker_uses_bounded_polling_until_completion() {
    let mut app = App::default();
    let ctx = egui::Context::default();
    let now = std::time::Instant::now();
    let geometry = app.document.geometry_snapshot();
    let path = std::env::temp_dir()
        .join(format!(
            "sketchotter-scheduling-missing-{}",
            std::process::id()
        ))
        .to_string_lossy()
        .into_owned();
    app.mesh_job =
        Some(MeshJob::start(geometry, Operation::ImportObj(path)).expect("start test worker"));
    assert_eq!(
        app.repaint_delay_for_pending_work(&ctx, now),
        Some(BACKGROUND_POLL_INTERVAL)
    );
    app.mesh_job.as_ref().unwrap().cancel();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        if let Some(result) = app.mesh_job.as_mut().unwrap().poll() {
            break result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not finish"
        );
        std::thread::yield_now();
    };
    assert!(
        result.is_err(),
        "cancelled worker must not publish geometry"
    );
    app.mesh_job = None;
    assert_eq!(
        app.repaint_delay_for_pending_work(&ctx, now),
        None,
        "completed work must stop the polling timer"
    );
}

#[test]
fn dirty_idle_document_schedules_the_next_recovery_deadline() {
    let directory = Directory::new();
    let mut app = App::default();
    let ctx = egui::Context::default();
    app.document = document();
    app.initialize_recovery(directory.0.clone());
    let now = std::time::Instant::now();
    let expected = app
        .recovery
        .as_ref()
        .unwrap()
        .time_until_due(&app.ui.preferences.recovery, now);
    assert!(expected.is_some());
    assert_eq!(app.repaint_delay_for_pending_work(&ctx, now), expected);
}

#[test]
fn recovery_completion_is_polled_without_another_input_event() {
    let directory = Directory::new();
    let mut app = App::default();
    let ctx = egui::Context::default();
    app.document = document();
    app.initialize_recovery(directory.0.clone());
    app.save_recovery(&ctx);
    assert!(
        app.recovery.as_ref().unwrap().needs_poll(),
        "the in-flight write needs a repaint to poll its completion"
    );
    assert_eq!(
        app.repaint_delay_for_pending_work(&ctx, std::time::Instant::now()),
        Some(BACKGROUND_POLL_INTERVAL)
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.recovery.as_ref().unwrap().needs_poll() {
        app.tick_recovery(&ctx);
        assert!(
            std::time::Instant::now() < deadline,
            "recovery completion was not reported"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        app.repaint_delay_for_pending_work(&ctx, std::time::Instant::now())
            > Some(BACKGROUND_POLL_INTERVAL)
    );
}
