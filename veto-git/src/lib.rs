use std::collections::HashMap;

use git2::*;

#[derive(Debug)]
pub struct RawCommit {
    pub parent_hashes: Vec<String>,
    pub author: String,
    pub timestamp: u64,
    pub message: String,
}


fn walk_all_branches(repo: &Repository) -> HashMap<String, RawCommit>{
    let mut commit_map : HashMap<String, RawCommit> = HashMap::new();
    let mut w = match repo.revwalk(){
        Ok(w) => w,
        Err(e) =>  panic!("Failure creating revwalk: {}", e)
    };

    let _ = w.push_glob("refs/heads/*"); // every local branch
    let _ = w.push_head(); // detached HEAD commits not on any branch

    for commit_id in w{

        let commit_id  = commit_id.unwrap();
        let commit = repo.find_commit(commit_id).unwrap();

        let raw_commit = RawCommit{
            parent_hashes: commit
                .parent_ids()
                .map(|id| id.to_string()).collect(), 
            author: commit
                .author()
                .name()
                .unwrap()
                .to_string(), 
            timestamp:  commit
                .time()
                .seconds() as u64, 
            message: commit
                .message()
                .unwrap()
                .to_string(),
        };

        commit_map.insert(commit_id.to_string(), raw_commit);
    }

    commit_map
}


pub fn  get_commits_from_repo(path: String) -> HashMap<String, RawCommit>{ 
    let repo = match  Repository::open(&path){
        Ok(repo) => repo ,
        Err(e)  => panic!("failed to open: {}" , e)
    };

    walk_all_branches(&repo)
}

/// Local branch (name, tip hash) pairs, sorted by name.
pub fn get_branches(path: String) -> Vec<(String, String)> {
    let repo = match Repository::open(&path){
        Ok(repo) => repo,
        Err(e) => panic!("failed to open: {}", e)
    };

    let mut out = Vec::new();
    if let Ok(branches) = repo.branches(Some(BranchType::Local)) {
        for (branch, _) in branches.flatten() {
            if let (Ok(Some(name)), Some(tip)) = (branch.name(), branch.get().target()) {
                out.push((name.to_string(), tip.to_string()));
            }
        }
    }
    out.sort();
    out
}
