use veto_git::get_commits_from_repo;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::rc::{Rc, Weak};


struct CommitNode{
    hash: String, 
    short_hash: String, 
    message: String, 
    parents: RefCell<Vec<Weak<RefCell<CommitNode>>>>, 
    children: RefCell<Vec<Weak<RefCell<CommitNode>>>>,
}
