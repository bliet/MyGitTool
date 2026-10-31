//! The settings dialog. Every change applies and is saved right away.

use super::update_settings;
use crate::launch::Services;
use crate::state::AppState;
use crate::view::settings::{DATE_FORMATS, FILE_VIEWS, THEMES};
use application::Settings;
use dioxus::prelude::*;

/// Whether the settings dialog is open.
#[derive(Clone, Copy)]
pub struct SettingsOpen(pub Signal<bool>);

#[component]
pub fn SettingsDialog() -> Element {
    let SettingsOpen(mut open) = use_context::<SettingsOpen>();
    let services = use_context::<Services>();
    let settings = use_context::<Signal<Settings>>();
    let app = use_context::<Signal<AppState>>();
    let change = use_callback(move |apply: Box<dyn FnOnce(&mut Settings)>| {
        update_settings(&services, settings, app, apply);
    });
    if !open() {
        return rsx! {};
    }
    let current = *settings.read();

    rsx! {
        div {
            class: "dialog-backdrop",
            onclick: move |_| open.set(false),
            div {
                class: "dialog settings",
                role: "dialog",
                aria_modal: "true",
                aria_label: "Settings",
                onclick: move |event| event.stop_propagation(),
                h3 { "Settings" }
                div { class: "dialog-fields",
                    div { class: "field",
                        span { class: "field-label", "Theme" }
                        div { class: "toggle", role: "radiogroup", aria_label: "Theme",
                            for (theme, label) in THEMES {
                                button {
                                    key: "{label}",
                                    role: "radio",
                                    aria_checked: "{current.theme == theme}",
                                    class: if current.theme == theme { "active" } else { "" },
                                    onclick: move |_| change.call(Box::new(move |s| s.theme = theme)),
                                    "{label}"
                                }
                            }
                        }
                    }
                    label { "Date format"
                        select {
                            onchange: move |event| {
                                let chosen = DATE_FORMATS
                                    .iter()
                                    .position(|(_, label)| *label == event.value())
                                    .map(|i| DATE_FORMATS[i].0);
                                if let Some(format) = chosen {
                                    change.call(Box::new(move |s| s.date_format = format));
                                }
                            },
                            for (format, label) in DATE_FORMATS {
                                option {
                                    key: "{label}",
                                    value: "{label}",
                                    selected: current.date_format == format,
                                    "{label}"
                                }
                            }
                        }
                    }
                    div { class: "field",
                        span { class: "field-label", "Changed files" }
                        div { class: "toggle", role: "radiogroup", aria_label: "Changed files",
                            for (mode, label) in FILE_VIEWS {
                                button {
                                    key: "{label}",
                                    role: "radio",
                                    aria_checked: "{current.file_view == mode}",
                                    class: if current.file_view == mode { "active" } else { "" },
                                    onclick: move |_| change.call(Box::new(move |s| s.file_view = mode)),
                                    "{label}"
                                }
                            }
                        }
                    }
                    label { class: "checkbox",
                        input {
                            r#type: "checkbox",
                            checked: current.indent_guides,
                            onchange: move |event| {
                                let on = event.checked();
                                change.call(Box::new(move |s| s.indent_guides = on));
                            },
                        }
                        "Indent guides in trees"
                    }
                }
                div { class: "dialog-buttons",
                    button { class: "primary", onclick: move |_| open.set(false), "Done" }
                }
            }
        }
    }
}
