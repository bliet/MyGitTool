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

/// Reports the size of the page, the window's content. The native window
/// size is no use here: GTK on Wayland includes client-side decorations
/// whose shadow changes with the focus.
const SIZE_LISTENER: &str = r#"
    const send = () => dioxus.send([window.innerWidth, window.innerHeight]);
    window.addEventListener("resize", send);
    send();
    await new Promise(() => {});
"#;

#[derive(Clone, Copy)]
pub struct LayoutKeeper {
    services: Signal<Services>,
    app: Signal<AppState>,
    pub columns: Signal<ColumnLayout>,
    /// The current width of the window's content. The columns span all of
    /// it, so their room is known without measuring them, which would feed
    /// their own widths back into the measurement.
    pub window_width: Signal<f64>,
    /// The last size of the window while it was neither maximized nor
    /// minimized, so a maximized window comes back with its normal size.
    normal_size: Signal<(f64, f64)>,
    /// The window size minus the content size. The window is created with
    /// the stored size, but on some platforms (e.g. GTK on Wayland) the
    /// title bar takes part of it, so the content comes out smaller. The
    /// first report of a normal window measures this; without it the
    /// window would shrink with every start.
    chrome: Signal<Option<(f64, f64)>>,
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
    let keeper = use_context_provider(move || LayoutKeeper {
        services: Signal::new(services),
        app,
        columns: Signal::new(ColumnLayout::restored(
            initial.left_column,
            initial.right_column,
        )),
        window_width: Signal::new(initial.width),
        normal_size: Signal::new((initial.width, initial.height)),
        chrome: Signal::new(None),
    });
    use_hook(move || {
        spawn(async move {
            let mut sizes = document::eval(SIZE_LISTENER);
            while let Ok([width, height]) = sizes.recv::<[f64; 2]>().await {
                let desktop = window();
                if desktop.is_minimized() || width <= 0.0 {
                    continue;
                }
                let mut window_width = keeper.window_width;
                if *window_width.peek() != width {
                    window_width.set(width);
                }
                if desktop.is_maximized() {
                    continue;
                }
                // A window leaving maximized state also comes back with the
                // stored size, so it can be measured then.
                let mut chrome = keeper.chrome;
                let (extra_width, extra_height) = *chrome
                    .write()
                    .get_or_insert((initial.width - width, initial.height - height));
                let mut normal_size = keeper.normal_size;
                normal_size.set((width + extra_width, height + extra_height));
            }
        });
    });
    let main = use_hook(|| window().window.id());
    use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent {
            window_id,
            event: WindowEvent::CloseRequested,
            ..
        } = event
            && *window_id == main
        {
            keeper.save_now();
        }
    });
    keeper
}
