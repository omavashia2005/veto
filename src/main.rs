use veto_git::get_commits_from_repo;
use  veto_graph::*;

fn main() {
    println!("Printing something...");
    let path: String =  String::from("/Users/omavashia/hydra/.git");
    let raw_commits =  get_commits_from_repo(path); 

    // build the graph
    let nodes = CommitGraph::build(&raw_commits);

    // connect edges
    CommitGraph::connect_edges(&nodes, &raw_commits);
}
