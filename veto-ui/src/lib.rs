use std::{cell::RefCell, collections::HashMap, io, rc::Rc};
use veto_graph::CommitNode;

pub mod layout;

/// Take over the terminal and run the interactive commit-graph explorer.
/// h/l/j/k traverse the graph, i shows node metadata, q quits.
pub fn run(_nodes: &HashMap<String, Rc<RefCell<CommitNode>>>) -> io::Result<()> {
    Ok(()) // ponytail: stub, implemented in tui pass
}
