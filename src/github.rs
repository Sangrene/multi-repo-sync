pub mod github {

    use regex::Regex;

    use super::super::parsing::parsing::Config;
    use octocrab::{Error, Octocrab};

    enum RepoType {
        Python,
        Node,
    }


    trait GithubClient<T> {
        fn init(&self, c: Config) -> Result<T, Error>;
    }

    struct OctocrabClient {

    }

    impl GithubClient<Octocrab> for OctocrabClient{
        fn init(&self, config: Config) -> Result<Octocrab, Error> {
            let octocrab = Octocrab::builder().personal_token(config.pat).build()?;
            return Ok(octocrab);
        }
    }

    struct RepoAnalysis<'a> {
        language: RepoType,
        file_to_detect: &'a str,
        file_content_update_fn: fn(content: String, version: &String) -> String,
    }

    const FILE_TO_LANGUAGE: [RepoAnalysis; 2] = [
        RepoAnalysis {
            file_to_detect: "package.json",
            language: RepoType::Node,
            file_content_update_fn: |content, version| {
                let re = Regex::new(r#""version":.+\n"#).unwrap();
                return re
                    .replace(&content, format!(r#""version": "{}"\n"#, version))
                    .to_string();
            },
        },
        RepoAnalysis {
            file_to_detect: "pyproject.toml",
            language: RepoType::Python,
            file_content_update_fn: |content, version| {
                let re = Regex::new(r"version\s=.+\n").unwrap();
                return re
                    .replace(&content, format!(r#"version = "{}"\n"#, version))
                    .to_string();
            },
        },
    ];

    fn get_repo_with_file_to_update<'a>(
        files: &Vec<octocrab::models::repos::Content>,
        version: &'a String,
    ) -> Option<(RepoAnalysis<'a>, String, String)> {
        for item in files {
            for analysis in FILE_TO_LANGUAGE {
                if item.name.eq(analysis.file_to_detect) {
                    let new_content =
                        (analysis.file_content_update_fn)(item.decoded_content().unwrap(), version);
                    return Some((analysis, new_content, item.sha.clone()));
                }
            }
        }
        return None;
    }

    pub fn create_octocrab(pat: &str) -> Result<Octocrab, Error> {
        let octocrab = Octocrab::builder().personal_token(pat).build()?;
        return Ok(octocrab);
    }

    pub async fn get_root_file_list(
        octocrab: &Octocrab,
        owner: &String,
        repo: &String,
    ) -> Result<Vec<octocrab::models::repos::Content>, Error> {
        let content = octocrab.repos(owner, repo).get_content().send().await?;
        return Ok(content.items);
    }

    async fn create_pr(
        octocrab: &Octocrab,
        owner: &String,
        repo: &String,
        title: &String,
        origin: &String,
        target: &String,
        body: &String,
    ) -> Result<(u64, String), Error> {
        let pr_result = octocrab
            .pulls(owner, repo)
            .create(title, origin, target)
            .body(body)
            .send()
            .await?;

        return Ok((pr_result.number, pr_result.head.sha));
    }

    pub async fn update_file_version(
        octocrab: &Octocrab,
        owner: &String,
        repo: &String,
        path: &str,
        content: &String,
        sha: &String,
        branch: &String,
    ) -> Result<(), Error> {
        octocrab
            .repos(owner, repo)
            .update_file(path, "Bumping version", content, sha)
            .branch(branch)
            .send()
            .await?;

        return Ok(());
    }

    pub async fn merge_branch(
        octocrab: &Octocrab,
        owner: &String,
        repo: &String,
        pr_number: u64,
    ) -> Result<(String), Error> {
        let res = octocrab.pulls(owner, repo).merge(pr_number).send().await?;
        return Ok(res.sha.unwrap());
    }

    pub async fn create_release(
        octocrab: &Octocrab,
        owner: &String,
        repo: &String,
        version: &String,
        merge_sha: &String,
    ) -> Result<(), Error> {
        octocrab
            .repos(owner, repo)
            .releases()
            .create(version)
            .target_commitish(merge_sha)
            .make_latest(octocrab::repos::releases::MakeLatest::True)
            .send()
            .await?;
        octocrab
            .repos(owner, repo)
            .releases()
            .generate_release_notes(&version)
            .send()
            .await?;
        return Ok(());
    }

    pub async fn create_version_branch(
        octocrab: &Octocrab,
        owner: &String,
        repo: &String,
        version: &String,
        sha: &String,
    ) -> Result<(), Error> {
        octocrab
            .repos(owner, repo)
            .create_ref(
                &octocrab::params::repos::Reference::Branch(version.clone()),
                sha,
            )
            .await?;
        return Ok(());
    }

    pub async fn get_all_repos<'a>(
        octocrab: &Octocrab,
        config: &'a Config,
        version: String,
    ) -> Result<(), Error> {
        for json_repo in &config.repositories {
            let files = get_root_file_list(octocrab, &json_repo.owner, &json_repo.repo).await?;
            let file_to_update = get_repo_with_file_to_update(&files, &version).unwrap();
            let pr_resullt = create_pr(
                octocrab,
                &json_repo.owner,
                &json_repo.repo,
                &config.pattern.title,
                &json_repo.origin,
                &json_repo.target,
                &config.pattern.body,
            )
            .await?;
            update_file_version(
                octocrab,
                &json_repo.owner,
                &json_repo.repo,
                &file_to_update.0.file_to_detect,
                &file_to_update.1,
                &file_to_update.2,
                &json_repo.origin,
            )
            .await?;
            let merge_result =
                merge_branch(octocrab, &json_repo.owner, &json_repo.repo, pr_resullt.0).await?;
            create_release(
                octocrab,
                &json_repo.owner,
                &json_repo.repo,
                &version,
                &merge_result,
            )
            .await?;
            create_version_branch(
                octocrab,
                &json_repo.owner,
                &json_repo.repo,
                &version,
                &pr_resullt.1,
            )
            .await?;
        }
        return Ok(());
    }
}
