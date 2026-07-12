use std::{cell::RefCell, collections::HashMap, rc::Rc};
use veto_graph::CommitNode;

/// Grid position of one commit. row 0 = newest commit; lane = branch column.
pub struct Cell {
    pub hash: String,
    pub lane: usize,
    pub row: usize,
}

/// Lay commits out on a lane/row grid (git log --graph style).
/// Returned Vec is sorted by row ascending.
pub fn layout(nodes: &HashMap<String, Rc<RefCell<CommitNode>>>) -> Vec<Cell> {
    let mut order: Vec<&Rc<RefCell<CommitNode>>> = nodes.values().collect();
    // newest first, hash tie-break for determinism
    order.sort_by(|a, b| {
        let (a, b) = (a.borrow(), b.borrow());
        b.timestamp.cmp(&a.timestamp).then(a.hash.cmp(&b.hash))
    });

    // active[i] = Some(hash) means lane i is waiting for that parent commit
    let mut active: Vec<Option<String>> = Vec::new();
    let mut cells = Vec::with_capacity(order.len());

    for (row, node) in order.iter().enumerate() {
        let node = node.borrow();
        // ponytail: O(rows*lanes) linear scans; fine at git-log scale
        let lane = match active.iter().position(|l| l.as_deref() == Some(&node.hash)) {
            Some(lane) => {
                // merging: free every other lane waiting for this commit
                for l in active.iter_mut().skip(lane + 1) {
                    if l.as_deref() == Some(&node.hash) {
                        *l = None;
                    }
                }
                lane
            }
            None => take_free_lane(&mut active), // branch tip
        };

        let parents: Vec<String> = node
            .parents
            .borrow()
            .iter()
            .filter_map(|w| w.upgrade().map(|p| p.borrow().hash.clone()))
            .collect();

        match parents.split_first() {
            Some((first, rest)) => {
                active[lane] = Some(first.clone()); // first parent inherits the lane
                for extra in rest {
                    let l = take_free_lane(&mut active);
                    active[l] = Some(extra.clone());
                }
            }
            None => active[lane] = None, // root closes its lane
        }

        cells.push(Cell { hash: node.hash.clone(), lane, row });
    }

    cells
}

/// Leftmost free slot, growing the lane list if none.
fn take_free_lane(active: &mut Vec<Option<String>>) -> usize {
    match active.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            active.push(None);
            active.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(hash: &str, timestamp: u64) -> Rc<RefCell<CommitNode>> {
        Rc::new(RefCell::new(CommitNode {
            hash: hash.to_string(),
            short_hash: hash.to_string(),
            message: String::new(),
            timestamp,
            parents: RefCell::new(Vec::new()),
            children: RefCell::new(Vec::new()),
        }))
    }

    fn link(child: &Rc<RefCell<CommitNode>>, parent: &Rc<RefCell<CommitNode>>) {
        child.borrow().parents.borrow_mut().push(Rc::downgrade(parent));
        parent.borrow().children.borrow_mut().push(Rc::downgrade(child));
    }

    #[test]
    fn diamond() {
        // ponytail: RawCommit isn't reachable from veto-ui deps, wire nodes by hand
        let (a, b, c, d) = (node("a", 1), node("b", 2), node("c", 3), node("d", 4));
        link(&b, &a);
        link(&c, &a);
        link(&d, &b); // first parent
        link(&d, &c);
        let nodes: HashMap<_, _> = [&a, &b, &c, &d]
            .into_iter()
            .map(|n| (n.borrow().hash.clone(), n.clone()))
            .collect();

        let cells = layout(&nodes);

        // rows: timestamp descending, one per row
        let hashes: Vec<&str> = cells.iter().map(|c| c.hash.as_str()).collect();
        assert_eq!(hashes, ["d", "c", "b", "a"]);
        assert_eq!(cells.iter().map(|c| c.row).collect::<Vec<_>>(), [0, 1, 2, 3]);

        // the two branch commits sit on different lanes
        let lane = |h: &str| cells.iter().find(|c| c.hash == h).unwrap().lane;
        assert_ne!(lane("b"), lane("c"));
        assert_eq!(lane("d"), lane("b")); // first parent inherits the merge's lane
        assert_eq!(lane("a"), lane("b")); // merge point takes the leftmost waiting lane
    }
}
