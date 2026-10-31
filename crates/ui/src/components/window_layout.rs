//! Keeps track of the main window's layout and saves it, when a column
//! resize ends and when the window closes.

use super::blocking;
use crate::launch::Services;
use crate::state::AppState;
use crate::view::columns::ColumnLayout;
use application::WindowLayout;
use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::{use_wry_event_handler, window};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct LayoutKeeper {
    services: Signal<Services>,
    app: Signal<AppState>,
    pub columns: Signal<ColumnLayout>,
    /// The current inner width of the window. The columns span all of it,
    /// so their room is known without measuring them, which would feed
    /// their own widths back into the measurement.
    pub window_width: Signal<f64>,
    /// The last size of the window while it was neither maximized nor
    /// minimized, so a maximized window comes back with its normal size.
    normal_size: Signal<(f64, f64)>,
}

impl LayoutKeeper {
    fn layout(&self) -> WindowLayout {
        let (width, height) = *self.normal_size.peek();
        let columns = *self.columns.peek();
        WindowLayout {
            width,
            height,
            maximized: window().is_maximized(),
            left_column: columns.left,
            right_column: columns.right,
        }
    }

    /// Saves the layout in the background.
    pub fn save(mut self) {
        let layout = self.layout();
        let remember = self.services.peek().remember_layout.clone();
        spawn(async move {
            match blocking(move || remember.save(&layout)).await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => self.app.write().error = Some(format!("Window layout: {err}")),
                Err(message) => self.app.write().error = Some(message),
            }
        });
    }

    /// Saves the layout right away, for when the window is about to close
    /// and a background task would not finish. A failure has no one left to
    /// tell, so it is ignored.
    fn save_now(&self) {
        let _ = self.services.peek().remember_layout.save(&self.layout());
    }
}

/// Provides the [`LayoutKeeper`] and follows the main window's size.
pub fn use_layout_keeper(services: &Services, initial: WindowLayout) -> LayoutKeeper {
    let app = use_context::<Signal<AppState>>();
    let services = services.clone();
    let keeper = use_context_provider(move || {
        let desktop = window();
        let width = desktop
            .inner_size()
            .to_logical::<f64>(desktop.scale_factor())
            .width;
        LayoutKeeper {
            services: Signal::new(services),
            app,
            columns: Signal::new(ColumnLayout::restored(
                initial.left_column,
                initial.right_column,
            )),
            window_width: Signal::new(width),
            normal_size: Signal::new((initial.width, initial.height)),
        }
    });
    let main = use_hook(|| window().window.id());
    use_wry_event_handler(move |event, _| {
        let Event::WindowEvent {
            window_id, event, ..
        } = event
        else {
            return;
        };
        if *window_id != main {
            return;
        }
        match event {
            WindowEvent::Resized(size) => {
                let desktop = window();
                let size = size.to_logical::<f64>(desktop.scale_factor());
                if desktop.is_minimized() {
                    return;
                }
                let mut window_width = keeper.window_width;
                if *window_width.peek() != size.width {
                    window_width.set(size.width);
                }
                if desktop.is_maximized() {
                    return;
                }
                let mut normal_size = keeper.normal_size;
                normal_size.set((size.width, size.height));
            }
            WindowEvent::CloseRequested => keeper.save_now(),
            _ => {}
        }
    });
    keeper
}
