use tauri::{AppHandle, Manager};

use crate::db::DbState;
use crate::settings::{self, Settings};
use crate::timer::TimerController;

pub const WIDGET_LABEL: &str = "widget";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetVisibilityAction {
    Show,
    Hide,
}

pub fn widget_visibility_action(
    enabled: bool,
    main_visible: bool,
    main_minimized: bool,
) -> WidgetVisibilityAction {
    if enabled && (!main_visible || main_minimized) {
        WidgetVisibilityAction::Show
    } else {
        WidgetVisibilityAction::Hide
    }
}

pub fn effective_always_on_top(
    always_on_top: bool,
    break_always_on_top: bool,
    round_type: &str,
) -> bool {
    always_on_top && !(break_always_on_top && round_type != "work")
}

pub fn sync_for_main_window(app: &AppHandle) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let visible = main.is_visible().unwrap_or(false);
    let minimized = main.is_minimized().unwrap_or(false);
    let settings = load_settings(app);
    let round_type = current_round_type(app);
    sync_with_state(app, &settings, visible, minimized, &round_type);
}

pub fn show_if_enabled(app: &AppHandle) {
    let settings = load_settings(app);
    let round_type = current_round_type(app);
    sync_widget(
        app,
        widget_visibility_action(settings.floating_widget_enabled, false, false),
        effective_always_on_top(
            settings.always_on_top,
            settings.break_always_on_top,
            &round_type,
        ),
    );
}

pub fn hide(app: &AppHandle) {
    sync_widget(app, WidgetVisibilityAction::Hide, false);
}

pub fn sync_always_on_top(app: &AppHandle, settings: &Settings, round_type: &str) {
    if let Some(widget) = app.get_webview_window(WIDGET_LABEL) {
        let _ = widget.set_always_on_top(effective_always_on_top(
            settings.always_on_top,
            settings.break_always_on_top,
            round_type,
        ));
    }
}

fn sync_with_state(
    app: &AppHandle,
    settings: &Settings,
    main_visible: bool,
    main_minimized: bool,
    round_type: &str,
) {
    sync_widget(
        app,
        widget_visibility_action(
            settings.floating_widget_enabled,
            main_visible,
            main_minimized,
        ),
        effective_always_on_top(
            settings.always_on_top,
            settings.break_always_on_top,
            round_type,
        ),
    );
}

fn sync_widget(app: &AppHandle, action: WidgetVisibilityAction, always_on_top: bool) {
    let Some(widget) = app.get_webview_window(WIDGET_LABEL) else {
        return;
    };

    let _ = widget.set_always_on_top(always_on_top);
    match action {
        WidgetVisibilityAction::Show => {
            let _ = widget.show();
        }
        WidgetVisibilityAction::Hide => {
            let _ = widget.hide();
        }
    }
}

fn load_settings(app: &AppHandle) -> Settings {
    app.try_state::<DbState>()
        .and_then(|db| db.lock().ok().and_then(|conn| settings::load(&conn).ok()))
        .unwrap_or_default()
}

fn current_round_type(app: &AppHandle) -> String {
    app.try_state::<TimerController>()
        .map(|timer| timer.get_snapshot().round_type)
        .unwrap_or_else(|| "work".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_widget_is_hidden() {
        assert_eq!(
            widget_visibility_action(false, false, false),
            WidgetVisibilityAction::Hide
        );
        assert_eq!(
            widget_visibility_action(false, true, true),
            WidgetVisibilityAction::Hide
        );
    }

    #[test]
    fn enabled_widget_shows_when_main_is_hidden_or_minimized() {
        assert_eq!(
            widget_visibility_action(true, false, false),
            WidgetVisibilityAction::Show
        );
        assert_eq!(
            widget_visibility_action(true, true, true),
            WidgetVisibilityAction::Show
        );
    }

    #[test]
    fn enabled_widget_hides_when_main_is_visible_and_unminimized() {
        assert_eq!(
            widget_visibility_action(true, true, false),
            WidgetVisibilityAction::Hide
        );
    }

    #[test]
    fn widget_inherits_effective_always_on_top_policy() {
        assert!(effective_always_on_top(true, false, "work"));
        assert!(effective_always_on_top(true, false, "short-break"));
        assert!(effective_always_on_top(true, true, "work"));
        assert!(!effective_always_on_top(true, true, "short-break"));
        assert!(!effective_always_on_top(false, false, "work"));
    }

    #[test]
    fn effective_always_on_top_long_break_is_suppressed() {
        assert!(!effective_always_on_top(true, true, "long-break"));
    }

    #[test]
    fn always_on_top_false_overrides_break_flag() {
        assert!(!effective_always_on_top(false, true, "work"));
        assert!(!effective_always_on_top(false, true, "short-break"));
        assert!(!effective_always_on_top(false, true, "long-break"));
        assert!(!effective_always_on_top(false, false, "work"));
    }
}
