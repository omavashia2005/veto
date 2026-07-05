use veto_git::get_commits_from_repo;
use  veto_graph::*;

fn main() {
    println!("Printing something...");
    let path: String =  String::from("/Users/omavashia/hydra/.git");
    println!("{:?}", get_commits_from_repo(path).len());
}
