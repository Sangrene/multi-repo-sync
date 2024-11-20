pub mod parsing {
    use clap::Parser;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::io::Error;

    /// CLI to create PR, releases on multiple repos with the same version.
    #[derive(Parser, Debug)]
    #[command(version, about, long_about = None)]
    struct Args {
        /// JSON config file path
        #[arg(short, long, default_value_t=String::from("config.json"))]
        config: String,
        // Tag
        #[arg(short, long)]
        tag: String
    }

    #[derive(Serialize, Deserialize)]
    pub struct JSONPattern {
        pub title: String,
        pub body: String,
    }

    #[derive(Serialize, Deserialize, Clone)]
    pub struct JSONRepo {
        pub owner: String,
        pub repo: String,
        pub origin: String,
        pub target: String,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Config {
        pub pat: String,
        pub repositories: Vec<JSONRepo>,
        pub pattern: JSONPattern,
    }

    pub struct ParsedArgs {
        pub config: Config,
        pub tag: String,
    }

    pub fn parse_args() -> Result<ParsedArgs, Error> {
        let args = Args::parse();
        println!("Reading config from {}", args.config);
        let file = fs::File::open(args.config)?;
        // let config_content = fs::read_to_string(args.config).expect("Can't read file");
        let parsed_config: Config = serde_json::from_reader(file).unwrap();
        Ok(ParsedArgs {
            config: parsed_config,
            tag: args.tag
        })
    }
}
