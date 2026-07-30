use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};

pub const SHOW_WINDOW_ID: &str = "throne.show";
pub const TOGGLE_PROXY_ID: &str = "throne.toggle";
pub const QUIT_ID: &str = "throne.quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    ToggleProxy,
    Quit,
}

pub fn command_from_menu_id(id: &str) -> Option<TrayCommand> {
    match id {
        SHOW_WINDOW_ID => Some(TrayCommand::ShowWindow),
        TOGGLE_PROXY_ID => Some(TrayCommand::ToggleProxy),
        QUIT_ID => Some(TrayCommand::Quit),
        _ => None,
    }
}

pub fn install() -> Result<(), String> {
    let menu = Menu::new();
    let show_window = MenuItem::with_id(SHOW_WINDOW_ID, "Show Throne", true, None);
    let toggle_proxy = MenuItem::with_id(TOGGLE_PROXY_ID, "Start / Stop Proxy", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit Throne", true, None);
    let separator = PredefinedMenuItem::separator();
    menu.append_items(&[&show_window, &toggle_proxy, &separator, &quit])
        .map_err(|error| format!("build tray menu: {error}"))?;

    let icon = Icon::from_rgba(throne_icon_rgba(), 16, 16)
        .map_err(|error| format!("build tray icon: {error}"))?;
    let tray = TrayIconBuilder::new()
        .with_id("throne")
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_icon_as_template(true)
        .with_tooltip("Throne")
        .build()
        .map_err(|error| format!("install system tray: {error}"))?;
    Box::leak(Box::new(tray));
    Ok(())
}

pub fn next_command() -> Option<TrayCommand> {
    loop {
        let event = MenuEvent::receiver().try_recv().ok()?;
        if let Some(command) = command_from_menu_id(event.id.as_ref()) {
            return Some(command);
        }
    }
}

fn throne_icon_rgba() -> Vec<u8> {
    let mut rgba = vec![0; 16 * 16 * 4];
    for (y, ranges) in [
        (2, &[7..9][..]),
        (3, &[6..10][..]),
        (4, &[3..5, 6..10, 11..13][..]),
        (5, &[3..5, 5..11, 11..13][..]),
        (6, &[3..13][..]),
        (7, &[4..12][..]),
        (8, &[4..12][..]),
        (9, &[4..12][..]),
        (10, &[5..11][..]),
        (11, &[5..11][..]),
        (12, &[4..12][..]),
        (13, &[4..12][..]),
    ] {
        for range in ranges {
            for x in range.clone() {
                let offset = (y * 16 + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::throne_icon_rgba;

    #[test]
    fn tray_icon_uses_a_transparent_canvas_with_crown_peaks() {
        let rgba = throne_icon_rgba();
        let alpha = |x: usize, y: usize| rgba[(y * 16 + x) * 4 + 3];

        assert_eq!(rgba.len(), 16 * 16 * 4);
        assert_eq!(alpha(2, 2), 0);
        assert_eq!(alpha(3, 5), 255);
        assert_eq!(alpha(7, 2), 255);
        assert_eq!(alpha(12, 5), 255);
    }
}
