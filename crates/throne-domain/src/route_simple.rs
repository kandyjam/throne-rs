//! Simple-mode route rules — mirrors upstream `RouteProfile::GetSimpleRules` /
//! `UpdateSimpleRules` (`RouteProfile.cpp`).

use crate::models::{DefaultOutbound, RouteProfile, RouteRule, outbound_ids};

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
        self.rules
            .push(RouteRule::blank_simple(rule_type, action));
    }

    fn simple_rule_mut(&mut self, rule_type: i32) -> Option<&mut RouteRule> {
        self.rules.iter_mut().find(|r| r.rule_type == rule_type)
    }

    pub fn filter_empty_rules(&mut self) {
        self.rules.retain(|r| !r.is_empty_rule());
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
    if line.starts_with("domain")
        || line.starts_with("suffix")
        || line.starts_with("keyword")
        || line.starts_with("regex")
        || line.starts_with("ruleset")
        || line.starts_with("ip")
    {
        return Some(action.address_type());
    }
    if line.starts_with("processName") {
        return Some(action.process_name_type());
    }
    if line.starts_with("processPath") {
        return Some(action.process_path_type());
    }
    None
}

fn add_simple_line(line: &str, rule: &mut RouteRule) -> bool {
    let Some((prefix, rest)) = line.split_once(':') else {
        return false;
    };
    let value = rest.to_string();
    if value.is_empty() {
        return false;
    }
    match prefix {
        "domain" => push_unique(&mut rule.domain, value),
        "suffix" => push_unique(&mut rule.domain_suffix, value),
        "keyword" => push_unique(&mut rule.domain_keyword, value),
        "regex" => push_unique(&mut rule.domain_regex, value),
        "ruleset" => push_unique(&mut rule.rule_set, value),
        "ip" => push_unique(&mut rule.ip_cidr, value),
        "processName" => push_unique(&mut rule.process_name, value),
        "processPath" => push_unique(&mut rule.process_path, value),
        _ => return false,
    }
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
}
