use veto_git::RawCommit;
use std::cell::RefCell;
use std::collections::{HashMap};
use std::rc::{Rc, Weak};


pub struct CommitNode{
    pub hash: String, 
    pub short_hash: String, 
    pub message: String, 
    pub timestamp: u64,
    pub parents: RefCell<Vec<Weak<RefCell<CommitNode>>>>, 
    pub children: RefCell<Vec<Weak<RefCell<CommitNode>>>>,
}

pub struct CommitGraph;

impl CommitGraph {
    
    pub fn build(
        raw_commits: &HashMap<String, RawCommit>, 
    ) -> HashMap<String, Rc<RefCell<CommitNode>>>{

        let mut nodes: HashMap<String, Rc<RefCell<CommitNode>>> =  HashMap::new();

        for (hash, raw) in  raw_commits{

            let node = Rc::new(RefCell::new(CommitNode{
                hash: hash.clone(), 
                short_hash: hash[..7].to_string(), 
                message: raw.message.clone(), 
                timestamp: raw.timestamp, 
                parents: RefCell::new(Vec::new()),
                children: RefCell::new(Vec::new()),
            }));

            nodes.insert(hash.clone(), node);
        }

        nodes
    }

    //  In the second pass, we walk through every node and 
    //  wire up its parent and child edges using Weak references

    pub fn connect_edges(
        nodes: &HashMap<String, Rc<RefCell<CommitNode>>>, 
        raw_commits:  &HashMap<String, RawCommit>,
    ){
        for  (hash, node) in  nodes{
            if  let Some(raw) =  raw_commits.get(hash){
                for parent_hash in &raw.parent_hashes{
                    if let Some(parent_node) = nodes.get(parent_hash){
                        node.borrow() //  immutable borrow of RefCell<CommitNode>
                            .parents
                            .borrow_mut() // mutable borrow  (inner mutability) of the CommitNode itself
                            .push(Rc::downgrade(parent_node));

                        parent_node.borrow()
                            .children
                            .borrow_mut()
                            .push(Rc::downgrade(node));
                    }
                }

            }
        }
    }

}
