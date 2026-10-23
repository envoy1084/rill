//! The history graph `rill log` draws, jj-style: `@` the working change,
//! `○` an unlanded change, `◆` a trunk landing, `×` a conflicted version,
//! joined by lines. Edges follow first parents, so a stack of changes
//! branches off trunk and joins it where it is based. Pure layout: rows
//! of cells the caller colours.

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// The checkout's working change.
    Working,
    /// A change not landed yet.
    Change,
    /// A trunk landing.
    Trunk,
    /// A version with conflicts.
    Conflict,
}

impl NodeKind {
    /// The glyph drawn for the node.
    pub fn glyph(self) -> char {
        match self {
            NodeKind::Working => '@',
            NodeKind::Change => '○',
            NodeKind::Trunk => '◆',
            NodeKind::Conflict => '×',
        }
    }
}

/// A node to place, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The node's key.
    pub id: String,
    /// Its first parent's key.
    pub parent: Option<String>,
    /// What it is.
    pub kind: NodeKind,
}

/// One cell of a drawn row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    /// The row's node.
    Node(NodeKind),
    /// A lane passing through (`│`).
    Line,
    /// A lane joining from the right (`├─╯` pieces).
    Join(char),
    /// Nothing.
    Empty,
}

impl Cell {
    /// The cell's two characters.
    pub fn text(self) -> String {
        match self {
            Cell::Node(k) => format!("{} ", k.glyph()),
            Cell::Line => "│ ".into(),
            Cell::Join(c) => format!("{c}{}", if c == '╯' { ' ' } else { '─' }),
            Cell::Empty => "  ".into(),
        }
    }
}

/// The rows for one node: its own, then connector rows after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows {
    /// The node's row.
    pub node: Vec<Cell>,
    /// Connector rows drawn after it.
    pub after: Vec<Vec<Cell>>,
}

/// Place every node. A node whose parent is not in the list ends its lane.
pub fn layout(nodes: &[Node]) -> Vec<Rows> {
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        let col = match lanes
            .iter()
            .position(|l| l.as_deref() == Some(node.id.as_str()))
        {
            Some(c) => c,
            None => match lanes.iter().position(Option::is_none) {
                Some(free) => free,
                None => {
                    lanes.push(None);
                    lanes.len() - 1
                }
            },
        };
        let joining: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(j, l)| *j != col && l.as_deref() == Some(node.id.as_str()))
            .map(|(j, _)| j)
            .collect();
        let row: Vec<Cell> = (0..lanes.len())
            .map(|j| {
                if j == col {
                    Cell::Node(node.kind)
                } else if lanes[j].is_some() {
                    Cell::Line
                } else {
                    Cell::Empty
                }
            })
            .collect();
        lanes[col] = node.parent.clone();
        let mut after = Vec::new();
        if let Some(&last) = joining.iter().max() {
            let connector = (0..lanes.len())
                .map(|j| {
                    if j == col {
                        Cell::Join('├')
                    } else if joining.contains(&j) && j == last {
                        Cell::Join('╯')
                    } else if joining.contains(&j) || (j > col && j < last) {
                        Cell::Join('─')
                    } else if lanes[j].is_some() {
                        Cell::Line
                    } else {
                        Cell::Empty
                    }
                })
                .collect();
            after.push(connector);
            for j in &joining {
                lanes[*j] = None;
            }
        }
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        out.push(Rows { node: row, after });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, kind: NodeKind) -> Node {
        Node {
            id: id.into(),
            parent: parent.map(Into::into),
            kind,
        }
    }

    fn draw(rows: &[Rows]) -> Vec<String> {
        let line = |cells: &[Cell]| {
            cells
                .iter()
                .map(|c| c.text())
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        rows.iter()
            .flat_map(|r| std::iter::once(line(&r.node)).chain(r.after.iter().map(|a| line(a))))
            .collect()
    }

    #[test]
    fn a_stack_branches_off_trunk_and_joins_at_its_base() {
        let rows = layout(&[
            node("w", Some("c"), NodeKind::Working),
            node("c", Some("t1"), NodeKind::Change),
            node("t2", Some("t1"), NodeKind::Trunk),
            node("t1", Some("t0"), NodeKind::Trunk),
            node("t0", None, NodeKind::Trunk),
        ]);
        assert_eq!(
            draw(&rows),
            ["@", "○", "│ ◆", "◆ │", "├─╯", "◆"],
            "{rows:?}"
        );
    }

    #[test]
    fn linear_history_is_one_lane() {
        let rows = layout(&[
            node("b", Some("a"), NodeKind::Trunk),
            node("a", None, NodeKind::Trunk),
        ]);
        assert_eq!(draw(&rows), ["◆", "◆"]);
    }
}
