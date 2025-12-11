use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// A parsed ITF trace using itf::Value for state values
pub struct Trace {
    pub meta: itf::trace::Meta,
    pub vars: Vec<String>,
    pub states: Vec<State>,
    pub loop_index: Option<u64>,
}

/// A single state in the trace
pub struct State {
    pub index: u64,
    pub values: HashMap<String, itf::Value>,
}

/// Raw trace structure for initial JSON parsing
/// This avoids the flatten + untagged serde issue in the itf crate
#[derive(Deserialize)]
struct RawTrace {
    #[serde(rename = "#meta")]
    meta: itf::trace::Meta,
    #[serde(default)]
    vars: Vec<String>,
    states: Vec<serde_json::Value>,
    #[serde(rename = "loop")]
    loop_index: Option<u64>,
}

/// Load an ITF trace from a JSON file
pub fn load_trace(path: &Path) -> Result<Trace> {
    let contents = fs::read_to_string(path)
        .context(format!("Failed to read file: {}", path.display()))?;

    // Step 1: Parse the trace structure (avoids the flatten + untagged issue)
    let raw: RawTrace = serde_json::from_str(&contents)
        .context("Failed to parse ITF JSON structure")?;

    // Step 2: Convert each state's variable values to itf::Value
    let states: Vec<State> = raw
        .states
        .into_iter()
        .enumerate()
        .map(|(i, state_json)| parse_state(i, state_json))
        .collect::<Result<Vec<_>>>()?;

    Ok(Trace {
        meta: raw.meta,
        vars: raw.vars,
        states,
        loop_index: raw.loop_index,
    })
}

/// Parse a single state from its JSON representation
fn parse_state(index: usize, json: serde_json::Value) -> Result<State> {
    let mut values = HashMap::new();

    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            // Skip #meta - it's metadata, not a variable
            if key == "#meta" {
                continue;
            }

            // Convert each variable's value to itf::Value
            let itf_value: itf::Value = serde_json::from_value(val.clone())
                .context(format!("Failed to parse variable '{}' in state {}", key, index))?;

            values.insert(key.clone(), itf_value);
        }
    }

    Ok(State {
        index: index as u64,
        values,
    })
}

// ============================================================================
// RUST CONCEPT: Tests module
//
// #[cfg(test)] means this module only compiles when running `cargo test`
// #[test] marks individual test functions
// assert! and assert_eq! are macros for checking conditions
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper to get path to example traces
    fn example_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name)
    }

    #[test]
    fn test_load_missionaries_and_cannibals() {
        let path = example_path("MissionariesAndCannibals.itf.json");
        let trace = load_trace(&path).expect("Failed to load trace");

        // Check metadata
        assert_eq!(
            trace.meta.source,
            Some("MC_MissionariesAndCannibalsTyped.tla".to_string())
        );

        // Check variables
        assert_eq!(trace.vars, vec!["bank_of_boat", "who_is_on_bank"]);

        // Check state count
        assert_eq!(trace.states.len(), 6);

        // Check first state has expected variables
        let state0 = &trace.states[0];
        assert!(state0.values.contains_key("bank_of_boat"));
        assert!(state0.values.contains_key("who_is_on_bank"));

        // Check bank_of_boat is a string "E" in state 0
        if let itf::Value::String(s) = &state0.values["bank_of_boat"] {
            assert_eq!(s, "E");
        } else {
            panic!("Expected bank_of_boat to be a String");
        }

        // Check who_is_on_bank is a Map
        assert!(matches!(
            &state0.values["who_is_on_bank"],
            itf::Value::Map(_)
        ));
    }

    #[test]
    fn test_load_sum_types() {
        let path = example_path("SumTypes0.itf.json");
        let trace = load_trace(&path).expect("Failed to load trace");

        // Check metadata
        assert_eq!(trace.meta.source, Some("SumTypes.qnt".to_string()));
        assert_eq!(trace.meta.format, Some("ITF".to_string()));

        // Check variables
        assert_eq!(trace.vars, vec!["value"]);

        // Check state count
        assert_eq!(trace.states.len(), 3);

        // State 0: value = { tag: "None", value: {} }
        let state0 = &trace.states[0];
        if let itf::Value::Record(rec) = &state0.values["value"] {
            // Check tag field
            if let Some(itf::Value::String(tag)) = rec.get("tag") {
                assert_eq!(tag, "None");
            } else {
                panic!("Expected tag to be a String");
            }
        } else {
            panic!("Expected value to be a Record");
        }

        // State 1: value = { tag: "Some", value: 40 }
        let state1 = &trace.states[1];
        if let itf::Value::Record(rec) = &state1.values["value"] {
            if let Some(itf::Value::String(tag)) = rec.get("tag") {
                assert_eq!(tag, "Some");
            }
            if let Some(itf::Value::BigInt(n)) = rec.get("value") {
                assert_eq!(n.to_string(), "40");
            } else {
                panic!("Expected value field to be a BigInt");
            }
        }
    }

    #[test]
    fn test_load_propeller() {
        let path = example_path("propeller.itf.json");
        let trace = load_trace(&path).expect("Failed to load trace");

        // Check metadata
        assert_eq!(trace.meta.source, Some("propeller.qnt".to_string()));

        // Check variables
        assert_eq!(trace.vars, vec!["propeller::choreo::s"]);

        // Check state count
        assert_eq!(trace.states.len(), 16);

        // Check that all states have the expected variable
        for (i, state) in trace.states.iter().enumerate() {
            assert!(
                state.values.contains_key("propeller::choreo::s"),
                "State {} missing expected variable",
                i
            );
        }
    }

    #[test]
    fn test_load_decide_non_proposer() {
        let path = example_path("DecideNonProposerTest0.itf.json");
        let trace = load_trace(&path).expect("Failed to load trace");

        // Just verify it loads without error and has states
        assert!(!trace.states.is_empty());
        assert!(!trace.vars.is_empty());
    }

    #[test]
    fn test_load_test_insufficient_success() {
        let path = example_path("TestInsufficientSuccess9.itf.json");
        let trace = load_trace(&path).expect("Failed to load trace");

        // Just verify it loads without error and has states
        assert!(!trace.states.is_empty());
        assert!(!trace.vars.is_empty());
    }

    #[test]
    fn test_nonexistent_file() {
        let path = example_path("nonexistent.itf.json");
        let result = load_trace(&path);
        assert!(result.is_err());
    }
}
