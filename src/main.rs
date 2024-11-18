mod parsing;
mod github;

#[tokio::main]
async fn main() {
    let args = parsing::parsing::parse_args().unwrap();
    let octocrab = github::github::create_octocrab(&args.config.pat).unwrap();
    let repos = github::github::get_all_repos(&octocrab, &args.config).await.unwrap();
    github::github::create_prs(octocrab, &args.config, &repos);
    println!("Repo number {}", repos.len());
}