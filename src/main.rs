mod github;
mod parsing;

#[tokio::main]
async fn main() {
    let args = parsing::parsing::parse_args().unwrap();
    let octocrab = github::github::create_octocrab(&args.config.pat).unwrap();
    match github::github::set_all_repos(octocrab, args.config, args.tag).await {
        Ok(()) => {
            println!("Sucessfully setting up all repos")
        }
        Err(error) => {
            panic!("Error setting up repos {:?}", error);
        }
    }
}
