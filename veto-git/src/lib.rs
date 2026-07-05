use std::{collections::HashMap, panic};

use git2::*;

#[derive(Debug)]
pub struct RawCommit {
    pub parent_hashes: Vec<String>,
    pub author: String,
    pub timestamp: u64,
    pub message: String,
}


fn walk_main(repo: &Repository, head_name: &str) -> HashMap<String, RawCommit>{ 
    let mut commit_map : HashMap<String, RawCommit> = HashMap::new();
    let mut w = match repo.revwalk(){
        Ok(w) => w, 
        Err(e) =>  panic!("Failure resolving head ref name: {}", e)
    };
    
    let _ = w.push_ref(head_name);

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

    let repo_head: Reference =  match repo.head(){
        Ok(repo_head) => repo_head,
        Err(e) => panic!("Error resolving head: {}", e)
    };

    let head_name =  match repo_head.name(){
        Ok(head_name) => head_name, 
        Err(e) =>  panic!("Error reading head name :{}", e)
    };

    walk_main(&repo, head_name)
}
