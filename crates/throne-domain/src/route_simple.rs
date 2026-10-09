//! Simple-mode route rules — mirrors upstream `RouteProfile::GetSimpleRules` /
//! `UpdateSimpleRules` (`RouteProfile.cpp`).

use crate::models::{outbound_ids, DefaultOutbound, RouteProfile, RouteRule};

/// One selectable Connections routing target, matching upstream 1.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionRouteTarget {
    pub label: String,
    pub rule: String,
    /// Separate a broad TLD rule or process from the narrower domain targets.
    pub separator_before: bool,
}

/// Domain suffixes, service keyword, IP address and process targets for a connection.
pub fn connection_route_targets(
    dest: &str,
    domain: &str,
    process: &str,
) -> Vec<ConnectionRouteTarget> {
    let host = if domain.trim().is_empty() {
        crate::endpoint_host(dest)
    } else {
        domain.trim().to_string()
    };
    let mut targets = Vec::new();
    if !host.is_empty() {
        if host.parse::<std::net::IpAddr>().is_ok() {
            targets.push(ConnectionRouteTarget {
                label: host.clone(),
                rule: format!("ip:{host}"),
                separator_before: false,
            });
        } else {
            let host = host.to_lowercase();
            let labels: Vec<_> = host.split('.').filter(|s| !s.is_empty()).collect();
            for i in 0..labels.len() {
                let level = labels[i..].join(".");
                targets.push(ConnectionRouteTarget {
                    label: format!("*.{level}"),
                    rule: format!("suffix:{level}"),
                    separator_before: i > 0 && i == labels.len() - 1,
                });
            }
            if labels.len() > 1 {
                let name = labels[labels.len() - 2];
                if name.chars().count() >= 4 {
                    targets.insert(
                        targets.len() - 1,
                        ConnectionRouteTarget {
                            label: format!("*{name}*"),
                            rule: format!("keyword:{name}"),
                            separator_before: false,
                        },
                    );
                }
            }
        }
    }
    if !process.trim().is_empty() {
        let process = process.trim();
        let separator_before = !targets.is_empty();
        targets.push(ConnectionRouteTarget {
            label: format!("Process {process}"),
            rule: format!("processName:{process}"),
            separator_before,
        });
    }
    targets
}

/// One of the four simple-mode buckets in the Route Profile editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleAction {
    /// Direct / bypass outbound.
    Bypass,
    Block,
    Proxy,
    WarpBypass,
}

impl SimpleAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bypass => "Direct",
            Self::Block => "Block",
            Self::Proxy => "Proxy",
            Self::WarpBypass => "Warp-bypass",
        }
    }

    fn address_type(self) -> i32 {
        match self {
            Self::Proxy => 1,       // simpleAddressProxy
            Self::Bypass => 2,      // simpleAddressBypass
            Self::Block => 3,       // simpleAddressBlock
            Self::WarpBypass => 10, // simpleAddressWarpBypass
        }
    }

    fn process_name_type(self) -> i32 {
        match self {
            Self::Proxy => 4,
            Self::Bypass => 5,
            Self::Block => 6,
            Self::WarpBypass => 11,
        }
    }

    fn process_path_type(self) -> i32 {
        match self {
            Self::Proxy => 7,
            Self::Bypass => 8,
            Self::Block => 9,
            Self::WarpBypass => 12,
        }
    }

    fn types(self) -> [i32; 3] {
        [
            self.address_type(),
            self.process_name_type(),
            self.process_path_type(),
        ]
    }

    fn outbound_id(self) -> i64 {
        match self {
            Self::Proxy => outbound_ids::PROXY,
            Self::Bypass => outbound_ids::DIRECT,
            Self::Block => outbound_ids::BLOCK,
            Self::WarpBypass => outbound_ids::WARP_BYPASS,
        }
    }

    fn action_token(self) -> &'static str {
        match self {
            Self::Block => "reject",
            _ => "route",
        }
    }

    fn type_name(t: i32) -> &'static str {
        match t {
            1 => "Simple Address Proxy",
            2 => "Simple Address Bypass",
            3 => "Simple Address Block",
            4 => "Simple Process Name Proxy",
            5 => "Simple Process Name Bypass",
            6 => "Simple Process Name Block",
            7 => "Simple Process Path Proxy",
            8 => "Simple Process Path Bypass",
            9 => "Simple Process Path Block",
            10 => "Simple Address Warp-bypass",
            11 => "Simple Process Name Warp-bypass",
            12 => "Simple Process Path Warp-bypass",
            _ => "Custom",
        }
    }
}

impl RouteRule {
    /// Upstream `RouteRule::isEmpty` for simple types (custom uses a lighter check).
    pub fn is_empty_rule(&self) -> bool {
        if self.rule_type != 0 {
            let t = self.rule_type;
            if matches!(t, 1 | 2 | 3 | 10) {
                return self.domain.is_empty()
                    && self.domain_suffix.is_empty()
                    && self.domain_keyword.is_empty()
                    && self.domain_regex.is_empty()
                    && self.rule_set.is_empty()
                    && self.ip_cidr.is_empty();
            }
            return self.process_name.is_empty() && self.process_path.is_empty();
        }
        // custom: empty if no match fields and no special action payload
        self.domain.is_empty()
            && self.domain_suffix.is_empty()
            && self.domain_keyword.is_empty()
            && self.domain_regex.is_empty()
            && self.ip_cidr.is_empty()
            && self.rule_set.is_empty()
            && self.process_name.is_empty()
            && self.process_path.is_empty()
            && self.protocol.is_empty()
            && self.network.is_empty()
            && self.port.is_empty()
            && self.source_port.is_empty()
    }

    fn blank_simple(rule_type: i32, action: SimpleAction) -> Self {
        Self {
            name: SimpleAction::type_name(rule_type).into(),
            rule_type,
            rule_type_token: RouteRule::token_from_type(rule_type).into(),
            outbound_id: action.outbound_id(),
            action: action.action_token().into(),
            ..Default::default()
        }
    }
}

impl RouteProfile {
    /// Serialize simple-mode lines for one action bucket.
    pub fn simple_rules_text(&self, action: SimpleAction) -> String {
        let types = action.types();
        let mut out = String::new();
        for item in &self.rules {
            if !types.contains(&item.rule_type) {
                continue;
            }
            for d in &item.domain {
                out.push_str(&format!("domain:{d}\n"));
            }
            for d in &item.domain_suffix {
                out.push_str(&format!("suffix:{d}\n"));
            }
            for d in &item.domain_keyword {
                out.push_str(&format!("keyword:{d}\n"));
            }
            for d in &item.domain_regex {
                out.push_str(&format!("regex:{d}\n"));
            }
            for d in &item.rule_set {
                out.push_str(&format!("ruleset:{d}\n"));
            }
            for d in &item.ip_cidr {
                out.push_str(&format!("ip:{d}\n"));
            }
            for d in &item.process_name {
                out.push_str(&format!("processName:{d}\n"));
            }
            for d in &item.process_path {
                out.push_str(&format!("processPath:{d}\n"));
            }
        }
        out
    }

    /// Replace simple-mode rules for `action` from multi-line content.
    /// Returns a human-readable error blob (empty on full success).
    pub fn update_simple_rules(&mut self, content: &str, action: SimpleAction) -> String {
        let mut errors = String::new();
        for t in action.types() {
            self.reset_simple_rule(t, action);
        }
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            let Some(rule_type) = classify_simple_line(line, action) else {
                errors.push_str(&format!("invalid rule:{raw}\n"));
                continue;
            };
            let Some(rule) = self.simple_rule_mut(rule_type) else {
                errors.push_str(&format!(
                    "internal error, failed to get rule for: {}\n",
                    SimpleAction::type_name(rule_type)
                ));
                continue;
            };
            if !add_simple_line(line, rule) {
                errors.push_str(&format!("invalid rule:{raw}\n"));
            }
        }
        self.filter_empty_rules();
        errors
    }

    fn reset_simple_rule(&mut self, rule_type: i32, action: SimpleAction) {
        if let Some(r) = self.rules.iter_mut().find(|r| r.rule_type == rule_type) {
            *r = RouteRule::blank_simple(rule_type, action);
            return;
        }
        self.rules.push(RouteRule::blank_simple(rule_type, action));
    }

    fn simple_rule_mut(&mut self, rule_type: i32) -> Option<&mut RouteRule> {
        self.rules.iter_mut().find(|r| r.rule_type == rule_type)
    }

    pub fn filter_empty_rules(&mut self) {
        self.rules.retain(|r| !r.is_empty_rule());
    }

    /// Add one `prefix:value` line to the matching simple rule (upstream `AppendSimpleRule`).
    pub fn append_simple_rule(&mut self, raw: &str, action: SimpleAction) -> Result<(), String> {
        if self.is_raw {
            return Err("cannot edit a raw routing profile".into());
        }
        if self.prevent_modifications {
            return Err("routing profile is locked".into());
        }
        let line = raw.trim();
        if line.is_empty() {
            return Err("empty rule".into());
        }
        let Some(rule_type) = classify_simple_line(line, action) else {
            return Err(format!("invalid rule:{raw}"));
        };
        if self.simple_rule_mut(rule_type).is_none() {
            self.reset_simple_rule(rule_type, action);
        }
        let Some(rule) = self.simple_rule_mut(rule_type) else {
            return Err(format!(
                "internal error, failed to get rule for: {}",
                SimpleAction::type_name(rule_type)
            ));
        };
        if !add_simple_line(line, rule) {
            return Err(format!("invalid rule:{raw}"));
        }
        self.filter_empty_rules();
        Ok(())
    }

    /// Whether the exact line is already in this action's simple rules.
    pub fn has_simple_rule(&self, raw: &str, action: SimpleAction) -> bool {
        let Some((prefix, value)) = simple_line_parts(raw.trim()) else {
            return false;
        };
        let Some(rule_type) = classify_simple_line(raw.trim(), action) else {
            return false;
        };
        self.rules.iter().any(|rule| {
            rule.rule_type == rule_type
                && simple_rule_values(rule, prefix).is_some_and(|values| values.contains(&value))
        })
    }

    /// Remove all copies, including imported duplicate simple rules, without touching other rules.
    pub fn remove_simple_rule(&mut self, raw: &str, action: SimpleAction) -> Result<bool, String> {
        if self.is_raw {
            return Err("cannot edit a raw routing profile".into());
        }
        if self.prevent_modifications {
            return Err("routing profile is locked".into());
        }
        let Some((prefix, value)) = simple_line_parts(raw.trim()) else {
            return Ok(false);
        };
        let Some(rule_type) = classify_simple_line(raw.trim(), action) else {
            return Ok(false);
        };
        let mut removed = false;
        self.rules.retain_mut(|rule| {
            if rule.rule_type != rule_type {
                return true;
            }
            let Some(values) = simple_rule_values_mut(rule, prefix) else {
                return true;
            };
            let old_len = values.len();
            values.retain(|stored| stored != &value);
            if values.len() == old_len {
                return true;
            }
            removed = true;
            !rule.is_empty_rule()
        });
        Ok(removed)
    }

    /// Earlier rule from another action that shadows every host of this suffix/keyword.
    /// Identical lines do not shadow: the Connections toggle moves them between actions.
    pub fn covering_simple_rule(
        &self,
        raw: &str,
        action: SimpleAction,
    ) -> Option<(String, SimpleAction)> {
        let (prefix, value) = simple_line_parts(raw.trim())?;
        if !matches!(prefix, "suffix" | "keyword") {
            return None;
        }
        for rule in &self.rules {
            if rule.rule_type == action.address_type() {
                break;
            }
            let other = match rule.rule_type {
                1 => SimpleAction::Proxy,
                2 => SimpleAction::Bypass,
                3 => SimpleAction::Block,
                10 => SimpleAction::WarpBypass,
                _ => continue,
            };
            if other == action {
                continue;
            }
            for keyword in &rule.domain_keyword {
                let k = keyword.to_lowercase();
                if !k.is_empty() && value.contains(&k) && !(prefix == "keyword" && k == value) {
                    return Some((format!("keyword:{keyword}"), other));
                }
            }
            if prefix == "suffix" {
                for suffix in &rule.domain_suffix {
                    let s = suffix.to_lowercase();
                    if !s.is_empty() && value.ends_with(&format!(".{s}")) {
                        return Some((format!("suffix:{suffix}"), other));
                    }
                }
            }
        }
        None
    }

    /// True when there are no rules and no raw body (upstream `IsEmpty`).
    pub fn is_empty_profile(&self) -> bool {
        if self.is_raw {
            return self.raw_route.trim().is_empty();
        }
        self.rules.is_empty()
    }

    /// Ensure a default dns-hijack rule exists when the profile would otherwise be empty.
    pub fn ensure_default_dns_hijack(&mut self) {
        if !self.is_empty_profile() {
            return;
        }
        self.rules.push(RouteRule {
            name: "dns-hijack".into(),
            protocol: "dns".into(),
            action: "hijack-dns".into(),
            outbound_id: DefaultOutbound::Proxy.as_id(),
            ..Default::default()
        });
    }

    /// Fresh structured profile matching upstream `GetDefaultChain`.
    pub fn default_chain(id: i64) -> Self {
        let mut p = Self::new(id, "Default");
        p.rules.push(RouteRule {
            name: "Route DNS".into(),
            protocol: "dns".into(),
            action: "hijack-dns".into(),
            outbound_id: DefaultOutbound::Proxy.as_id(),
            ..Default::default()
        });
        p
    }
}

fn classify_simple_line(line: &str, action: SimpleAction) -> Option<i32> {
    match simple_line_parts(line)?.0 {
        "domain" | "suffix" | "keyword" | "regex" | "ruleset" | "ip" => Some(action.address_type()),
        "processName" => Some(action.process_name_type()),
        "processPath" => Some(action.process_path_type()),
        _ => None,
    }
}

fn simple_line_parts(line: &str) -> Option<(&str, String)> {
    let (prefix, rest) = line.split_once(':')?;
    let prefix = prefix.trim();
    let rest = rest.trim();
    // sing-box lowercases the host before matching but takes these values as written (1.3.1).
    // Regex stays as written: lowercasing a pattern can change it (`\D` is not `\d`).
    let value = match prefix {
        "domain" | "suffix" | "keyword" => rest.to_lowercase(),
        _ => rest.to_string(),
    };
    if value.is_empty() {
        return None;
    }
    Some((prefix, value))
}

fn simple_rule_values<'a>(rule: &'a RouteRule, prefix: &str) -> Option<&'a Vec<String>> {
    match prefix {
        "domain" => Some(&rule.domain),
        "suffix" => Some(&rule.domain_suffix),
        "keyword" => Some(&rule.domain_keyword),
        "regex" => Some(&rule.domain_regex),
        "ruleset" => Some(&rule.rule_set),
        "ip" => Some(&rule.ip_cidr),
        "processName" => Some(&rule.process_name),
        "processPath" => Some(&rule.process_path),
        _ => None,
    }
}

fn simple_rule_values_mut<'a>(
    rule: &'a mut RouteRule,
    prefix: &str,
) -> Option<&'a mut Vec<String>> {
    match prefix {
        "domain" => Some(&mut rule.domain),
        "suffix" => Some(&mut rule.domain_suffix),
        "keyword" => Some(&mut rule.domain_keyword),
        "regex" => Some(&mut rule.domain_regex),
        "ruleset" => Some(&mut rule.rule_set),
        "ip" => Some(&mut rule.ip_cidr),
        "processName" => Some(&mut rule.process_name),
        "processPath" => Some(&mut rule.process_path),
        _ => None,
    }
}

fn add_simple_line(line: &str, rule: &mut RouteRule) -> bool {
    let Some((prefix, value)) = simple_line_parts(line) else {
        return false;
    };
    let Some(values) = simple_rule_values_mut(rule, prefix) else {
        return false;
    };
    push_unique(values, value);
    true
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.iter().any(|x| x == &value) {
        list.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_simple_direct_rules() {
        let mut p = RouteProfile::new(1, "t");
        let err = p.update_simple_rules(
            "domain:a.com\nsuffix:cn\nip:1.1.1.1/32\nprocessName:curl",
            SimpleAction::Bypass,
        );
        assert!(err.is_empty(), "{err}");
        let text = p.simple_rules_text(SimpleAction::Bypass);
        assert!(text.contains("domain:a.com"));
        let err = p.update_simple_rules("domain:Example.COM\nregex:\\D+", SimpleAction::Proxy);
        assert!(err.is_empty(), "{err}");
        let proxy = p.rules.iter().find(|r| r.rule_type == 1).unwrap();
        assert_eq!(proxy.domain, vec!["example.com".to_string()]);
        assert_eq!(proxy.domain_regex, vec!["\\D+".to_string()]);
        assert!(text.contains("suffix:cn"));
        assert!(text.contains("ip:1.1.1.1/32"));
        assert!(text.contains("processName:curl"));
        assert_eq!(
            p.rules
                .iter()
                .find(|r| r.rule_type == 2)
                .map(|r| r.outbound_id),
            Some(outbound_ids::DIRECT)
        );
    }

    #[test]
    fn invalid_line_reported() {
        let mut p = RouteProfile::new(1, "t");
        let err = p.update_simple_rules("nope:x", SimpleAction::Proxy);
        assert!(err.contains("invalid rule"));
    }

    #[test]
    fn append_simple_rule_adds_one_domain_line() {
        let mut p = RouteProfile::new(1, "t");
        p.append_simple_rule("domain:cdn.example.com", SimpleAction::Proxy)
            .unwrap();
        p.append_simple_rule("domain:cdn.example.com", SimpleAction::Proxy)
            .unwrap();
        let text = p.simple_rules_text(SimpleAction::Proxy);
        assert_eq!(text.matches("domain:cdn.example.com").count(), 1);
    }

    #[test]
    fn remove_simple_rule_removes_imported_duplicates_and_preserves_unrelated_empty_rules() {
        let mut p = RouteProfile::new(1, "t");
        p.append_simple_rule("suffix: Example.COM ", SimpleAction::Proxy)
            .unwrap();
        p.rules.push(p.rules[0].clone());
        p.rules
            .push(RouteRule::blank_simple(2, SimpleAction::Bypass));
        assert!(p.has_simple_rule("suffix:EXAMPLE.com", SimpleAction::Proxy));
        assert!(p
            .remove_simple_rule("suffix:EXAMPLE.com", SimpleAction::Proxy)
            .unwrap());
        assert!(!p.has_simple_rule("suffix:example.com", SimpleAction::Proxy));
        assert_eq!(p.rules.len(), 1);
        assert_eq!(p.rules[0].rule_type, 2);
        assert!(!p
            .remove_simple_rule("suffix:example.com", SimpleAction::Proxy)
            .unwrap());
    }

    #[test]
    fn covering_rules_respect_order_label_boundaries_and_identical_lines() {
        let mut p = RouteProfile::new(1, "t");
        p.append_simple_rule("suffix:github.com", SimpleAction::Bypass)
            .unwrap();
        assert_eq!(
            p.covering_simple_rule("suffix:api.github.com", SimpleAction::Proxy),
            Some(("suffix:github.com".into(), SimpleAction::Bypass))
        );
        assert_eq!(
            p.covering_simple_rule("suffix:mygithub.com", SimpleAction::Proxy),
            None
        );
        assert_eq!(
            p.covering_simple_rule("suffix:github.com", SimpleAction::Proxy),
            None
        );
        p.append_simple_rule("keyword:github", SimpleAction::Block)
            .unwrap();
        assert_eq!(
            p.covering_simple_rule("suffix:githubusercontent.com", SimpleAction::Proxy),
            Some(("keyword:github".into(), SimpleAction::Block))
        );
        assert_eq!(
            p.covering_simple_rule("keyword:github", SimpleAction::Proxy),
            None
        );
        p.append_simple_rule("suffix:api.github.com", SimpleAction::Proxy)
            .unwrap();
        p.rules.rotate_right(1);
        assert_eq!(
            p.covering_simple_rule("suffix:api.github.com", SimpleAction::Proxy),
            None
        );
    }

    #[test]
    fn locked_or_raw_routes_cannot_be_toggled() {
        let mut p = RouteProfile::new(1, "t");
        p.append_simple_rule("ip:1.1.1.1", SimpleAction::Proxy)
            .unwrap();
        p.prevent_modifications = true;
        assert!(p
            .remove_simple_rule("ip:1.1.1.1", SimpleAction::Proxy)
            .is_err());
        p.prevent_modifications = false;
        p.is_raw = true;
        assert!(p
            .remove_simple_rule("ip:1.1.1.1", SimpleAction::Proxy)
            .is_err());
        assert!(p.has_simple_rule("ip:1.1.1.1", SimpleAction::Proxy));
    }

    #[test]
    fn connection_targets_offer_suffix_levels_keyword_and_separated_tld() {
        let targets = connection_route_targets("1.1.1.1:443", "Tile.OpenStreetMap.Org.", "curl");
        let rules: Vec<_> = targets.iter().map(|t| t.rule.as_str()).collect();
        assert_eq!(
            rules,
            [
                "suffix:tile.openstreetmap.org",
                "suffix:openstreetmap.org",
                "keyword:openstreetmap",
                "suffix:org",
                "processName:curl"
            ]
        );
        assert!(targets[3].separator_before);
        assert!(targets[4].separator_before);
        assert!(connection_route_targets("bbc.co.uk:443", "", "")
            .iter()
            .all(|t| !t.rule.starts_with("keyword:")));
        assert_eq!(
            connection_route_targets("[2001:db8::1]:443", "", "")[0].rule,
            "ip:2001:db8::1"
        );
    }
}
