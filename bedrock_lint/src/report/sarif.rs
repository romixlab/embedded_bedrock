//! The report as SARIF 2.1.0 (OASIS), for GitHub code scanning and other SARIF viewers: one run, a rule per check
//! that ran or found something (id = catalogue id, help = its catalogue line), a result per finding with the
//! fingerprint as a partial fingerprint, waived findings as in-source suppressions, the baseline state when the run
//! was compared, checks that did not run as notifications. Paths are relative to the firmware crate, the
//! `FWROOT` base.

use super::Report;
use crate::catalogue::catalogue;
use crate::finding::{BaselineState, Finding, Severity};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The schema URI SARIF files name
pub const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    id: String,
    short_description: Value,
    full_description: Value,
    help_uri: String,
    default_configuration: Value,
    properties: Value,
}

fn level(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Suggestion => "note",
    }
}

/// The report as a SARIF log, pretty-printed
pub fn render(report: &Report) -> String {
    let cat = catalogue();
    let ids: BTreeSet<&str> = report
        .ran
        .iter()
        .map(String::as_str)
        .chain(report.findings.iter().map(|f| f.check.as_str()))
        .collect();
    let mut ids: Vec<&str> = ids.into_iter().collect();
    ids.sort_by_key(|id| crate::catalogue::order_key(id));
    let rules: Vec<Rule> = ids
        .iter()
        .map(|id| {
            let e = cat.entry(id);
            let title = e.map(|e| e.plain_title()).unwrap_or_default();
            Rule {
                id: id.to_string(),
                short_description: json!({ "text": title }),
                full_description: json!({
                    "text": e.map(|e| format!("{} (check: {})", e.plain_title(), e.check)).unwrap_or_default()
                }),
                help_uri: format!(
                    "https://github.com/romixlab/embedded_bedrock/blob/main/{}",
                    e.map(|e| e.link()).unwrap_or_default()
                ),
                default_configuration: json!({
                    "level": e.map_or("warning", |e| level(e.severity))
                }),
                properties: json!({ "tags": [id.split('-').next().unwrap_or(id)] }),
            }
        })
        .collect();
    let results: Vec<Value> = report.findings.iter().map(|f| result(f, &ids)).collect();
    let mut notifications: Vec<Value> = report
        .not_run
        .iter()
        .map(|n| {
            json!({
                "level": "note",
                "message": { "text": format!("{} not run: {}", n.check, n.reason) },
                "associatedRule": { "id": n.check },
            })
        })
        .collect();
    notifications.extend(report.notes.iter().map(|n| {
        json!({
            "level": "warning",
            "message": { "text": format!("{}: {}", n.file, n.message) },
        })
    }));
    notifications.extend(report.problems.iter().map(|p| {
        json!({
            "level": "error",
            "message": { "text": format!("{}:{}: {}", p.file, p.line, p.message) },
            "locations": [physical(&p.file, p.line, 0)],
        })
    }));
    let dir = report.dir.trim_end_matches('/');
    let log = json!({
        "$schema": SCHEMA,
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "bedrock-lint",
                "semanticVersion": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/romixlab/embedded_bedrock",
                "rules": rules,
            }},
            "originalUriBaseIds": { "FWROOT": { "uri": format!("file://{dir}/") } },
            "invocations": [{
                "executionSuccessful": report.problems.is_empty(),
                "toolExecutionNotifications": notifications,
            }],
            "results": results,
        }],
    });
    serde_json::to_string_pretty(&log).expect("sarif serializes") + "\n"
}

fn physical(file: &str, line: u32, column: u32) -> Value {
    let mut region = serde_json::Map::new();
    if line > 0 {
        region.insert("startLine".into(), line.into());
        if column > 0 {
            region.insert("startColumn".into(), column.into());
        }
    }
    let mut loc = json!({
        "physicalLocation": {
            "artifactLocation": { "uri": file, "uriBaseId": "FWROOT" },
        }
    });
    if !region.is_empty() {
        loc["physicalLocation"]["region"] = Value::Object(region);
    }
    loc
}

fn result(f: &Finding, ids: &[&str]) -> Value {
    let mut spans = f.spans.iter();
    let locations: Vec<Value> = spans
        .next()
        .map(|s| physical(&s.file, s.line, s.column))
        .into_iter()
        .collect();
    let related: Vec<Value> = spans
        .enumerate()
        .map(|(i, s)| {
            let mut l = physical(&s.file, s.line, s.column);
            l["id"] = (i + 1).into();
            l["message"] = json!({ "text": s.label });
            l
        })
        .collect();
    let mut text = f.message.clone();
    if let Some(fix) = &f.fix {
        text.push_str(". Fix: ");
        text.push_str(fix);
    }
    let mut r = json!({
        "ruleId": f.check,
        "ruleIndex": ids.iter().position(|i| *i == f.check),
        "level": level(f.severity),
        "message": { "text": text },
        "locations": locations,
        "partialFingerprints": { "bedrockLint/v1": f.fingerprint },
        "properties": { "confidence": f.confidence.name(), "item": f.item },
    });
    if !related.is_empty() {
        r["relatedLocations"] = Value::Array(related);
    }
    if let Some(reason) = &f.waived {
        r["suppressions"] = json!([{ "kind": "inSource", "justification": reason }]);
    }
    if let Some(b) = f.baseline {
        r["baselineState"] = match b {
            BaselineState::New => "new",
            BaselineState::Unchanged => "unchanged",
        }
        .into();
    }
    r
}
