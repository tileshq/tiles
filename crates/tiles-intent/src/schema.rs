//! The JSON Schema of a chart intent, as the MCP tool publishes it (ADR-003).
//!
//! It is built from the same constants and enums the parser checks against
//! ([`crate::MAX_TITLE_CHARS`], [`crate::ClaimKind::ALL`], the source limits),
//! and the form list comes from the engine's form files through [`FormEntry`],
//! so the schema a model reads and the parser that judges it cannot drift. The
//! tests below hold the two together: every key the schema names is one the
//! parser accepts, every key it requires is one the parser requires, and every
//! limit it states is the limit the parser enforces.

use serde_json::{Value, json};

use crate::source::{
    DEFAULT_OMNI_LIMIT, MAX_COLUMNS as MAX_OMNI_FIELDS, MAX_NAME_CHARS, MAX_SQL_BYTES,
};
use crate::{
    ClaimKind, MAX_HIGHLIGHT, MAX_NOTE_CHARS, MAX_ROWS, MAX_SUBTITLE_CHARS, MAX_TEXT_CHARS,
    MAX_TITLE_CHARS, SCHEMA_VERSION, Tone, Unit,
};

/// One role a form declares, as the schema describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleEntry {
    /// The key under `fields`.
    pub name: String,
    /// Whether the form needs it.
    pub required: bool,
    /// What the column must hold and mean.
    pub doc: String,
}

/// One chart form, as the schema describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormEntry {
    /// The value of `form`.
    pub name: String,
    /// When to use it.
    pub summary: String,
    /// The roles it fills.
    pub roles: Vec<RoleEntry>,
}

/// The JSON Schema of an intent for the given forms.
#[must_use]
#[allow(clippy::too_many_lines)] // One JSON literal; split up, the schema would no longer read as one document.
pub fn intent_schema(forms: &[FormEntry]) -> Value {
    let form_names: Vec<&str> = forms.iter().map(|f| f.name.as_str()).collect();
    let mut role_names: Vec<&str> = forms
        .iter()
        .flat_map(|f| f.roles.iter().map(|r| r.name.as_str()))
        .collect();
    role_names.sort_unstable();
    role_names.dedup();
    let mut role_props = serde_json::Map::new();
    for role in role_names {
        let users: Vec<&str> = forms
            .iter()
            .filter(|f| f.roles.iter().any(|r| r.name == role))
            .map(|f| f.name.as_str())
            .collect();
        role_props.insert(
            role.to_string(),
            json!({
                "type": "string",
                "maxLength": MAX_TEXT_CHARS,
                "description": format!("The column for the `{role}` role ({})", users.join(", ")),
            }),
        );
    }
    let text = |max: usize, description: &str| json!({ "type": "string", "maxLength": max, "description": description });
    let mut schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["v", "form", "title", "claim", "fields", "data"],
        "properties": {
            "v": { "type": "integer", "enum": [SCHEMA_VERSION], "description": "Schema version, always 1; this key and the others sit at the top level of the call, not nested under another key" },
            "form": {
                "type": "string",
                "enum": form_names,
                "description": "The chart form; each form's roles are listed in the tool description",
            },
            "title": text(MAX_TITLE_CHARS, "The finding in words, such as \"Signups dipped in W3\". Plain text"),
            "subtitle": text(MAX_SUBTITLE_CHARS, "Optional second line: the unit, period or source"),
            "claim": {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "tone"],
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ClaimKind::ALL.iter().map(|k| k.name()).collect::<Vec<_>>(),
                        "description": "What kind of finding the title states",
                    },
                    "tone": {
                        "type": "string",
                        "enum": Tone::ALL.iter().map(|t| t.name()).collect::<Vec<_>>(),
                        "description": "Whether the finding is good, bad or neither; it picks the accent colour",
                    },
                },
            },
            "fields": {
                "type": "object",
                "description": "Which column fills each role the form declares",
                "properties": role_props,
                "additionalProperties": { "type": "string", "maxLength": MAX_TEXT_CHARS },
            },
            "highlight": {
                "type": "array",
                "maxItems": MAX_HIGHLIGHT,
                "items": { "type": ["string", "number"] },
                "description": "The values the title names, exactly as they appear in the data. Only these marks get the accent",
            },
            "data": {
                "description": "Exactly one source: inline rows, an Omni topic Tiles queries itself, or a warehouse SELECT Tiles checks and runs itself",
                "oneOf": [
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["rows"],
                        "properties": {
                            "rows": {
                                "type": "array",
                                "minItems": 1,
                                "maxItems": MAX_ROWS,
                                "items": {
                                    "type": "object",
                                    "additionalProperties": { "type": ["string", "number", "null"] },
                                },
                                "description": "Aggregated rows, one object per row keyed by column name. Give full values (128000, not 128 for $128K); Tiles abbreviates them",
                            },
                        },
                    },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["omni"],
                        "properties": {
                            "omni": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["topic", "fields"],
                                "properties": {
                                    "topic": { "type": "string", "maxLength": MAX_NAME_CHARS, "description": "A topic name as `tiles omni topics` lists it" },
                                    "fields": {
                                        "type": "array",
                                        "minItems": 1,
                                        "maxItems": MAX_OMNI_FIELDS,
                                        "items": { "type": "string", "maxLength": MAX_NAME_CHARS },
                                        "description": "Fully qualified fields (view.field, view.date[week]) as `tiles omni fields` lists them; the chart's fields name these",
                                    },
                                    "sorts": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "additionalProperties": false,
                                            "required": ["field"],
                                            "properties": {
                                                "field": { "type": "string", "maxLength": MAX_NAME_CHARS },
                                                "desc": { "type": "boolean" },
                                            },
                                        },
                                    },
                                    "limit": {
                                        "type": "integer",
                                        "minimum": 1,
                                        "maximum": MAX_ROWS,
                                        "description": format!("Most rows to fetch, {DEFAULT_OMNI_LIMIT} when absent"),
                                    },
                                },
                            },
                        },
                    },
                ],
            },
        },
    });
    // The warehouse branch and the optional presentation keys (ADR-012,
    // ADR-031), added apart because one more nested literal passes the json!
    // macro's recursion limit.
    if let Some(sources) = schema["properties"]["data"]["oneOf"].as_array_mut() {
        sources.push(warehouse_branch());
    }
    schema["properties"]["unit"] = json!({
        "type": "string",
        "enum": Unit::ALL.iter().map(|u| u.name()).collect::<Vec<_>>(),
        "description": "Optional: the currency or percent sign written on every value, tick and tooltip of the y measure (31%, $262K, €8.2M). Leave it out for counts such as users or orders. Give values in full units, not thousands or millions; percent values are given in points (4.1 for 4.1%, not 0.041)",
    });
    schema["properties"]["note"] = text(
        MAX_NOTE_CHARS,
        "Optional short callout beside the first highlighted mark, such as \"Checkout bug, 4 to 9 Nov\". Needs highlight; refused where it cannot fit",
    );
    schema
}

/// The `data.warehouse` branch of the schema.
fn warehouse_branch() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["warehouse"],
        "properties": {
            "warehouse": {
                "type": "object",
                "additionalProperties": false,
                "required": ["sql"],
                "properties": {
                    "sql": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": MAX_SQL_BYTES,
                        "description": "One GoogleSQL SELECT; CTEs and joins allowed. Write a dbt model as {{ ref('fct_x') }}, which Tiles reads from target's build, and any other table as `project.dataset.table`. Name each result column the chart's fields use",
                    },
                    "target": {
                        "type": "string",
                        "pattern": "^(prod|dev|mr:[0-9]{1,9}(:full)?)$",
                        "description": "The dbt build the {{ ref(...) }} models are read from: prod, dev (the user's own build), mr:<iid> or mr:<iid>:full. Needed with a ref, refused without one; if the user has not said which, ask",
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_ROWS,
                        "description": format!("Most rows to fetch, {DEFAULT_OMNI_LIMIT} when absent"),
                    },
                },
            },
        },
    })
}

/// The form list in words, one form per line with its roles, for a tool
/// description a model reads.
#[must_use]
pub fn forms_text(forms: &[FormEntry]) -> String {
    forms
        .iter()
        .map(|f| {
            let roles: Vec<String> = f
                .roles
                .iter()
                .map(|r| {
                    if r.required {
                        r.name.clone()
                    } else {
                        format!("{}?", r.name)
                    }
                })
                .collect();
            format!("- {} ({}): {}", f.name, roles.join(", "), f.summary)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{FormEntry, RoleEntry, forms_text, intent_schema};
    use crate::{ClaimKind, MAX_ROWS, Tone, Unit, parse, parse_request};

    fn forms() -> Vec<FormEntry> {
        vec![FormEntry {
            name: "column".into(),
            summary: "Values over a few periods".into(),
            roles: vec![
                RoleEntry {
                    name: "x".into(),
                    required: true,
                    doc: "Periods".into(),
                },
                RoleEntry {
                    name: "y".into(),
                    required: true,
                    doc: "Values".into(),
                },
            ],
        }]
    }

    fn full() -> Value {
        json!({
            "v": 1, "form": "column", "title": "Signups dipped in W3", "subtitle": "Weekly",
            "claim": { "kind": "change", "tone": "bad" },
            "fields": { "x": "week", "y": "signups" },
            "highlight": ["W3"], "unit": "percent", "note": "Checkout bug",
            "data": { "rows": [{ "week": "W3", "signups": 298 }] }
        })
    }

    fn props(schema: &Value) -> Vec<String> {
        let mut keys: Vec<String> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn schema_names_exactly_the_keys_the_parser_reads() {
        let schema = intent_schema(&forms());
        let mut intent_keys: Vec<String> = full().as_object().unwrap().keys().cloned().collect();
        intent_keys.sort();
        assert_eq!(props(&schema), intent_keys);
        assert!(parse(&full().to_string()).is_ok());
    }

    /// The keys the parser accepts, read from its own refusal of an unknown
    /// key, so a key added to the parser fails this test until the schema
    /// names it too.
    fn parser_keys(json: &Value, at: &[&str]) -> Vec<String> {
        let mut probe = json.clone();
        let mut target = &mut probe;
        for k in at {
            target = &mut target[*k];
        }
        target["zz_probe"] = json!(1);
        let problem = parse_request(&probe.to_string()).unwrap_err().problem;
        let list = problem
            .split("expected one of ")
            .nth(1)
            .or_else(|| problem.split("expected ").nth(1))
            .unwrap();
        let list = list.split(" at line").next().unwrap();
        let mut keys: Vec<String> = list
            .split(", ")
            .flat_map(|k| k.split(" or "))
            .map(|k| k.trim_matches('`').to_string())
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn schema_keys_are_the_parser_keys_at_every_level() {
        let schema = intent_schema(&forms());
        // The top level and the claim take any key and leave out the ones
        // they do not read, so their keys are the parser's own list.
        let mut keys: Vec<String> = crate::KEYS.iter().map(|k| (*k).to_string()).collect();
        keys.sort();
        assert_eq!(props(&schema), keys);
        assert_eq!(props(&schema["properties"]["claim"]), ["kind", "tone"]);
        let mut probe = full();
        probe["zz_probe"] = json!(1);
        probe["claim"]["zz_probe"] = json!(1);
        let read = parse_request(&probe.to_string()).unwrap();
        assert_eq!(
            read.notes,
            [
                "Tiles left out the keys it does not take: zz_probe.",
                "Tiles left out the claim keys it does not take: zz_probe."
            ]
        );
        let omni = json!({ "v": 1, "form": "column", "title": "t", "claim": { "kind": "rank", "tone": "good" },
            "fields": {}, "data": { "omni": { "topic": "orders", "fields": ["orders.count"] } } });
        let branch = &schema["properties"]["data"]["oneOf"][1]["properties"]["omni"];
        assert_eq!(props(branch), parser_keys(&omni, &["data", "omni"]));
        let warehouse = json!({ "v": 1, "form": "column", "title": "t", "claim": { "kind": "rank", "tone": "good" },
            "fields": {}, "data": { "warehouse": { "sql": "SELECT 1 AS n" } } });
        let branch = &schema["properties"]["data"]["oneOf"][2]["properties"]["warehouse"];
        assert_eq!(
            props(branch),
            parser_keys(&warehouse, &["data", "warehouse"])
        );
        let sources: Vec<String> = schema["properties"]["data"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(props)
            .collect();
        assert_eq!(sources, ["rows", "omni", "warehouse"]);
    }

    #[test]
    fn schema_requires_exactly_what_the_parser_requires() {
        let schema = intent_schema(&forms());
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for key in props(&schema) {
            let mut intent = full();
            intent.as_object_mut().unwrap().remove(&key);
            // A note sits beside a highlighted mark, so it leaves with the highlight.
            if key == "highlight" {
                intent.as_object_mut().unwrap().remove("note");
            }
            // A model is asked for these, but without them the card still
            // draws: version 1, the measure as the title, the default claim.
            let filled = ["v", "title", "claim"].contains(&key.as_str());
            let refused = parse(&intent.to_string()).is_err();
            assert_eq!(
                refused,
                required.contains(&key.as_str()) && !filled,
                "dropping `{key}`"
            );
        }
    }

    #[test]
    fn stated_text_limits_are_the_enforced_limits() {
        let schema = intent_schema(&forms());
        for key in ["title", "subtitle", "note"] {
            let max =
                usize::try_from(schema["properties"][key]["maxLength"].as_u64().unwrap()).unwrap();
            let mut at = full();
            at[key] = json!("a".repeat(max));
            assert!(parse(&at.to_string()).is_ok(), "{key} at the limit");
            at[key] = json!("a".repeat(max + 1));
            let cut = parse(&at.to_string()).unwrap();
            assert!(
                cut.notes
                    .contains(&format!("Tiles cut {key} to {max} characters.")),
                "{key} past the limit is cut: {:?}",
                cut.notes
            );
        }
    }

    #[test]
    fn stated_row_and_highlight_limits_are_the_enforced_limits() {
        let schema = intent_schema(&forms());
        let rows = &schema["properties"]["data"]["oneOf"][0]["properties"]["rows"];
        let max_rows = usize::try_from(rows["maxItems"].as_u64().unwrap()).unwrap();
        let mut intent = full();
        intent["data"]["rows"] = Value::Array(vec![json!({"week": "W3", "signups": 1}); max_rows]);
        assert!(parse(&intent.to_string()).is_ok());
        intent["data"]["rows"] =
            Value::Array(vec![json!({"week": "W3", "signups": 1}); max_rows + 1]);
        assert_eq!(parse(&intent.to_string()).unwrap_err().key, "data.rows");
        let max_hl = usize::try_from(
            schema["properties"]["highlight"]["maxItems"]
                .as_u64()
                .unwrap(),
        )
        .unwrap();
        let mut intent = full();
        intent["highlight"] = Value::Array(vec![json!("W3"); max_hl + 1]);
        let kept = parse(&intent.to_string()).unwrap();
        assert_eq!(kept.highlight.len(), max_hl, "the first are kept");
    }

    #[test]
    fn every_listed_claim_and_tone_parses() {
        let schema = intent_schema(&forms());
        let claim = &schema["properties"]["claim"]["properties"];
        assert_eq!(
            claim["kind"]["enum"].as_array().unwrap().len(),
            ClaimKind::ALL.len()
        );
        assert_eq!(
            claim["tone"]["enum"].as_array().unwrap().len(),
            Tone::ALL.len()
        );
        for kind in ClaimKind::ALL {
            for tone in Tone::ALL {
                let mut intent = full();
                intent["claim"] = json!({ "kind": kind.name(), "tone": tone.name() });
                let parsed = parse(&intent.to_string()).unwrap();
                assert_eq!((parsed.claim.kind, parsed.claim.tone), (kind, tone));
            }
        }
    }

    #[test]
    fn the_omni_branch_parses_as_a_request() {
        let schema = intent_schema(&forms());
        let omni = &schema["properties"]["data"]["oneOf"][1]["properties"]["omni"];
        let mut keys: Vec<&String> = omni["properties"].as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["fields", "limit", "sorts", "topic"]);
        let mut intent = full();
        intent["fields"] = json!({ "x": "orders.week", "y": "orders.count" });
        intent["data"] = json!({ "omni": {
            "topic": "orders", "fields": ["orders.week", "orders.count"],
            "sorts": [{ "field": "orders.week", "desc": false }], "limit": 12
        }});
        let request = parse_request(&intent.to_string()).unwrap();
        assert_eq!(request.omni().unwrap().topic, "orders");
    }

    #[test]
    fn the_warehouse_branch_parses_as_a_request_within_its_limits() {
        let schema = intent_schema(&forms());
        let warehouse = &schema["properties"]["data"]["oneOf"][2]["properties"]["warehouse"];
        assert_eq!(warehouse["required"], json!(["sql"]));
        let max = usize::try_from(
            warehouse["properties"]["sql"]["maxLength"]
                .as_u64()
                .unwrap(),
        )
        .unwrap();
        let mut intent = full();
        intent["fields"] = json!({ "x": "month", "y": "amount" });
        let sql = "SELECT month, amount FROM {{ ref('fct_payments') }} -- ";
        intent["data"] = json!({ "warehouse": {
            "sql": format!("{sql}{}", "x".repeat(max - sql.len())), "target": "mr:4521:full", "limit": 12
        }});
        let request = parse_request(&intent.to_string()).unwrap();
        assert_eq!(request.warehouse().unwrap().limit, Some(12));
        intent["data"]["warehouse"]["sql"] =
            json!(format!("{sql}{}", "x".repeat(max + 1 - sql.len())));
        assert_eq!(
            parse_request(&intent.to_string()).unwrap_err().key,
            "data.warehouse.sql"
        );
        let pattern = warehouse["properties"]["target"]["pattern"]
            .as_str()
            .unwrap();
        assert_eq!(pattern, "^(prod|dev|mr:[0-9]{1,9}(:full)?)$");
        let limit = &warehouse["properties"]["limit"];
        assert_eq!(limit["maximum"], json!(MAX_ROWS));
        intent["data"]["warehouse"]["sql"] = json!(sql);
        intent["data"]["warehouse"]["limit"] = json!(MAX_ROWS + 1);
        assert_eq!(
            parse_request(&intent.to_string()).unwrap_err().key,
            "data.warehouse.limit"
        );
    }

    #[test]
    fn forms_list_marks_optional_roles() {
        let mut forms = forms();
        forms[0].roles[1].required = false;
        assert_eq!(
            forms_text(&forms),
            "- column (x, y?): Values over a few periods"
        );
        let schema = intent_schema(&forms);
        assert_eq!(schema["properties"]["form"]["enum"], json!(["column"]));
        assert!(schema["properties"]["fields"]["properties"]["x"].is_object());
    }

    #[test]
    fn every_listed_unit_parses() {
        let schema = intent_schema(&forms());
        let units = schema["properties"]["unit"]["enum"].as_array().unwrap();
        assert_eq!(units.len(), Unit::ALL.len());
        for unit in Unit::ALL {
            let mut intent = full();
            intent["unit"] = json!(unit.name());
            assert_eq!(parse(&intent.to_string()).unwrap().unit, Some(unit));
        }
    }
}
