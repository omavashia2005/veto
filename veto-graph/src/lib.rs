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
        raw_commits: HashMap<String, RawCommit>, 
    ) -> HashMap<String, Rc<RefCell<CommitNode>>>{

        let mut nodes: HashMap<String, Rc<RefCell<CommitNode>>> =  HashMap::new();

        for (hash, raw) in  raw_commits{

            let node = Rc::new(RefCell::new(CommitNode{
                hash: hash.clone(), 
                short_hash: hash[..7].to_string(), 
                message: raw.message, 
                timestamp: raw.timestamp, 
                parents: RefCell::new(Vec::new()),
                children: RefCell::new(Vec::new()),
            }));

            nodes.insert(hash.clone(), node);
        }

        nodes
    }
}
