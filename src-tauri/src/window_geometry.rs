// Windows-only window geometry persistence. This deliberately uses OneKeePass's
// existing preference.toml rather than the Tauri window-state plugin's
// application-data directory (which is unsuitable for portable builds).
use std::sync::{Mutex, OnceLock};

use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

use crate::app_preference::WindowGeometry;
use crate::app_state::AppState;

static LAST_NORMAL_BOUNDS: OnceLock<Mutex<Option<WindowGeometry>>> = OnceLock::new();

fn normal_bounds() -> &'static Mutex<Option<WindowGeometry>> {
    LAST_NORMAL_BOUNDS.get_or_init(|| Mutex::new(None))
}

// Preferences store the OUTER window dimensions, including title bar and borders.
// Tauri's set_size instead expects INNER (client-area) dimensions. Convert using
// the current window decoration thickness so restoring cannot enlarge the
// window on each launch. Existing preference.toml values remain compatible.
fn inner_size_from_outer(
    saved_outer: PhysicalSize<u32>,
    current_outer: PhysicalSize<u32>,
    current_inner: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    let frame_width = current_outer.width.saturating_sub(current_inner.width);
    let frame_height = current_outer.height.saturating_sub(current_inner.height);

    PhysicalSize::new(
        saved_outer.width.saturating_sub(frame_width).max(1),
        saved_outer.height.saturating_sub(frame_height).max(1),
    )
}

pub(crate) fn capture<R: Runtime>(window: &WebviewWindow<R>) {
    if window.is_maximized().unwrap_or(false) || window.is_minimized().unwrap_or(false) {
        return;
    }
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    if size.width < 400 || size.height < 300 {
        return;
    }
    if let Ok(mut slot) = normal_bounds().lock() {
        *slot = Some(WindowGeometry {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
}

pub(crate) fn restore<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let saved = app
        .state::<AppState>()
        .preference
        .lock()
        .ok()
        .and_then(|preference| preference.window_geometry());

    if let Some(bounds) = saved {
        if let Ok(mut slot) = normal_bounds().lock() {
            *slot = Some(bounds);
        }
        if let Ok(monitors) = window.available_monitors() {
            // Keep at least part of the title bar and window visible. If the
            // stored monitor has gone away, use Tauri's default centered position.
            let monitor = monitors.iter().find(|monitor| {
                let mp = monitor.position();
                let ms = monitor.size();
                let left = (bounds.x as i64).max(mp.x as i64);
                let top = (bounds.y as i64).max(mp.y as i64);
                let right = (bounds.x as i64 + bounds.width as i64)
                    .min(mp.x as i64 + ms.width as i64);
                let bottom = (bounds.y as i64 + bounds.height as i64)
                    .min(mp.y as i64 + ms.height as i64);
                right - left >= 120 && bottom - top >= 80
            });
            if let Some(monitor) = monitor {
                let width = bounds.width.max(1024).min(monitor.size().width);
                let height = bounds.height.max(600).min(monitor.size().height);
                if let (Ok(current_outer), Ok(current_inner)) =
                    (window.outer_size(), window.inner_size())
                {
                    let restored_inner = inner_size_from_outer(
                        PhysicalSize::new(width, height),
                        current_outer,
                        current_inner,
                    );
                    let _ = window.set_size(restored_inner);
                } else {
                    log::warn!("Could not measure window borders; retaining current window size");
                }
                let _ = window.set_position(PhysicalPosition::new(bounds.x, bounds.y));
            }
        }
        if bounds.maximized {
            let _ = window.maximize();
        }
    }
    capture(&window);
}

pub(crate) fn save<R: Runtime>(app: &tauri::AppHandle<R>) {
    let window = app.get_webview_window("main");
    if let Some(ref window) = window {
        capture(window);
    }
    let cached = normal_bounds().lock().ok().and_then(|slot| *slot);
    let Some(mut bounds) = cached else {
        return;
    };
    if let Some(window) = window {
        // Preserve the last normal bounds when maximized, rather than storing
        // maximized screen-covering dimensions as the default window size.
        bounds.maximized = window.is_maximized().unwrap_or(bounds.maximized);
    }
    if let Ok(mut preference) = app.state::<AppState>().preference.lock() {
        preference.save_window_geometry(bounds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_outer_dimensions_do_not_grow_between_restarts() {
        let initial_outer = PhysicalSize::new(1116, 779);
        let initial_inner = PhysicalSize::new(1100, 740);
        let stored_outer = PhysicalSize::new(1300, 850);

        let restored = inner_size_from_outer(stored_outer, initial_outer, initial_inner);
        assert_eq!(restored, PhysicalSize::new(1284, 811));

        // The next shutdown saves outer_size(), restoring to the same client size.
        let next_outer = PhysicalSize::new(restored.width + 16, restored.height + 39);
        assert_eq!(
            inner_size_from_outer(next_outer, initial_outer, initial_inner),
            restored
        );
    }

    #[test]
    fn restored_size_respects_changed_window_frame_thickness() {
        let stored_outer = PhysicalSize::new(1300, 850);
        let current_outer = PhysicalSize::new(1120, 785);
        let current_inner = PhysicalSize::new(1100, 740);
        assert_eq!(
            inner_size_from_outer(stored_outer, current_outer, current_inner),
            PhysicalSize::new(1280, 805)
        );
    }
}
