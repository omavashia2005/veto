use std::io::Write;
use veto_git::get_commits_from_repo;
use veto_graph::CommitGraph;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        print!("git repo path> ");
        std::io::stdout().flush().unwrap();
        let mut s = String::new();
        std::io::stdin().read_line(&mut s).unwrap();
        s.trim().to_string()
    });

    let branches = veto_git::get_branches(path.clone());
    let raw_commits = get_commits_from_repo(path);
    let nodes = CommitGraph::build(&raw_commits);
    CommitGraph::connect_edges(&nodes, &raw_commits);

    veto_ui::run(&nodes, &branches).unwrap();
}
