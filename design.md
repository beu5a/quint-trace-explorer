# Quint ITF Trace Explorer - Design Plan

A terminal UI tool for exploring Quint/Apalache traces in the Informal Trace Format (ITF).

## Overview

**Goal**: Provide an interactive CLI tool to navigate, inspect, and debug ITF traces produced by Quint and Apalache model checkers.

**Why CLI?**: Useful for quick inspection, CI pipelines, SSH sessions, and environments where VS Code isn't available.

## ITF Format Reference

ITF is a JSON-based trace format. See [ADR-015](https://apalache-mc.org/docs/adr/015adr-trace.html) for full spec.

### Trace Structure

```json
{
  "#meta": { "source": "spec.qnt", "varTypes": { ... } },
  "vars": ["activeTimeouts", "msgBuffer", "system"],
  "states": [
    { "#meta": { "index": 0 }, "activeTimeouts": ..., "msgBuffer": ..., "system": ... },
    { "#meta": { "index": 1 }, ... }
  ],
  "loop": null
}
```

### ITF Value Types

| ITF Form | Meaning | Example |
|----------|---------|---------|
| `{ "#bigint": "123" }` | Integer | `{ "#bigint": "-42" }` |
| `{ "#set": [...] }` | Set | `{ "#set": [1, 2, 3] }` |
| `{ "#map": [[k,v], ...] }` | Map/Function | `{ "#map": [["a", 1], ["b", 2]] }` |
| `{ "#tup": [...] }` | Tuple | `{ "#tup": [1, "hello"] }` |
| `{ "tag": "X", "value": ... }` | Variant | `{ "tag": "Some", "value": 42 }` |
| `[...]` | Sequence/List | `[1, 2, 3]` |
| `{ "field": ... }` (no `#`) | Record | `{ "name": "alice", "age": 30 }` |
| `"string"` | String | `"hello"` |
| `true` / `false` | Boolean | `true` |

---

## UI Layout

```
┌─ Timeline ───────────────────────────────────────────────────┐
│ [0]──[1]──[2]──[3]●─[4]──[5]──[6]──[7]──[8]...               │
├─ State 3/47 ─────────────────────────────────────────────────┤
│ Variables: [x] system  [x] msgBuffer  [ ] activeTimeouts     │
├──────────────────────────────────────────────────────────────┤
│ ▼ msgBuffer: Set(1 item)                                     │
│   + Msg(from: "v1", to: "v2", type: Propose)                 │
│ ▼ system: Map(5 entries) ⚡                                   │
│     ▼ "v1" -> {pendingBlocks: [...], state: [...]}           │
│         ▶ pendingBlocks: [None, None, ...]                   │
│         ▼ state: [Set(...), Set(), ...]                      │
│             Set(Committed(0))                                │
│             Set()                                            │
│     ▶ "v2" -> {pendingBlocks: [...], state: [...]}           │
│     ▶ "v3" -> {pendingBlocks: [...], state: [...]}           │
│     ...                                                      │
└──────────────────────────────────────────────────────────────┘
```

### Layout Components

1. **Timeline bar**: Horizontal view of all states, current state highlighted
2. **State header**: Current state index, total count
3. **Variable toggles**: Show/hide top-level variables
4. **State tree**: Expandable tree view of current state with diff annotations

---

## Navigation

### State Navigation

| Key | Action |
|-----|--------|
| `←` / `h` | Previous state |
| `→` / `l` | Next state |
| `g` | Go to state (prompts for number) |
| `Home` | First state |
| `End` | Last state |

### Tree Navigation

| Key | Action |
|-----|--------|
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Enter` / `→` | Expand node under cursor |
| `←` / `Backspace` | Collapse node (or jump to parent) |

### Other

| Key | Action |
|-----|--------|
| `/` | Search/filter states |
| `v` | Toggle variable visibility menu |
| `q` / `Esc` | Quit |

---

## Tree Expansion Model

### Per-Node Expansion (Tree-Style)

Each expandable node maintains independent collapsed/expanded state.

**Collapsed view** shows truncated preview:

```
▶ system: {"v1": {...}, "v2": {...}, ...}
▶ pendingBlocks: [None, None, ...]
▶ state: [Set(ParentReady(-1)), Set(), ...]
```

**Expanded view** shows children:

```
▼ system: Map(5 entries)
    ▼ "v1" -> {pendingBlocks: [...], state: [...]}
        ▶ pendingBlocks: [None, None, ...]
        ▼ state: [Set(...), Set(), ...]
            Set(ParentReady(-1))
            Set()
    ▶ "v2" -> {pendingBlocks: [...], state: [...]}
```

### Expansion Persistence

- Expansion state is **preserved** when navigating between states
- Collapsed nodes containing changes are marked with `⚡`

---

## Diff Visualization

### Diff Indicators

| Symbol | Color | Meaning |
|--------|-------|---------|
| `+` | Green | Value added (new element in set/map/list, new field) |
| `-` | Red | Value removed |
| (none) | Yellow | Value modified (atomic change) |

### Examples

**Set with added element:**
```
▼ msgBuffer: Set(1 item)
  + Msg(from: "v1", to: "v2", type: Propose)      ← green
```

**Set with removed element:**
```
▼ msgBuffer: Set(0 items)
  - Msg(from: "v1", to: "v2", type: Propose)      ← red
```

**Modified atomic value:**
```
▼ state: List(5 items)
    Set(Committed(0))                             ← yellow (was different)
    Set()
```

**Collapsed node with changes inside:**
```
▶ system: {"v1": {...}, ...} ⚡                    ← ⚡ indicates changes within
```

### Design Rationale

- No "old value" display - avoids complexity of showing nested old state inline
- To see previous value: navigate to previous state with `←`
- Color + symbol is sufficient to identify what changed

---

## Variable Visibility

Toggle which top-level variables are displayed.

```
Variables: [x] system  [x] msgBuffer  [ ] activeTimeouts
```

- `v` opens variable toggle menu
- Space to toggle, Enter to confirm
- Hidden variables are completely omitted from the tree view

---

## Search & Filter

### Filter by Variable Changes

"Show only states where X changed"

```
/ msgBuffer changed
→ Shows: States 3, 7, 12, 45 (filtered view)
```

### Filter by Condition

"Show only states where msgBuffer is non-empty"

```
/ msgBuffer.#set.length > 0
→ Shows: States 3-12, 45-47 (filtered view)
```

### Implementation Note

Pre-compute change index on load:
```
StateIndex {
  state_id: 3,
  changed_vars: ["msgBuffer", "system.v1.state"]
}
```

This enables O(1) filtering rather than O(n) recomputation.

---

## Large Trace Handling

### Scale Expectations

| Dimension | Typical | Large |
|-----------|---------|-------|
| States | 10-50 | 100-1000+ |
| Variables | 3-10 | 10-20 |
| Nesting depth | 3-4 | 5-6 |
| Collection size | 5-20 | 100+ |

### Strategy

1. **Full load**: Load entire trace into memory (defer lazy loading)
2. **Virtual scrolling**: Only render visible portion of tree
3. **Pre-computed diffs**: Build change index on load for fast filtering
4. **Auto-collapse unchanged**: Collapsed by default, expand on demand

### Timeline at Scale

For traces with many states, timeline becomes a scrolling window:

```
# Few states - show all
[0]──[1]──[2]──[3]●─[4]──[5]──[6]──[7]──[8]──[9]

# Many states - scrolling window centered on current
  ...──[45]──[46]──[47]●─[48]──[49]──[50]──...
```

Future enhancement: minimap with activity heatmap (height = number of changes).

---

## Tech Stack

| Component | Choice |
|-----------|--------|
| Language | Rust |
| ITF parsing | `itf` crate (itf-rs) |
| TUI framework | `ratatui` |
| Terminal backend | `crossterm` |
| Diffing | Custom recursive diff on ITF values |

---

## Module Structure (Draft)

```
quint-trace-explorer/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── app.rs               # Application state & event loop
│   ├── itf/
│   │   ├── mod.rs
│   │   ├── loader.rs        # Load & parse ITF JSON
│   │   ├── types.rs         # ITF value types (or re-export from itf crate)
│   │   └── diff.rs          # Compute diff between states
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── timeline.rs      # Timeline bar widget
│   │   ├── tree.rs          # State tree widget
│   │   ├── variables.rs     # Variable toggle widget
│   │   └── render.rs        # ITF value → styled text
│   └── state/
│       ├── mod.rs
│       ├── expansion.rs     # Track expanded/collapsed nodes
│       ├── cursor.rs        # Cursor position in tree
│       └── filter.rs        # Search/filter state
```

---

## Data Model (Draft)

### Expansion State

```rust
struct ExpansionState {
    expanded: HashSet<Vec<PathSegment>>,
}

enum PathSegment {
    Field(String),   // record field or map key
    Index(usize),    // array/tuple/set index
}

impl ExpansionState {
    fn is_expanded(&self, path: &[PathSegment]) -> bool;
    fn toggle(&mut self, path: Vec<PathSegment>);
    fn expand(&mut self, path: Vec<PathSegment>);
    fn collapse(&mut self, path: Vec<PathSegment>);
}
```

### Diff Result

```rust
enum DiffKind {
    Added,
    Removed,
    Modified,
    Unchanged,
}

struct DiffNode {
    kind: DiffKind,
    children_changed: bool,  // for ⚡ indicator on collapsed nodes
}

// Map from path → diff info
type DiffIndex = HashMap<Vec<PathSegment>, DiffNode>;
```

### App State

```rust
struct App {
    trace: ItfTrace,
    current_state: usize,
    expansion: ExpansionState,
    cursor: TreeCursor,
    visible_vars: HashSet<String>,
    diff_cache: Vec<DiffIndex>,  // pre-computed diffs[i] = diff(state[i-1], state[i])
    filter: Option<FilterCriteria>,
}
```

---

## CLI Interface

```bash
# Interactive explorer (main use case)
quint-trace explore trace.itf.json

# Quick inspection commands (future)
quint-trace show trace.itf.json              # dump all states
quint-trace state trace.itf.json 5           # show state 5
quint-trace diff trace.itf.json 3 4          # diff states 3 and 4
quint-trace vars trace.itf.json              # list variables
```

---

## Future Enhancements (Out of Scope for v1)

- [ ] Activity heatmap in timeline (bar height = changes)
- [ ] Semantic grouping ("States 0-5: Init, 6-45: Rounds")
- [ ] Bookmarking states
- [ ] Export filtered view
- [ ] Config file for persistent preferences
- [ ] Side-by-side diff view (optional toggle)
- [ ] Lazy loading for very large traces
