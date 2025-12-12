use std::collections::{HashMap, HashSet};

use crate::tree::NodePath;

/// What changed at a node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Unchanged,
    Added,
    Removed,
    Modified,
}

/// Diff information for the whole state
pub struct DiffResult {
    pub changes: HashMap<NodePath, DiffKind>,
}

impl DiffResult {
    pub fn get(&self, path: &NodePath) -> DiffKind {
        self.changes.get(path).copied().unwrap_or(DiffKind::Unchanged)
    }

}

/// Compare two states and return what changed
pub fn compute_diff(
    prev: &HashMap<String, itf::Value>,
    curr: &HashMap<String, itf::Value>,
) -> DiffResult {
    let mut changes = HashMap::new();

    let prev_keys: HashSet<_> = prev.keys().collect();
    let curr_keys: HashSet<_> = curr.keys().collect();

    for key in prev_keys.difference(&curr_keys) {
        changes.insert(vec![(*key).clone()], DiffKind::Removed);
    }

    for key in curr_keys.difference(&prev_keys) {
        changes.insert(vec![(*key).clone()], DiffKind::Added);
    }

    for key in prev_keys.intersection(&curr_keys) {
        let path = vec![(*key).clone()];
        diff_value(&prev[*key], &curr[*key], path, &mut changes);
    }

    DiffResult { changes }
}

/// Recursively diff two values
fn diff_value(
    prev: &itf::Value,
    curr: &itf::Value,
    path: NodePath,
    changes: &mut HashMap<NodePath, DiffKind>,
) {
    if prev == curr {
        return;
    }

    match (prev, curr) {
        (itf::Value::Record(prev_fields), itf::Value::Record(curr_fields)) => {
            changes.insert(path.clone(), DiffKind::Modified);
            diff_record(prev_fields, curr_fields, path, changes);
        }
        (itf::Value::Map(prev_pairs), itf::Value::Map(curr_pairs)) => {
            changes.insert(path.clone(), DiffKind::Modified);
            diff_map(prev_pairs, curr_pairs, path, changes);
        }
        (itf::Value::Set(prev_items), itf::Value::Set(curr_items)) => {
            changes.insert(path.clone(), DiffKind::Modified);
            diff_set(prev_items, curr_items, path, changes);
        }
        _ => {
            changes.insert(path, DiffKind::Modified);
        }
    }
}

fn diff_record(
    prev: &itf::value::Record,
    curr: &itf::value::Record,
    path: NodePath,
    changes: &mut HashMap<NodePath, DiffKind>,
) {
    let prev_keys: HashSet<_> = prev.iter().map(|(k, _)| k).collect();
    let curr_keys: HashSet<_> = curr.iter().map(|(k, _)| k).collect();

    for key in prev_keys.difference(&curr_keys) {
        let mut child_path = path.clone();
        child_path.push((*key).clone());
        changes.insert(child_path, DiffKind::Removed);
    }

    for key in curr_keys.difference(&prev_keys) {
        let mut child_path = path.clone();
        child_path.push((*key).clone());
        changes.insert(child_path, DiffKind::Added);
    }

    for key in prev_keys.intersection(&curr_keys) {
        if let (Some(prev_val), Some(curr_val)) = (prev.get(*key), curr.get(*key)) {
            let mut child_path = path.clone();
            child_path.push((*key).clone());
            diff_value(prev_val, curr_val, child_path, changes);
        }
    }
}

fn diff_map(
    prev: &itf::value::Map<itf::Value, itf::Value>,
    curr: &itf::value::Map<itf::Value, itf::Value>,
    path: NodePath,
    changes: &mut HashMap<NodePath, DiffKind>,
) {
    let prev_map: HashMap<_, _> = prev.iter().collect();
    let curr_vec: Vec<_> = curr.iter().collect();

    for (i, (curr_key, curr_val)) in curr_vec.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(format!("{}", i));

        match prev_map.get(curr_key) {
            Some(prev_val) => {
                if *prev_val != *curr_val {
                    diff_value(*prev_val, *curr_val, child_path, changes);
                }
            }
            None => {
                changes.insert(child_path, DiffKind::Added);
            }
        }
    }
}

fn diff_set(
    prev: &itf::value::Set<itf::Value>,
    curr: &itf::value::Set<itf::Value>,
    path: NodePath,
    changes: &mut HashMap<NodePath, DiffKind>,
) {
    let prev_vec: Vec<_> = prev.iter().collect();
    let curr_vec: Vec<_> = curr.iter().collect();

    for (i, curr_item) in curr_vec.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(format!("{}", i));

        let exists_in_prev = prev_vec.iter().any(|p| p == curr_item);
        if !exists_in_prev {
            changes.insert(child_path, DiffKind::Added);
        }
    }
}
