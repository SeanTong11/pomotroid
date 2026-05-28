use tauri::{AppHandle, Manager};

use crate::db::DbState;
use crate::settings::{self, Settings};
use crate::timer::TimerController;
use crate::window as app_window;

pub const WIDGET_LABEL: &str = "widget";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetVisibilityAction {
    Show,
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetDisplayTrigger {
    Minimize,
    CloseToTray,
}

pub fn widget_visibility_action(
    can_show: bool,
    main_visible: bool,
    main_minimized: bool,
) -> WidgetVisibilityAction {
    if can_show && (!main_visible || main_minimized) {
        WidgetVisibilityAction::Show
    } else {
        WidgetVisibilityAction::Hide
    }
}

pub fn widget_trigger_enabled(settings: &Settings, trigger: WidgetDisplayTrigger) -> bool {
    settings.floating_widget_enabled
        && match trigger {
            WidgetDisplayTrigger::Minimize => settings.floating_widget_on_minimize,
            WidgetDisplayTrigger::CloseToTray => {
                settings.min_to_tray_on_close && settings.floating_widget_on_close
            }
        }
}

pub fn has_active_widget_trigger(settings: &Settings) -> bool {
    !settings.floating_widget_enabled
        || settings.floating_widget_on_minimize
        || (settings.min_to_tray_on_close && settings.floating_widget_on_close)
}

pub fn widget_trigger_repair_setting(settings: &Settings) -> Option<(&'static str, &'static str)> {
    if has_active_widget_trigger(settings) {
        None
    } else {
        Some(("floating_widget_on_minimize", "true"))
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
    sync_with_state(
        app,
        &settings,
        WidgetDisplayTrigger::Minimize,
        visible,
        minimized,
        &round_type,
    );
}

pub fn sync_for_main_window_minimize_event(app: &AppHandle) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let minimized = main.is_minimized().unwrap_or(false);
    if !should_sync_for_focus_loss(minimized) {
        return;
    }

    let visible = main.is_visible().unwrap_or(false);
    let settings = load_settings(app);
    let round_type = current_round_type(app);
    sync_with_state(
        app,
        &settings,
        WidgetDisplayTrigger::Minimize,
        visible,
        minimized,
        &round_type,
    );
}

pub fn should_sync_for_focus_loss(main_minimized: bool) -> bool {
    main_minimized
}

pub fn show_for_minimize(app: &AppHandle) {
    show_for_trigger(app, WidgetDisplayTrigger::Minimize);
}

pub fn show_for_close_to_tray(app: &AppHandle) {
    show_for_trigger(app, WidgetDisplayTrigger::CloseToTray);
}

fn show_for_trigger(app: &AppHandle, trigger: WidgetDisplayTrigger) {
    let settings = load_settings(app);
    let round_type = current_round_type(app);
    sync_widget(
        app,
        widget_visibility_action(widget_trigger_enabled(&settings, trigger), false, false),
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
        app_window::set_always_on_top(
            &widget,
            effective_always_on_top(
                settings.always_on_top,
                settings.break_always_on_top,
                round_type,
            ),
            "widget",
        );
    }
}

fn sync_with_state(
    app: &AppHandle,
    settings: &Settings,
    trigger: WidgetDisplayTrigger,
    main_visible: bool,
    main_minimized: bool,
    round_type: &str,
) {
    sync_widget(
        app,
        widget_visibility_action(
            widget_trigger_enabled(settings, trigger),
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

    match action {
        WidgetVisibilityAction::Show => {
            let _ = widget.show();
            app_window::set_always_on_top(&widget, always_on_top, "widget");
        }
        WidgetVisibilityAction::Hide => {
            app_window::set_always_on_top(&widget, false, "widget");
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
    fn minimize_trigger_is_enabled_by_default_when_widget_is_enabled() {
        let settings = Settings {
            floating_widget_enabled: true,
            ..Settings::default()
        };

        assert!(widget_trigger_enabled(&settings, WidgetDisplayTrigger::Minimize));
        assert!(has_active_widget_trigger(&settings));
        assert_eq!(widget_trigger_repair_setting(&settings), None);
    }

    #[test]
    fn minimize_trigger_can_be_disabled_when_close_trigger_is_active() {
        let settings = Settings {
            floating_widget_enabled: true,
            floating_widget_on_minimize: false,
            floating_widget_on_close: true,
            min_to_tray_on_close: true,
            ..Settings::default()
        };

        assert!(!widget_trigger_enabled(&settings, WidgetDisplayTrigger::Minimize));
        assert!(widget_trigger_enabled(&settings, WidgetDisplayTrigger::CloseToTray));
        assert!(has_active_widget_trigger(&settings));
        assert_eq!(widget_trigger_repair_setting(&settings), None);
    }

    #[test]
    fn close_trigger_requires_close_to_tray() {
        let settings = Settings {
            floating_widget_enabled: true,
            floating_widget_on_minimize: false,
            floating_widget_on_close: true,
            min_to_tray_on_close: false,
            ..Settings::default()
        };

        assert!(!widget_trigger_enabled(&settings, WidgetDisplayTrigger::CloseToTray));
        assert!(!has_active_widget_trigger(&settings));
        assert_eq!(
            widget_trigger_repair_setting(&settings),
            Some(("floating_widget_on_minimize", "true"))
        );
    }

    #[test]
    fn widget_requires_at_least_one_active_trigger_when_enabled() {
        let settings = Settings {
            floating_widget_enabled: true,
            floating_widget_on_minimize: false,
            floating_widget_on_close: false,
            ..Settings::default()
        };

        assert!(!has_active_widget_trigger(&settings));
        assert_eq!(
            widget_trigger_repair_setting(&settings),
            Some(("floating_widget_on_minimize", "true"))
        );
    }

    #[test]
    fn focus_loss_sync_only_handles_actual_minimize() {
        assert!(should_sync_for_focus_loss(true));
        assert!(!should_sync_for_focus_loss(false));
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
