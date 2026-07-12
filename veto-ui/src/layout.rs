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
pub fn layout(_nodes: &HashMap<String, Rc<RefCell<CommitNode>>>) -> Vec<Cell> {
    Vec::new() // ponytail: stub, implemented in layout pass
}
