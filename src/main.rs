use octocrab::Octocrab;
use parsing::parsing::ParsedArgs;

mod parsing;
mod github;

#[tokio::main]
async fn main() {
    let args= parsing::parsing::parse_args().unwrap();
    let octocrab = github::github::create_octocrab(&args.config.pat).unwrap();
    github::github::set_all_repos(octocrab, args.config, args.tag).await;
}