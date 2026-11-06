//! macOS/Windows application menu bar aligned with upstream Throne 1.4.0.
//!
//! Upstream hides `QMenuBar` in the window and attaches the same menus to the
//! toolbar. On macOS that menu bar is still the system menu. Items with no
//! native implementation are omitted here rather than shown as dead commands.

use gpui::{App, Menu, MenuItem};

/// One row in a system or toolbar menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuNode {
    pub id: String,
    pub label: String,
    pub checked: bool,
    pub children: Vec<MenuNode>,
}

impl MenuNode {
    fn item(id: &str, label: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.into(),
            checked: false,
            children: Vec::new(),
        }
    }

    fn checked(id: &str, label: impl Into<String>, checked: bool) -> Self {
        Self {
            checked,
            ..Self::item(id, label)
        }
    }

    fn submenu(label: impl Into<String>, children: Vec<MenuNode>) -> Self {
        Self {
            id: String::new(),
            label: label.into(),
            checked: false,
            children,
        }
    }

    fn separator() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            checked: false,
            children: Vec::new(),
        }
    }

    pub fn is_separator(&self) -> bool {
        self.id.is_empty() && self.label.is_empty()
    }
}

/// Checkable settings and route list that the menu bar must reflect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSnapshot {
    pub start_with_system: bool,
    pub remember_last: bool,
    pub allow_lan: bool,
    pub adblock: bool,
    pub warp: bool,
    pub routes: Vec<RouteMenuItem>,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteMenuItem {
    pub id: i64,
    pub name: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSection {
    pub name: &'static str,
    pub items: Vec<MenuNode>,
}

pub fn sections(snapshot: &MenuSnapshot) -> Vec<MenuSection> {
    vec![
        MenuSection {
            name: "Program",
            items: vec![
                MenuNode::item("hide", "Hide window"),
                MenuNode::separator(),
                MenuNode::item("new-profile", "New profile"),
                MenuNode::item("new-auto", "New Auto Selector"),
                MenuNode::item("clipboard", "Add profile from clipboard"),
                MenuNode::item("files", "Add profile from File(s)"),
                MenuNode::separator(),
                MenuNode::checked(
                    "start-with-system",
                    "Start with system",
                    snapshot.start_with_system,
                ),
                MenuNode::checked(
                    "remember-last",
                    "Remember last profile",
                    snapshot.remember_last,
                ),
                MenuNode::checked(
                    "allow-lan",
                    "Allow other devices to connect",
                    snapshot.allow_lan,
                ),
                MenuNode::separator(),
                MenuNode::item("check-update", "Check For Update"),
                MenuNode::item("about", "About"),
                MenuNode::separator(),
                MenuNode::item("restart-core", "Restart Core"),
                MenuNode::item("restart-program", "Restart Program"),
                MenuNode::item("quit", "Exit"),
            ],
        },
        MenuSection {
            name: "Preferences",
            items: vec![
                MenuNode::item("basic", "Basic Settings"),
                MenuNode::item("routing-settings", "Routing Settings"),
                MenuNode::item("tun", "Tun Settings"),
                MenuNode::item("preset", "Preset Settings"),
                MenuNode::item("hotkeys", "Hotkey Settings"),
                MenuNode::item("config-folder", "Open Config Folder"),
                MenuNode::item("clear-proxy", "Clear system proxy now"),
            ],
        },
        MenuSection {
            name: "Server",
            items: vec![
                MenuNode::item("new-profile", "New profile"),
                MenuNode::item("clipboard", "Add profile from clipboard"),
                MenuNode::item("files", "Add profile from File(s)"),
                MenuNode::separator(),
                MenuNode::item("start", "Start"),
                MenuNode::item("stop", "Stop"),
                MenuNode::item("delete", "Delete"),
                MenuNode::separator(),
                MenuNode::item("select-all", "Select All"),
                MenuNode::separator(),
                MenuNode::item("url-test-selected", "Url Test Selected"),
                MenuNode::item("speedtest-selected", "Speedtest Selected"),
                MenuNode::item("resolve-selected-ip", "Resolve Selected Out IP"),
                MenuNode::item("resolve-selected-domain", "Resolve Selected Domain"),
            ],
        },
        MenuSection {
            name: "Routing",
            items: routing_items(snapshot),
        },
        MenuSection {
            name: "Groups",
            items: vec![
                MenuNode::submenu(
                    "Clean up",
                    vec![
                        MenuNode::item("remove-duplicates", "Remove Duplicates"),
                        MenuNode::item("remove-unavailable", "Remove Unavailable"),
                        MenuNode::item("remove-invalid", "Remove Invalid"),
                        MenuNode::item("remove-insecure", "Remove Insecure Configs"),
                    ],
                ),
                MenuNode::item("manage-groups", "Manage Groups"),
                MenuNode::item("add-group", "Add new Group"),
                MenuNode::item("edit-group", "Edit current Group"),
                MenuNode::item("delete-group", "Delete current Group"),
                MenuNode::separator(),
                MenuNode::item("url-test-group", "Url Test Group"),
                MenuNode::item("speedtest-group", "Speedtest Group"),
                MenuNode::item("resolve-group-domain", "Resolve Domain for group"),
                MenuNode::item("clear-group-test", "Clear Group test result"),
                MenuNode::separator(),
                MenuNode::item("update-sub", "Update subscription"),
                MenuNode::item("update-all-subs", "Update all subscriptions"),
            ],
        },
        MenuSection {
            name: "Tools",
            items: vec![
                MenuNode::item("auto-selector-stats", "Auto Selector Stats"),
                MenuNode::item("speedtest-current", "Speedtest Current"),
                MenuNode::separator(),
                MenuNode::item("scanner", "IP Scanner"),
                MenuNode::separator(),
                MenuNode::item("traffic-stats", "Traffic Stats"),
                MenuNode::item("column-widths", "Refresh Column Widths"),
            ],
        },
    ]
}

fn routing_items(snapshot: &MenuSnapshot) -> Vec<MenuNode> {
    let mut items = vec![
        MenuNode::item("routing-settings", "Routing Settings"),
        MenuNode::checked("adblock", "Enable AdBlock", snapshot.adblock),
        MenuNode::checked("warp", "Enable Warp", snapshot.warp),
        MenuNode::separator(),
    ];
    for route in &snapshot.routes {
        items.push(MenuNode::checked(
            &format!("route:{}", route.id),
            route.name.clone(),
            route.active,
        ));
    }
    items
}

pub fn install(cx: &mut App, snapshot: &MenuSnapshot) {
    cx.set_menus(
        sections(snapshot).into_iter().map(|section| {
            Menu::new(section.name).items(section.items.into_iter().map(to_menu_item))
        }),
    );
}

fn to_menu_item(node: MenuNode) -> MenuItem {
    if node.is_separator() {
        return MenuItem::separator();
    }
    if !node.children.is_empty() {
        return MenuItem::submenu(
            Menu::new(node.label).items(node.children.into_iter().map(to_menu_item)),
        );
    }
    MenuItem::action(node.label, SystemCommand { id: node.id }).checked(node.checked)
}

/// Menu command dispatched by the system menu bar and the toolbar dropdowns.
#[derive(Clone, PartialEq, Eq, Debug, gpui::Action)]
#[action(namespace = throne_menu, no_json, name = "SystemCommand")]
pub struct SystemCommand {
    pub id: String,
}

#[cfg(test)]
pub fn labels_of(section: &str, snapshot: &MenuSnapshot) -> Vec<String> {
    sections(snapshot)
        .into_iter()
        .find(|item| item.name == section)
        .map(|item| flatten_labels(&item.items))
        .unwrap_or_default()
}

#[cfg(test)]
fn flatten_labels(nodes: &[MenuNode]) -> Vec<String> {
    let mut labels = Vec::new();
    for node in nodes {
        if node.is_separator() {
            labels.push("---".into());
        } else if !node.children.is_empty() {
            labels.push(node.label.clone());
            labels.extend(flatten_labels(&node.children));
        } else {
            labels.push(node.label.clone());
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MenuSnapshot {
        MenuSnapshot {
            start_with_system: true,
            remember_last: false,
            allow_lan: true,
            adblock: false,
            warp: true,
            routes: vec![
                RouteMenuItem {
                    id: 1,
                    name: "Default".into(),
                    active: false,
                },
                RouteMenuItem {
                    id: 7,
                    name: "Work".into(),
                    active: true,
                },
            ],
            version: "1.4.0".into(),
        }
    }

    #[test]
    fn program_menu_matches_upstream_order_and_checks() {
        let snapshot = sample();
        let program = &sections(&snapshot)[0];
        assert_eq!(
            labels_of("Program", &snapshot),
            vec![
                "Hide window",
                "---",
                "New profile",
                "New Auto Selector",
                "Add profile from clipboard",
                "Add profile from File(s)",
                "---",
                "Start with system",
                "Remember last profile",
                "Allow other devices to connect",
                "---",
                "Check For Update",
                "About",
                "---",
                "Restart Core",
                "Restart Program",
                "Exit",
            ]
        );
        let start = program
            .items
            .iter()
            .find(|item| item.id == "start-with-system")
            .unwrap();
        assert!(start.checked);
        assert!(
            !program
                .items
                .iter()
                .find(|item| item.id == "remember-last")
                .unwrap()
                .checked
        );
    }

    #[test]
    fn groups_menu_nests_cleanup_and_omits_unported_items() {
        let labels = labels_of("Groups", &sample());
        assert_eq!(labels[0], "Clean up");
        assert_eq!(
            &labels[1..5],
            [
                "Remove Duplicates",
                "Remove Unavailable",
                "Remove Invalid",
                "Remove Insecure Configs",
            ]
        );
        let all = sections(&sample())
            .into_iter()
            .flat_map(|section| flatten_labels(&section.items))
            .collect::<Vec<_>>();
        for missing in [
            "Scan QR Code",
            "Integration Settings",
            "OTP Manager",
            "Open Web dashboard",
            "Download Profiles",
            "Update Rule-Sets",
        ] {
            assert!(
                !all.iter().any(|label| label == missing),
                "{missing} has no native command"
            );
        }
    }

    #[test]
    fn routing_menu_checks_adblock_warp_and_the_active_route() {
        let routing = sections(&sample())
            .into_iter()
            .find(|section| section.name == "Routing")
            .unwrap();
        assert!(
            !routing
                .items
                .iter()
                .find(|item| item.id == "adblock")
                .unwrap()
                .checked
        );
        assert!(
            routing
                .items
                .iter()
                .find(|item| item.id == "warp")
                .unwrap()
                .checked
        );
        assert_eq!(
            routing
                .items
                .iter()
                .find(|item| item.id == "route:7")
                .unwrap()
                .label,
            "Work"
        );
        assert!(
            routing
                .items
                .iter()
                .find(|item| item.id == "route:7")
                .unwrap()
                .checked
        );
    }
}
